//! GPU frame timing (`docs/PRD.md` WP-12) without stalling the frame loop.
//!
//! Each timed sample records when its work was submitted. A helper thread then blocks on that submission's
//! completion (`Device::poll` with `PollType::Wait { submission_index }`) and timestamps the moment it wakes. The
//! sample's GPU busy time is `done(N) − max(submitted(N), done(N−1))`: the work starts when it is submitted or when
//! the previous timed work finishes, whichever is later. That excludes time spent queued behind earlier frames
//! when the CPU runs ahead.
//!
//! History:
//! - Pass timestamp queries (the PRD's first choice) returned zero or stale end-of-pass values on wgpu 30 / Metal.
//! - `Queue::on_submitted_work_done` callbacks only fire inside `poll`, so with one poll per frame their times
//!   were quantised to the frame interval. The HUD read "GPU 0.0 ms" when several completed in one poll.
//!
//! The waiting thread gives completion times to within its wake-up latency (≈ 0.1 ms).

use std::collections::VecDeque;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{Sender, channel};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Instant;

use super::GpuContext;

/// Most samples in flight that are timed (further ones are not timed until earlier ones complete).
const MAX_PENDING: usize = 8;

/// A completed measurement: milliseconds and the caller's tag.
pub type Timing = (f64, f32);

type Done = (Instant, f32, Instant);

/// GPU busy time in milliseconds of work submitted at `submitted` and completed at `done`, when the previous timed
/// work completed at `prev_done`.
pub fn busy_ms(submitted: Instant, done: Instant, prev_done: Option<Instant>) -> f64 {
    let start = prev_done.map_or(submitted, |p| p.max(submitted));
    done.saturating_duration_since(start).as_secs_f64() * 1e3
}

/// Asynchronous GPU timer with a completion-waiting helper thread.
pub struct GpuTimer {
    started: Option<Instant>,
    tx: Option<Sender<(Instant, f32, wgpu::SubmissionIndex)>>,
    done: Arc<Mutex<VecDeque<Done>>>,
    in_flight: Arc<AtomicUsize>,
    last_done: Option<Instant>,
    thread: Option<JoinHandle<()>>,
}

impl GpuTimer {
    /// Starts the helper thread for `ctx`'s device.
    pub fn new(ctx: &GpuContext) -> Self {
        let (tx, rx) = channel::<(Instant, f32, wgpu::SubmissionIndex)>();
        let done = Arc::new(Mutex::new(VecDeque::new()));
        let in_flight = Arc::new(AtomicUsize::new(0));
        let device = ctx.device.clone();
        let (d, f) = (done.clone(), in_flight.clone());
        let thread = std::thread::Builder::new()
            .name("gpu-timer".into())
            .spawn(move || {
                for (start, tag, index) in rx {
                    let _ = device.poll(wgpu::PollType::Wait {
                        submission_index: Some(index),
                        timeout: None,
                    });
                    let end = Instant::now();
                    if let Ok(mut q) = d.lock() {
                        q.push_back((start, tag, end));
                    }
                    f.fetch_sub(1, Ordering::AcqRel);
                }
            })
            .ok();
        Self {
            started: None,
            tx: thread.is_some().then_some(tx),
            done,
            in_flight,
            last_done: None,
            thread,
        }
    }

    /// Call just before submitting the work to time.
    pub fn begin(&mut self) {
        self.started = Some(Instant::now());
    }

    /// Call right after submitting it. `tag` is returned with the measurement (results arrive a frame or more
    /// later), e.g. the render scale used.
    pub fn finish(&mut self, ctx: &GpuContext, tag: f32) {
        let (Some(start), Some(tx)) = (self.started.take(), self.tx.as_ref()) else {
            return;
        };
        if self.in_flight.load(Ordering::Acquire) >= MAX_PENDING {
            // The helper is behind: skip timing this sample, and forget the previous completion so the next timed
            // sample is not under-counted.
            self.last_done = None;
            return;
        }
        // An empty submission completes after everything submitted before it: the sample's work.
        let index = ctx.queue.submit([]);
        self.in_flight.fetch_add(1, Ordering::AcqRel);
        if tx.send((start, tag, index)).is_err() {
            self.in_flight.fetch_sub(1, Ordering::AcqRel);
        }
    }

    /// Non-blocking: the most recent completed measurement, if any completed since the last call.
    pub fn poll(&mut self, _ctx: &GpuContext) -> Option<Timing> {
        let mut latest = None;
        let drained: Vec<Done> = self
            .done
            .lock()
            .map(|mut q| q.drain(..).collect())
            .unwrap_or_default();
        for (start, tag, end) in drained {
            latest = Some((busy_ms(start, end, self.last_done), tag));
            self.last_done = Some(end);
        }
        latest
    }

    /// Like [`GpuTimer::poll`], but first waits until every timed sample has completed (tests and benchmarks).
    pub fn wait(&mut self, ctx: &GpuContext) -> Option<Timing> {
        let _ = ctx.device.poll(wgpu::PollType::wait_indefinitely());
        while self.in_flight.load(Ordering::Acquire) > 0 {
            std::thread::yield_now();
        }
        self.poll(ctx)
    }
}

impl Drop for GpuTimer {
    fn drop(&mut self) {
        // Closing the channel ends the helper's loop.
        self.tx = None;
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
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
