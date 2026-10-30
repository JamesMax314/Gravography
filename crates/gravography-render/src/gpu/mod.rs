//! wgpu backend (`docs/PRD.md` §4.2–4.3): headless device, WGSL assembly, test-ray tracers (RK4, DOPRI5) for the
//! GPU ↔ CPU parity harness ([`crate::parity`]), and the per-pixel DOPRI5 trace pass writing the G-buffer
//! ([`tracer`]), and the shading / accumulation / tone-map / debug passes ([`renderer`]).
//!
//! The GPU computes in f32 only (Apple GPUs have no f64). WGSL functions are ports of the Rust physics with the
//! same names and structure (`CLAUDE.md` §5).

pub mod context;
pub mod present;
pub mod renderer;
pub mod shader;
pub mod stats;
pub mod test_rays;
pub mod timer;
pub mod tracer;

pub use context::GpuContext;
pub use present::Presenter;
pub use renderer::{DebugView, GpuFrame, GpuRenderer, GpuSky};
pub use stats::{FrameStats, GpuStats};
pub use test_rays::{GpuPhase, GpuRay, GpuRayResult, GpuTraceParams, TestRayPipeline};
pub use timer::GpuTimer;
pub use tracer::{GBuffer, GBufferTexel, GpuCamera, GpuTracer};
