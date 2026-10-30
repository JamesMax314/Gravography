//! wgpu backend (`docs/PRD.md` §4.2–4.3): headless device, WGSL assembly, test-ray tracers (RK4, DOPRI5) for the
//! GPU ↔ CPU parity harness ([`crate::parity`]), and the per-pixel DOPRI5 trace pass writing the G-buffer
//! ([`tracer`]). Shading follows in WP-09.
//!
//! The GPU computes in f32 only (Apple GPUs have no f64). WGSL functions are ports of the Rust physics with the
//! same names and structure (`CLAUDE.md` §5).

pub mod context;
pub mod shader;
pub mod test_rays;
pub mod tracer;

pub use context::GpuContext;
pub use test_rays::{GpuPhase, GpuRay, GpuRayResult, GpuTraceParams, TestRayPipeline};
pub use tracer::{GBuffer, GBufferTexel, GpuCamera, GpuTracer};
