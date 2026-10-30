//! RK4 test-ray tracer on the GPU: traces a batch of rays given by `(x, p)` and reads the results back. This is the
//! GPU side of the parity harness (`docs/PRD.md` §10 WP-07); the per-pixel tracer (WP-08) reuses the WGSL.

use bytemuck::{Pod, Zeroable};
use gravography_physics::{Kerr, TraceParams};
use wgpu::util::DeviceExt;

use super::GpuContext;
use super::shader::test_ray_shader;
use crate::RenderError;

/// Tracing parameters for the GPU (WGSL `TraceParams` in `ray.wgsl`; uniform, 64 bytes).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Pod, Zeroable)]
pub struct GpuTraceParams {
    /// Hole mass (1).
    pub mass: f32,
    /// Spin `a`.
    pub spin: f32,
    /// Outer horizon `r_+`, computed in f32 like the CPU f32 path (`Kerr::<f32>::outer_horizon`).
    pub r_plus: f32,
    /// See [`TraceParams::horizon_step_factor`].
    pub horizon_step_factor: f32,
    /// See [`TraceParams::far_step_factor`].
    pub far_step_factor: f32,
    /// See [`TraceParams::h_max`].
    pub h_max: f32,
    /// See [`TraceParams::escape_factor`].
    pub escape_factor: f32,
    /// See [`TraceParams::escape_min_radius`].
    pub escape_min_radius: f32,
    /// See [`TraceParams::horizon_epsilon`].
    pub horizon_epsilon: f32,
    /// See [`TraceParams::momentum_blowup`].
    pub momentum_blowup: f32,
    /// See [`TraceParams::r_singular`].
    pub r_singular: f32,
    /// See [`TraceParams::max_steps`].
    pub max_steps: u32,
    /// Number of rays in the batch (unused by the per-pixel pass).
    pub ray_count: u32,
    /// DOPRI5 relative tolerance.
    pub rtol: f32,
    /// DOPRI5 absolute tolerance.
    pub atol: f32,
    /// See [`TraceParams::h_min`].
    pub h_min: f32,
}

impl GpuTraceParams {
    /// Converts f32 tracing parameters (the integrator is chosen by the entry point, not by this struct).
    pub fn new(kerr: &Kerr<f32>, p: &TraceParams<f32>, ray_count: u32) -> Self {
        Self {
            mass: kerr.mass(),
            spin: kerr.spin(),
            r_plus: kerr.outer_horizon(),
            horizon_step_factor: p.horizon_step_factor,
            far_step_factor: p.far_step_factor,
            h_max: p.h_max,
            escape_factor: p.escape_factor,
            escape_min_radius: p.escape_min_radius,
            horizon_epsilon: p.horizon_epsilon,
            momentum_blowup: p.momentum_blowup,
            r_singular: p.r_singular,
            max_steps: p.max_steps,
            ray_count,
            rtol: p.rtol,
            atol: p.atol,
            h_min: p.h_min,
        }
    }
}

/// One input ray (WGSL `Ray`; 32 bytes): KS event and traced covariant momentum.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Pod, Zeroable)]
pub struct GpuRay {
    /// `[t, x, y, z]`.
    pub x: [f32; 4],
    /// `p_μ`.
    pub p: [f32; 4],
}

/// Result of one GPU ray (WGSL `RayResult`; 64 bytes).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Pod, Zeroable)]
pub struct GpuRayResult {
    /// Final event.
    pub x: [f32; 4],
    /// Final momentum.
    pub p: [f32; 4],
    /// Unit escape direction in `xyz` (zero unless escaped); `w` unused.
    pub dir: [f32; 4],
    /// Status code ([`crate::cpu::status_index`] order).
    pub status: u32,
    /// Accepted steps.
    pub steps: u32,
    /// Affine parameter at termination.
    pub lambda: f32,
    /// Rejected steps (DOPRI5).
    pub rejected: u32,
}

/// Right-hand side `(dx/dλ, dp/dλ)` evaluated on the GPU (WGSL `Phase`; 32 bytes).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Pod, Zeroable)]
pub struct GpuPhase {
    /// `dx^μ/dλ`.
    pub dx: [f32; 4],
    /// `dp_μ/dλ`.
    pub dp: [f32; 4],
}

/// Workgroup size of the test-ray entry points (`@workgroup_size(64)` in `test_rays.wgsl`).
pub const WORKGROUP_SIZE: u32 = 64;

/// Compute pipelines for the test-ray entry points.
pub struct TestRayPipeline {
    trace: wgpu::ComputePipeline,
    dopri5: wgpu::ComputePipeline,
    rhs: wgpu::ComputePipeline,
}

