//! Viewer state that does not touch the GPU: display settings, internal resolution and progressive accumulation.

use gravography_render::{Exposure, Scene, ToneMap};

/// Smallest and largest render scale (internal resolution / window resolution).
pub const RENDER_SCALE_RANGE: (f32, f32) = (0.25, 1.0);
/// Largest progressive sample target selectable in the panel.
pub const MAX_SAMPLE_TARGET: u32 = 256;

/// User-adjustable render settings (egui panel).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ViewSettings {
    /// Progressive sample target per pixel (accumulation stops there).
    pub samples: u32,
    /// Internal resolution as a fraction of the window's physical resolution.
    pub render_scale: f32,
    /// Tone-mapping operator.
    pub tone_map: ToneMap,
    /// Exposure.
    pub exposure: Exposure,
}

impl ViewSettings {
    /// Settings from a scene file. The scene's sample count becomes the progressive target (at least 16 so a
    /// static view converges to a clean image). Render scale 0.5 keeps a Retina window interactive until
    /// WP-12 adds dynamic resolution.
    pub fn from_scene(scene: &Scene) -> Self {
        Self {
            samples: scene.camera.samples.clamp(16, MAX_SAMPLE_TARGET),
            render_scale: 0.5,
            tone_map: scene.render.tone_map,
            exposure: scene.render.exposure,
        }
    }

    /// Whether changing from `self` to `new` invalidates the accumulated image (exposure and tone map do not: they
    /// are applied when presenting).
    pub fn resets_accumulation(&self, new: &Self) -> bool {
        self.samples != new.samples || self.render_scale.to_bits() != new.render_scale.to_bits()
    }
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

    /// Whether auto exposure should be re-metered after the sample just recorded: after the first sample (a quick
    /// estimate) and at convergence (the final value), to avoid a GPU readback every frame.
    pub fn meter_exposure_now(&self) -> bool {
        self.done == 1 || self.converged()
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
        assert_eq!(a.next_sample(), Some(0));
        a.advance();
        assert!(a.meter_exposure_now());
        assert_eq!(a.next_sample(), Some(1));
        a.advance();
        assert!(!a.meter_exposure_now());
        a.advance();
        assert!(a.converged() && a.meter_exposure_now());
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
        assert!(!s.resets_accumulation(&t));
        t.render_scale = 0.75;
        assert!(s.resets_accumulation(&t));
        let mut u = s;
        u.samples = 64;
        assert!(s.resets_accumulation(&u));
    }
}
