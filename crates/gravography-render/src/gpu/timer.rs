//! GPU frame timing (`docs/PRD.md` WP-12) without stalling the frame loop. Each timed sample records when its
//! work was submitted and when the queue reported it complete (`Queue::on_submitted_work_done`, in submission
//! order). Its GPU busy time is `done(N) − max(submitted(N), done(N−1))`: the work starts when it is submitted or
//! when the previous timed work finishes, whichever is later. Plain submission-to-completion would include time
//! spent queued behind earlier frames when the CPU runs ahead (seen as 33 ms "GPU time" at 115 fps). Pass timestamp queries were tried first (PRD: "timestamp queries if available") but on wgpu 30 /
//! Metal the end-of-pass timestamps of later frames came back stale or zero (progress report, WP-12).

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use super::GpuContext;

/// Most samples in flight that are tracked (older ones are dropped).
const MAX_PENDING: usize = 8;

type Done = Arc<Mutex<Option<Instant>>>;

/// A completed measurement: milliseconds and the caller's tag.
pub type Timing = (f64, f32);

/// Submission-to-completion timer.
#[derive(Default)]
pub struct GpuTimer {
    started: Option<Instant>,
    pending: VecDeque<(Instant, f32, Done)>,
    last_done: Option<Instant>,
}

/// GPU busy time in milliseconds of work submitted at `submitted` and completed at `done`, when the previous timed
/// work completed at `prev_done`.
pub fn busy_ms(submitted: Instant, done: Instant, prev_done: Option<Instant>) -> f64 {
    let start = prev_done.map_or(submitted, |p| p.max(submitted));
    done.saturating_duration_since(start).as_secs_f64() * 1e3
}

impl GpuTimer {
    /// A timer with nothing in flight.
    pub fn new() -> Self {
        Self::default()
    }

    /// Call just before submitting the work to time.
    pub fn begin(&mut self) {
        self.started = Some(Instant::now());
    }

    /// Call right after submitting it: registers a completion callback for everything submitted so far. `tag` is
    /// returned with the measurement (results arrive a frame or more later), e.g. the render scale used.
    pub fn finish(&mut self, ctx: &GpuContext, tag: f32) {
        let Some(start) = self.started.take() else {
            return;
        };
        let done: Done = Arc::new(Mutex::new(None));
        let d = done.clone();
        ctx.queue.on_submitted_work_done(move || {
            if let Ok(mut slot) = d.lock() {
                *slot = Some(Instant::now());
            }
        });
        if self.pending.len() == MAX_PENDING {
            // Dropped entries lose their completion time; forget it so the next one is not under-counted.
            self.pending.pop_front();
            self.last_done = None;
        }
        self.pending.push_back((start, tag, done));
    }

    /// Non-blocking: returns the most recent completed measurement, if any completed since the last call.
    pub fn poll(&mut self, ctx: &GpuContext) -> Option<Timing> {
        let _ = ctx.device.poll(wgpu::PollType::Poll);
        self.collect()
    }

    /// Like [`GpuTimer::poll`], but waits for the GPU first (tests and benchmarks).
    pub fn wait(&mut self, ctx: &GpuContext) -> Option<Timing> {
        let _ = ctx.device.poll(wgpu::PollType::wait_indefinitely());
        self.collect()
    }

    fn collect(&mut self) -> Option<Timing> {
        let mut latest = None;
        while let Some((start, tag, done)) = self.pending.front() {
            let end = done.lock().ok().and_then(|g| *g);
            let Some(end) = end else { break };
            latest = Some((busy_ms(*start, end, self.last_done), *tag));
            self.last_done = Some(end);
            self.pending.pop_front();
        }
        latest
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn busy_time_excludes_queueing() {
        let t0 = Instant::now();
        let ms = |m: u64| t0 + Duration::from_millis(m);
        // Idle GPU: submitted at 0, done at 10 → 10 ms.
        assert!((busy_ms(ms(0), ms(10), None) - 10.0).abs() < 1e-9);
        // CPU ran ahead: submitted at 2, but the previous work finished at 10; done at 18 → 8 ms, not 16.
        assert!((busy_ms(ms(2), ms(18), Some(ms(10))) - 8.0).abs() < 1e-9);
        // Previous work finished before this was submitted: counted from submission.
        assert!((busy_ms(ms(20), ms(25), Some(ms(10))) - 5.0).abs() < 1e-9);
        // Clock oddities never give negative times.
        assert_eq!(busy_ms(ms(5), ms(3), None), 0.0);
    }
}
