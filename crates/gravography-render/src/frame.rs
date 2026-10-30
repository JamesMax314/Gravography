//! Backend-agnostic frame description (`docs/PRD.md` §4.2): everything a backend needs to render one image.
//! Built from a [`crate::Scene`] (headless) or, from WP-10 on, by the viewer each frame.

use gravography_physics::{Kerr, Tetrad, Vec4};
use serde::{Deserialize, Serialize};

use crate::RenderError;
use crate::camera::Projection;
use crate::tonemap::{Exposure, ToneMap};

/// Pedagogical effect toggles (`docs/PRD.md` §6.4). All on = physically correct.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Effects {
    /// Gravitational lensing. Off: rays go straight along the spatial part of the camera ray `k`, so aberration
    /// from the observer's motion remains but no bending occurs.
    pub lensing: bool,
    /// Frequency shift. Off: `g = 1` (no colour or intensity change).
    pub frequency_shift: bool,
    /// Intensity beaming. Off: keep the colour from `g T` but divide by `g⁴`, so the bolometric brightness equals
    /// the unshifted (`g = 1`) value.
    pub beaming: bool,
}

impl Default for Effects {
    fn default() -> Self {
        Self {
            lensing: true,
            frequency_shift: true,
            beaming: true,
        }
    }
}

/// One frame to render.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FrameParams {
    /// The black hole (`M = 1`).
    pub kerr: Kerr<f64>,
    /// Camera event (KS `[t, x, y, z]`).
    pub position: Vec4<f64>,
    /// Camera tetrad (`e_(0) = u`; forward, left, up), with the look rotation already applied.
    pub tetrad: Tetrad<f64>,
    /// Image width in pixels.
    pub width: u32,
    /// Image height in pixels.
    pub height: u32,
    /// Projection.
    pub projection: Projection,
    /// Samples per pixel (jittered, averaged).
    pub samples: u32,
    /// Effect toggles.
    pub effects: Effects,
    /// Exposure.
    pub exposure: Exposure,
    /// Tone-mapping operator.
    pub tone_map: ToneMap,
}

/// Largest accepted image side (keeps buffers well below memory limits).
pub const MAX_IMAGE_SIDE: u32 = 16_384;
/// Largest accepted number of samples per pixel.
pub const MAX_SAMPLES: u32 = 4096;

impl FrameParams {
    /// Checks sizes, sample count and projection.
    ///
    /// # Errors
    /// [`RenderError::InvalidScene`] describing the first invalid value.
    pub fn validate(&self) -> Result<(), RenderError> {
        let bad = |m: String| Err(RenderError::InvalidScene(m));
        if !(1..=MAX_IMAGE_SIDE).contains(&self.width) || !(1..=MAX_IMAGE_SIDE).contains(&self.height) {
            return bad(format!(
                "image size {}×{} must be within 1..={MAX_IMAGE_SIDE}",
                self.width, self.height
            ));
        }
        if !(1..=MAX_SAMPLES).contains(&self.samples) {
            return bad(format!(
                "samples must be within 1..={MAX_SAMPLES}, got {}",
                self.samples
            ));
        }
        if !self.exposure.ev.is_finite() {
            return bad("exposure ev must be finite".into());
        }
        self.projection.validate().map_err(RenderError::InvalidScene)
    }
}
