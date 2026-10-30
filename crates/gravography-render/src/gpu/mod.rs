//! wgpu backend (`docs/PRD.md` §4.2–4.3). WP-07: headless device, WGSL assembly and an RK4 test-ray tracer used by
//! the GPU ↔ CPU parity harness ([`crate::parity`]). The per-pixel renderer follows in WP-08/09.
//!
//! The GPU computes in f32 only (Apple GPUs have no f64). WGSL functions are ports of the Rust physics with the
//! same names and structure (`CLAUDE.md` §5).

pub mod context;
pub mod shader;
pub mod test_rays;

pub use context::GpuContext;
pub use test_rays::{GpuPhase, GpuRay, GpuRayResult, GpuTraceParams, TestRayPipeline};