impl TestRayPipeline {
    /// Compiles the assembled test-ray shader.
    pub fn new(ctx: &GpuContext) -> Self {
        let module = ctx.device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("test_rays"),
            source: wgpu::ShaderSource::Wgsl(test_ray_shader().into()),
        });
        let make = |entry: &str| {
            ctx.device
                .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                    label: Some(entry),
                    layout: None,
                    module: &module,
                    entry_point: Some(entry),
                    compilation_options: Default::default(),
                    cache: None,
                })
        };
        Self {
            trace: make("trace_rk4_main"),
            dopri5: make("trace_dopri5_main"),
            rhs: make("eval_rhs_main"),
        }
    }

    /// Traces `rays` with RK4 and returns one result per ray.
    ///
    /// # Errors
    /// [`RenderError::Gpu`] on validation or readback failure.
    pub fn trace(
        &self,
        ctx: &GpuContext,
        kerr: &Kerr<f32>,
        params: &TraceParams<f32>,
        rays: &[GpuRay],
    ) -> Result<Vec<GpuRayResult>, RenderError> {
        self.run(ctx, &self.trace, 2, kerr, params, rays)
    }

    /// Traces `rays` with adaptive DOPRI5 (`params.rtol`, `params.atol`) and returns one result per ray.
    ///
    /// # Errors
    /// [`RenderError::Gpu`] on validation or readback failure.
    pub fn trace_dopri5(
        &self,
        ctx: &GpuContext,
        kerr: &Kerr<f32>,
        params: &TraceParams<f32>,
        rays: &[GpuRay],
    ) -> Result<Vec<GpuRayResult>, RenderError> {
        self.run(ctx, &self.dopri5, 2, kerr, params, rays)
    }

    /// Evaluates the Hamiltonian right-hand side at each `(x, p)`.
    ///
    /// # Errors
    /// [`RenderError::Gpu`] on validation or readback failure.
    pub fn eval_rhs(
        &self,
        ctx: &GpuContext,
        kerr: &Kerr<f32>,
        rays: &[GpuRay],
    ) -> Result<Vec<GpuPhase>, RenderError> {
        self.run(ctx, &self.rhs, 3, kerr, &TraceParams::cpu_reference(), rays)
    }

    /// Uploads `rays`, dispatches `pipeline` and reads back binding `out_binding` as `T`s.
    fn run<T: Pod>(
        &self,
        ctx: &GpuContext,
        pipeline: &wgpu::ComputePipeline,
        out_binding: u32,
        kerr: &Kerr<f32>,
        params: &TraceParams<f32>,
        rays: &[GpuRay],
    ) -> Result<Vec<T>, RenderError> {
        if rays.is_empty() {
            return Ok(Vec::new());
        }
        let count = u32::try_from(rays.len()).map_err(|_| RenderError::Gpu("too many rays".into()))?;
        let dev = &ctx.device;
        let uniform = dev.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("trace params"),
            contents: bytemuck::bytes_of(&GpuTraceParams::new(kerr, params, count)),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let input = dev.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("rays"),
            contents: bytemuck::cast_slice(rays),
            usage: wgpu::BufferUsages::STORAGE,
        });
        let out_size = (rays.len() * size_of::<T>()) as u64;
        let output = dev.create_buffer(&wgpu::BufferDescriptor {
            label: Some("results"),
            size: out_size,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let readback = dev.create_buffer(&wgpu::BufferDescriptor {
            label: Some("readback"),
            size: out_size,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        // The auto-derived layout only contains the bindings this entry point uses.
        let bind_group = dev.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("test rays"),
            layout: &pipeline.get_bind_group_layout(0),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: input.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: out_binding,
                    resource: output.as_entire_binding(),
                },
            ],
        });
        let mut encoder = dev.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("test rays"),
                timestamp_writes: None,
            });
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            pass.dispatch_workgroups(count.div_ceil(WORKGROUP_SIZE), 1, 1);
        }
        encoder.copy_buffer_to_buffer(&output, 0, &readback, 0, out_size);
        ctx.queue.submit([encoder.finish()]);
        read_buffer(ctx, &readback)
    }
}

