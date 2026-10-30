//! Rendering backends for Gravography (`docs/PRD.md` §4.2–4.3).
//!
//! - [`scene`]: TOML scene files (`scenes/*.toml`) → [`FrameParams`] + sky parameters.
//! - [`frame`]: the backend-agnostic frame description.
//! - [`cpu`]: the f64 reference renderer (rayon), ground truth for GPU parity (WP-07 onwards adds `gpu`).
//! - [`sky`], [`camera`], [`tonemap`], [`output`]: shading, projection, exposure/tone mapping and image files.
//!
//! WGSL shaders will live in `src/gpu/shaders/`.

pub mod camera;
pub mod cpu;
pub mod error;
pub mod frame;
pub mod output;
pub mod scene;
pub mod sky;
pub mod tonemap;

pub use camera::Projection;
pub use cpu::{CpuRenderer, CpuSettings, RenderStats};
pub use error::RenderError;
pub use frame::{Effects, FrameParams};
pub use output::{HdrImage, OutputFormat, write_image};
pub use scene::Scene;
pub use sky::Sky;
pub use tonemap::{Exposure, ToneMap};

/// Exposure scale for displaying `image` rendered with `frame` (auto exposure uses the image's log-average
/// luminance; manual exposure is relative to [`Sky::reference_luminance`]).
pub fn display_scale(frame: &FrameParams, image: &HdrImage, sky: &Sky) -> f64 {
    let avg = frame
        .exposure
        .auto
        .then(|| tonemap::log_average_luminance(&image.pixels))
        .flatten();
    frame.exposure.scale(avg, sky.reference_luminance())
}
