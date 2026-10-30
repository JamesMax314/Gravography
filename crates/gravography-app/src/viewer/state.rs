//! Viewer state that does not touch the GPU: display settings, internal resolution and progressive accumulation.

use std::time::Duration;

use gravography_render::{Effects, Exposure, Scene, ToneMap};

/// Smallest and largest render scale (internal resolution / window resolution).
pub const RENDER_SCALE_RANGE: (f32, f32) = (0.25, 1.0);
/// Largest progressive sample target selectable in the panel.
pub const MAX_SAMPLE_TARGET: u32 = 256;

/// User-adjustable render settings (egui panel).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ViewSettings {
    /// Progressive sample target per pixel (accumulation stops there).
    pub samples: u32,
    /// Internal resolution as a fraction of the window's physical resolution while the camera is still (the view
    /// converges at this scale). While moving, [`DynamicResolution`] may lower it.
    pub render_scale: f32,
    /// Lower the resolution while the camera moves to hold [`ViewSettings::target_fps`] (FR-R3).
    pub dynamic_resolution: bool,
    /// Frame-rate target of the dynamic resolution.
    pub target_fps: f32,
    /// Tone-mapping operator.
    pub tone_map: ToneMap,
    /// Exposure.
    pub exposure: Exposure,
    /// Pedagogical effect toggles (`docs/PRD.md` §6.4).
    pub effects: Effects,
}

/// Shortest interval between exposure/statistics meterings while the camera keeps moving. Since WP-12 a metering
/// is a small GPU reduction read back asynchronously (`gravography_render::gpu::GpuStats`), so this only limits
/// how quickly auto exposure follows the view.
pub const METER_INTERVAL: Duration = Duration::from_millis(250);

impl ViewSettings {
    /// Settings from a scene file. The scene's sample count becomes the progressive target (at least 16 so a
    /// static view converges to a clean image). Full resolution when still, dynamic resolution at 60 fps while
    /// moving.
    pub fn from_scene(scene: &Scene) -> Self {
        Self {
            samples: scene.camera.samples.clamp(16, MAX_SAMPLE_TARGET),
            render_scale: 1.0,
            dynamic_resolution: true,
            target_fps: 60.0,
            tone_map: scene.render.tone_map,
            exposure: scene.render.exposure,
            effects: scene.render.effects,
        }
    }

    /// Whether changing from `self` to `new` invalidates the accumulated image (exposure and tone map do not: they
    /// are applied when presenting).
    pub fn resets_accumulation(&self, new: &Self) -> bool {
        self.samples != new.samples
            || self.render_scale.to_bits() != new.render_scale.to_bits()
            || self.effects != new.effects
    }
}

/// Fewest internal pixels used while moving: 960 × 540, i.e. 1080p at scale 0.5 (`docs/PRD.md` NF-1: "internal
/// scale may drop to ≥ 0.5" at 1080p). Expressed as a pixel count so the floor means the same on Retina windows.
pub const MIN_MOVING_PIXELS: f64 = 960.0 * 540.0;
/// The camera counts as moving for this long after its last change (bridges frames without input).
pub const MOVE_HOLD: Duration = Duration::from_millis(120);
/// A window resize takes effect once the size has been stable this long (WP-12; full-screen animations deliver a
/// stream of sizes).
pub const RESIZE_SETTLE: Duration = Duration::from_millis(150);
/// Share of the frame time given to the GPU work of a sample (the rest: present, egui, CPU).
pub const GPU_BUDGET_FRACTION: f64 = 0.8;

/// Smallest moving-camera scale for a window of `window` physical pixels: keeps at least [`MIN_MOVING_PIXELS`]
/// internal pixels, and never below the render-scale range.
pub fn min_moving_scale(window: (u32, u32)) -> f32 {
    let px = f64::from(window.0.max(1)) * f64::from(window.1.max(1));
    ((MIN_MOVING_PIXELS / px).sqrt() as f32).clamp(RENDER_SCALE_RANGE.0, RENDER_SCALE_RANGE.1)
}