/// Maps a `MAP_READ` buffer, blocking until the GPU is done, and copies it out.
pub(crate) fn read_buffer<T: Pod>(ctx: &GpuContext, buffer: &wgpu::Buffer) -> Result<Vec<T>, RenderError> {
    let slice = buffer.slice(..);
    let (tx, rx) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |r| {
        let _ = tx.send(r);
    });
    ctx.device
        .poll(wgpu::PollType::wait_indefinitely())
        .map_err(|e| RenderError::Gpu(format!("poll failed: {e}")))?;
    rx.recv()
        .map_err(|_| RenderError::Gpu("map callback dropped".into()))?
        .map_err(|e| RenderError::Gpu(format!("buffer map failed: {e}")))?;
    let data = {
        let view = slice
            .get_mapped_range()
            .map_err(|e| RenderError::Gpu(format!("mapped range: {e}")))?;
        bytemuck::cast_slice::<u8, T>(&view).to_vec()
    };
    buffer.unmap();
    Ok(data)
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::mem::offset_of;

    #[test]
    fn layouts_match_wgsl() {
        // WGSL layout rules: vec4<f32> has size 16 and alignment 16; structs round up to their alignment; uniform
        // structs must be 16-byte multiples. These match the WGSL structs in ray.wgsl / test_rays.wgsl /
        // geodesic.wgsl.
        assert_eq!(size_of::<GpuTraceParams>(), 64);
        assert_eq!(offset_of!(GpuTraceParams, max_steps), 44);
        assert_eq!(offset_of!(GpuTraceParams, ray_count), 48);
        assert_eq!(offset_of!(GpuTraceParams, h_min), 60);
        assert_eq!(size_of::<GpuRay>(), 32);
        assert_eq!(offset_of!(GpuRay, p), 16);
        assert_eq!(size_of::<GpuRayResult>(), 64);
        assert_eq!(offset_of!(GpuRayResult, dir), 32);
        assert_eq!(offset_of!(GpuRayResult, status), 48);
        assert_eq!(offset_of!(GpuRayResult, steps), 52);
        assert_eq!(offset_of!(GpuRayResult, lambda), 56);
        assert_eq!(size_of::<GpuPhase>(), 32);
        assert_eq!(offset_of!(GpuPhase, dp), 16);
    }

    /// `(size, [(member, offset)])` of WGSL struct `name` in the assembled test-ray shader, as laid out by naga.
    fn wgsl_layout(name: &str) -> (u32, Vec<(String, u32)>) {
        let module = wgpu::naga::front::wgsl::parse_str(&test_ray_shader()).unwrap();
        for (_, ty) in module.types.iter() {
            if ty.name.as_deref() == Some(name)
                && let wgpu::naga::TypeInner::Struct { members, span } = &ty.inner
            {
                let m = members
                    .iter()
                    .map(|m| (m.name.clone().unwrap_or_default(), m.offset))
                    .collect();
                return (*span, m);
            }
        }
        panic!("WGSL struct {name} not found");
    }

    #[test]
    fn layouts_match_naga() {
        // The naga front end computes WGSL host-shareable layouts; Rust sizes and field offsets must agree.
        let check = |name: &str, size: usize, offsets: &[(&str, usize)]| {
            let (span, members) = wgsl_layout(name);
            assert_eq!(span as usize, size, "{name} size");
            for (field, off) in offsets {
                let m = members.iter().find(|(n, _)| n == field).unwrap();
                assert_eq!(m.1 as usize, *off, "{name}.{field}");
            }
        };
        check(
            "TraceParams",
            size_of::<GpuTraceParams>(),
            &[
                ("mass", offset_of!(GpuTraceParams, mass)),
                ("r_plus", offset_of!(GpuTraceParams, r_plus)),
                ("momentum_blowup", offset_of!(GpuTraceParams, momentum_blowup)),
                ("r_singular", offset_of!(GpuTraceParams, r_singular)),
                ("max_steps", offset_of!(GpuTraceParams, max_steps)),
                ("ray_count", offset_of!(GpuTraceParams, ray_count)),
                ("rtol", offset_of!(GpuTraceParams, rtol)),
                ("atol", offset_of!(GpuTraceParams, atol)),
                ("h_min", offset_of!(GpuTraceParams, h_min)),
            ],
        );
        check(
            "Ray",
            size_of::<GpuRay>(),
            &[("x", 0), ("p", offset_of!(GpuRay, p))],
        );
        check(
            "RayResult",
            size_of::<GpuRayResult>(),
            &[
                ("p", offset_of!(GpuRayResult, p)),
                ("dir", offset_of!(GpuRayResult, dir)),
                ("status", offset_of!(GpuRayResult, status)),
                ("steps", offset_of!(GpuRayResult, steps)),
                ("lambda", offset_of!(GpuRayResult, lambda)),
                ("rejected", offset_of!(GpuRayResult, rejected)),
            ],
        );
        check(
            "Phase",
            size_of::<GpuPhase>(),
            &[("x", 0), ("p", offset_of!(GpuPhase, dp))],
        );
    }

    #[test]
    fn params_conversion() {
        let k = Kerr::new(0.9_f32).unwrap();
        let p = GpuTraceParams::new(&k, &TraceParams::cpu_reference(), 7);
        assert_eq!(p.spin, 0.9);
        assert_eq!(p.r_plus, k.outer_horizon());
        assert_eq!(p.max_steps, 100_000);
        assert_eq!(p.ray_count, 7);
    }
}