/// GPU time budget per sample in milliseconds for a frame-rate target.
pub fn gpu_budget_ms(target_fps: f32) -> f64 {
    1e3 / f64::from(target_fps.max(1.0)) * GPU_BUDGET_FRACTION
}

/// Internal render size for a window of `window` physical pixels at `scale` (each side at least 1 pixel).
pub fn internal_size(window: (u32, u32), scale: f32) -> (u32, u32) {
    let s = scale.clamp(RENDER_SCALE_RANGE.0, RENDER_SCALE_RANGE.1);
    let side = |n: u32| ((n as f32 * s).round() as u32).max(1);
    (side(window.0), side(window.1))
}

/// Progressive accumulation bookkeeping for a fixed camera.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Accumulation {
    /// Samples accumulated so far.
    pub done: u32,
    /// Target samples per pixel.
    pub target: u32,
}

impl Accumulation {
    /// Empty accumulation towards `target` samples.
    pub fn new(target: u32) -> Self {
        Self {
            done: 0,
            target: target.max(1),
        }
    }

    /// Index of the next sample to render, or `None` when converged.
    pub fn next_sample(&self) -> Option<u32> {
        (self.done < self.target).then_some(self.done)
    }

    /// Records one rendered sample.
    pub fn advance(&mut self) {
        self.done = (self.done + 1).min(self.target);
    }

    /// Whether the target is reached.
    pub fn converged(&self) -> bool {
        self.done >= self.target
    }

    /// Whether auto exposure should be re-metered after the sample just recorded, given the time since the last
    /// metering (`None`: never metered). Always at convergence (the final value); after the first sample only if
    /// at least [`METER_INTERVAL`] has passed, so a moving camera (which restarts every frame) does not read the
    /// image back every frame.
    pub fn meter_exposure_now(&self, since_last: Option<Duration>) -> bool {
        self.converged() || (self.done == 1 && since_last.is_none_or(|d| d >= METER_INTERVAL))
    }
}

/// Exposure/statistics metering schedule. A metering becomes due when [`Accumulation::meter_exposure_now`] says
/// so, and stays due until a request is accepted. The GPU readback slots may all be busy at that moment, and the
/// metering at convergence must not be lost: a still view would otherwise keep the exposure and ray rates of the
/// motion before it (WP-12 bug, seen as 81% "escaped" rays inside the horizon).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Metering {
    due: bool,
    last: Option<std::time::Instant>,
}

impl Metering {
    /// Records the sample just rendered.
    pub fn after_sample(&mut self, accum: &Accumulation, now: std::time::Instant) {
        if accum.meter_exposure_now(self.last.map(|t| now.saturating_duration_since(t))) {
            self.due = true;
        }
    }

    /// If a metering is due, tries `request` (returns whether the GPU accepted it); clears the due flag on success.
    pub fn try_request(&mut self, now: std::time::Instant, request: impl FnOnce() -> bool) {
        if self.due && request() {
            self.due = false;
            self.last = Some(now);
        }
    }

    /// Whether a metering is waiting for a free slot.
    pub fn due(&self) -> bool {
        self.due
    }

    /// Forgets the last metering time, so the next first sample meters at once (new size or settings).
    pub fn reset(&mut self) {
        self.last = None;
    }
}

/// Dynamic resolution while the camera moves (`docs/PRD.md` FR-R3, NF-1). GPU time per sample is roughly
/// proportional to the pixel count, `t ∝ s²`, so the scale that meets the budget is `s · sqrt(t_target / t)`. The
/// controller moves halfway there (geometrically) per measurement, at most ×/÷ [`DynamicResolution::MAX_STEP`],
/// clamps to `[min_scale, max_scale]` and quantises to multiples of [`DynamicResolution::QUANTUM`], so small
/// fluctuations do not reallocate the frame buffers every frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DynamicResolution {
    scale: f32,
}

impl DynamicResolution {
    /// Scale quantum (also the hysteresis: changes smaller than this are ignored).
    pub const QUANTUM: f32 = 1.0 / 32.0;
    /// Largest scale change factor per measurement.
    pub const MAX_STEP: f64 = 1.2;

    /// Starts at `scale`.
    pub fn new(scale: f32) -> Self {
        Self { scale }
    }

    /// Current moving-camera scale.
    pub fn scale(&self) -> f32 {
        self.scale
    }

    /// Updates the scale from a GPU time `gpu_ms` measured at render scale `measured_scale`, aiming at
    /// `target_ms`, within `[min_scale, max_scale]`. Non-finite or non-positive inputs are ignored. Returns
    /// whether the scale changed.
    pub fn observe(
        &mut self,
        gpu_ms: f64,
        measured_scale: f32,
        target_ms: f64,
        min_scale: f32,
        max_scale: f32,
    ) -> bool {
        let (lo, hi) = (min_scale.min(max_scale), max_scale);
        if !(gpu_ms.is_finite() && gpu_ms > 0.0 && target_ms > 0.0 && measured_scale > 0.0) {
            return self.clamp_to(lo, hi);
        }
        let ideal = f64::from(measured_scale) * (target_ms / gpu_ms).sqrt();
        let current = f64::from(self.scale);
        let step = (ideal / current)
            .sqrt()
            .clamp(1.0 / Self::MAX_STEP, Self::MAX_STEP);
        let wanted = (current * step) as f32;
        let q = (wanted / Self::QUANTUM).round() * Self::QUANTUM;
        let next = q.clamp(lo, hi);
        let changed = (next - self.scale).abs() >= Self::QUANTUM * 0.5;
        if changed {
            self.scale = next;
        }
        changed | self.clamp_to(lo, hi)
    }

    fn clamp_to(&mut self, lo: f32, hi: f32) -> bool {
        let c = self.scale.clamp(lo, hi);
        let changed = c.to_bits() != self.scale.to_bits();
        self.scale = c;
        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn internal_size_scales_and_clamps() {
        assert_eq!(internal_size((2560, 1600), 0.5), (1280, 800));
        assert_eq!(internal_size((2560, 1600), 1.0), (2560, 1600));
        assert_eq!(internal_size((2560, 1600), 2.0), (2560, 1600));
        assert_eq!(internal_size((100, 3), 0.1), (25, 1));
        assert_eq!(internal_size((0, 0), 0.5), (1, 1));
    }

    #[test]
    fn accumulation_progresses_and_meters() {
        let mut a = Accumulation::new(3);
        let recent = Some(Duration::from_millis(100));
        assert_eq!(a.next_sample(), Some(0));
        a.advance();
        assert!(a.meter_exposure_now(None));
        assert!(a.meter_exposure_now(Some(METER_INTERVAL)));
        assert!(!a.meter_exposure_now(recent), "throttled while moving");
        assert_eq!(a.next_sample(), Some(1));
        a.advance();
        assert!(!a.meter_exposure_now(None));
        a.advance();
        assert!(a.converged() && a.meter_exposure_now(recent));
        assert_eq!(a.next_sample(), None);
        a.advance();
        assert_eq!(a.done, 3);
        assert_eq!(Accumulation::new(0).target, 1);
    }

    #[test]
    fn only_image_settings_reset() {
        let s = ViewSettings::from_scene(&Scene::default());
        assert_eq!(s.samples, 16);
        let mut t = s;
        t.exposure.ev = 2.0;
        t.tone_map = ToneMap::Reinhard;
        t.dynamic_resolution = false;
        t.target_fps = 30.0;
        assert!(!s.resets_accumulation(&t));
        t.render_scale = 0.75;
        assert!(s.resets_accumulation(&t));
        let mut u = s;
        u.samples = 64;
        assert!(s.resets_accumulation(&u));
        let mut e = s;
        e.effects.lensing = false;
        assert!(s.resets_accumulation(&e));
    }

    #[test]
    fn moving_floor_and_budget() {
        // 1080p: the PRD's 0.5. A 2560×1440 Retina window: 0.375 (the same 960×540 pixels). Tiny windows: full.
        assert!((min_moving_scale((1920, 1080)) - 0.5).abs() < 1e-6);
        assert!((min_moving_scale((2560, 1440)) - 0.375).abs() < 1e-6);
        assert_eq!(min_moving_scale((640, 360)), 1.0);
        assert_eq!(min_moving_scale((0, 0)), 1.0);
        assert!((gpu_budget_ms(60.0) - 1e3 / 60.0 * GPU_BUDGET_FRACTION).abs() < 1e-12);
        assert!(gpu_budget_ms(0.0).is_finite());
    }

    #[test]
    fn dynamic_resolution_converges_to_the_budget() {
        // A GPU whose time is t = 40 ms · s² (≈ 10 ms at s = 0.5): with a 10 ms target the controller settles at
        // s = 0.5 within a quantum, from above and from below, without oscillating.
        let gpu = |s: f32| 40.0 * f64::from(s) * f64::from(s);
        for start in [1.0, 0.25] {
            let mut d = DynamicResolution::new(start);
            let mut history = Vec::new();
            for _ in 0..40 {
                let s = d.scale();
                d.observe(gpu(s), s, 10.0, 0.25, 1.0);
                history.push(d.scale());
            }
            let last = &history[30..];
            assert!(
                last.iter()
                    .all(|&s| (s - 0.5).abs() <= DynamicResolution::QUANTUM),
                "{history:?}"
            );
            assert!(
                last.windows(2).all(|w| w[0].to_bits() == w[1].to_bits()),
                "settled: {history:?}"
            );
        }
    }

    #[test]
    fn dynamic_resolution_respects_limits_and_ignores_garbage() {
        let mut d = DynamicResolution::new(0.75);
        // Much too slow: steps down by at most MAX_STEP per measurement, then stops at the minimum.
        d.observe(1000.0, 0.75, 10.0, 0.5, 1.0);
        assert!(d.scale() >= (0.75 / DynamicResolution::MAX_STEP as f32) - DynamicResolution::QUANTUM);
        for _ in 0..20 {
            d.observe(1000.0, d.scale(), 10.0, 0.5, 1.0);
        }
        assert_eq!(d.scale(), 0.5);
        // Very fast: climbs to the maximum (the still-camera scale).
        for _ in 0..20 {
            d.observe(0.1, d.scale(), 10.0, 0.5, 0.875);
        }
        assert_eq!(d.scale(), 0.875);
        // Garbage measurements change nothing, except clamping into new limits.
        assert!(!d.observe(f64::NAN, 0.875, 10.0, 0.5, 0.875));
        assert!(!d.observe(-1.0, 0.875, 10.0, 0.5, 0.875));
        assert!(d.observe(f64::NAN, 0.875, 10.0, 0.5, 0.75));
        assert_eq!(d.scale(), 0.75);
        // Small fluctuations around the target do not change the scale (hysteresis).
        let mut d = DynamicResolution::new(0.5);
        assert!(!d.observe(10.4, 0.5, 10.0, 0.25, 1.0));
        assert!(!d.observe(9.6, 0.5, 10.0, 0.25, 1.0));
    }

    #[test]
    fn a_refused_metering_at_convergence_is_retried() {
        use std::time::{Duration, Instant};
        let t0 = Instant::now();
        let mut m = Metering::default();
        let mut a = Accumulation::new(2);
        // First sample: due, and accepted.
        a.advance();
        m.after_sample(&a, t0);
        let mut calls = 0;
        m.try_request(t0, || {
            calls += 1;
            true
        });
        assert!(!m.due() && calls == 1);
        // Convergence 10 ms later: due again (always at convergence), but every readback slot is busy.
        a.advance();
        let t1 = t0 + Duration::from_millis(10);
        m.after_sample(&a, t1);
        m.try_request(t1, || false);
        assert!(m.due(), "the converged metering must not be dropped");
        // No more samples are rendered after convergence; the next frame's retry succeeds.
        m.try_request(t1 + Duration::from_millis(16), || true);
        assert!(!m.due());
        // Moving view (first samples within METER_INTERVAL): throttled, nothing due.
        let mut moving = Accumulation::new(16);
        moving.advance();
        m.after_sample(&moving, t1 + Duration::from_millis(20));
        assert!(!m.due());
        m.reset();
        m.after_sample(&moving, t1 + Duration::from_millis(21));
        assert!(m.due(), "after a reset the first sample meters at once");
    }
}
