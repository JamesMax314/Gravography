//! Per-pixel GPU ray tracer (`docs/PRD.md` §4.3 step 2): one compute invocation per pixel builds the camera ray,
//! integrates it with DOPRI5 in f32 and writes the G-buffer `{escape direction, g, status, steps}`. The image is
//! dispatched in square tiles, each in its own command buffer, so that no single submission runs long enough to
//! trip a GPU watchdog.

use bytemuck::{Pod, Zeroable};
use gravography_physics::{Kerr, TraceParams};
use wgpu::util::DeviceExt;

use super::GpuContext;
use super::shader::trace_pixel_shader;
use super::test_rays::{GpuTraceParams, read_buffer};
use crate::RenderError;
use crate::camera::Projection;
use crate::frame::FrameParams;

/// Effect flag: lensing (WGSL `FLAG_LENSING`).
pub const FLAG_LENSING: u32 = 1;
/// Effect flag: frequency shift (WGSL `FLAG_FREQUENCY_SHIFT`).
pub const FLAG_FREQUENCY_SHIFT: u32 = 2;

/// Camera uniform (WGSL `Camera` in `camera.wgsl`; 112 bytes).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Pod, Zeroable)]
pub struct GpuCamera {
    /// KS event `[t, x, y, z]`.
    pub position: [f32; 4],
    /// Tetrad legs `e_(a)^μ` (u, forward, left, up).
    pub e: [[f32; 4]; 4],
    /// Image width.
    pub width: u32,
    /// Image height.
    pub height: u32,
    /// `tan(fov/2)` of the rectilinear projection.
    pub tan_half_fov: f32,
    /// [`FLAG_LENSING`] | [`FLAG_FREQUENCY_SHIFT`].
    pub flags: u32,
    /// Sub-pixel sample offset in `[0, 1)²`.
    pub jitter: [f32; 2],
    /// [`PROJECTION_RECTILINEAR`] or [`PROJECTION_EQUIRECTANGULAR`].
    pub projection: u32,
    /// Padding.
    pub _pad: u32,
}

/// Projection code: rectilinear (WGSL `PROJECTION_RECTILINEAR`).
pub const PROJECTION_RECTILINEAR: u32 = 0;
/// Projection code: equirectangular (WGSL `PROJECTION_EQUIRECTANGULAR`).
pub const PROJECTION_EQUIRECTANGULAR: u32 = 1;

impl GpuCamera {
    /// Camera uniform for `frame` with sample offset `jitter` (pixel `(i, j)` is traced at `(i, j) + jitter`).
    pub fn new(frame: &FrameParams, jitter: (f64, f64)) -> Self {
        let (projection, fov_deg) = match frame.projection {
            Projection::Rectilinear { fov_deg } => (PROJECTION_RECTILINEAR, fov_deg),
            Projection::Equirectangular => (PROJECTION_EQUIRECTANGULAR, 90.0),
        };
        let mut flags = 0;
        if frame.effects.lensing {
            flags |= FLAG_LENSING;
        }
        if frame.effects.frequency_shift {
            flags |= FLAG_FREQUENCY_SHIFT;
        }
        Self {
            position: frame.position.map(|c| c as f32),
            e: frame.tetrad.e_up.map(|leg| leg.map(|c| c as f32)),
            width: frame.width,
            height: frame.height,
            tan_half_fov: (0.5 * fov_deg.to_radians()).tan() as f32,
            flags,
            jitter: [jitter.0 as f32, jitter.1 as f32],
            projection,
            _pad: 0,
        }
    }
}

/// One dispatch tile (WGSL `Tile`; 16 bytes).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Pod, Zeroable)]
pub struct GpuTile {
    /// Top-left pixel.
    pub origin: [u32; 2],
    /// Size in pixels.
    pub size: [u32; 2],
}

/// One G-buffer texel (WGSL `GBufferTexel`; 32 bytes).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Pod, Zeroable)]
pub struct GBufferTexel {
    /// Unit escape direction in `xyz` (zero unless escaped) and `g = 1/p_t` in `w`.
    pub dir_g: [f32; 4],
    /// Status code ([`crate::cpu::status_index`] order).
    pub status: u32,
    /// Accepted steps.
    pub steps: u32,
    /// Rejected steps.
    pub rejected: u32,
    /// Padding.
    pub _pad: u32,
}

/// The G-buffer: one [`GBufferTexel`] per pixel, row-major, in a GPU storage buffer.
pub struct GBuffer {
    /// Storage buffer (`STORAGE | COPY_SRC`).
    pub buffer: wgpu::Buffer,
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
}

impl GBuffer {
    /// Allocates a `width × height` G-buffer.
    pub fn new(ctx: &GpuContext, width: u32, height: u32) -> Self {
        let buffer = ctx.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("g-buffer"),
            size: u64::from(width) * u64::from(height) * size_of::<GBufferTexel>() as u64,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        Self {
            buffer,
            width,
            height,
        }
    }

    /// Copies the G-buffer back to the CPU (blocks until the GPU is done).
    ///
    /// # Errors
    /// [`RenderError::Gpu`] on map failure.
    pub fn read(&self, ctx: &GpuContext) -> Result<Vec<GBufferTexel>, RenderError> {
        let size = self.buffer.size();
        let readback = ctx.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("g-buffer readback"),
            size,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let mut encoder = ctx
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        encoder.copy_buffer_to_buffer(&self.buffer, 0, &readback, 0, size);
        ctx.queue.submit([encoder.finish()]);
        read_buffer(ctx, &readback)
    }
}

/// Default tile side in pixels: 1024² ≈ 10⁶ rays per submission. After the WP-12 speed-ups a whole 1920×1080
/// frame of the hardest scene takes ≈ 55 ms on an Apple M2 Pro, far below the macOS GPU watchdog (seconds); fewer
/// submissions saved ≈ 10 % per frame compared with 256² tiles (progress report, WP-12).
pub const DEFAULT_TILE: u32 = 1024;

/// Workgroup size of `trace_pixels_main` (`@workgroup_size(8, 8)`).
pub const WORKGROUP_SIDE: u32 = 8;

/// The per-pixel trace pipeline.
pub struct GpuTracer {
    pipeline: wgpu::ComputePipeline,
    /// Tile side in pixels.
    pub tile: u32,
}

impl GpuTracer {
    /// Compiles the trace shader.
    pub fn new(ctx: &GpuContext) -> Self {
        let module = ctx.device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("trace_pixels"),
            source: wgpu::ShaderSource::Wgsl(trace_pixel_shader().into()),
        });
        let pipeline = ctx
            .device
            .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some("trace_pixels_main"),
                layout: None,
                module: &module,
                entry_point: Some("trace_pixels_main"),
                compilation_options: Default::default(),
                cache: None,
            });
        Self {
            pipeline,
            tile: DEFAULT_TILE,
        }
    }

    /// Traces every pixel of `frame` (one sample at offset `jitter`) into `gbuffer`, tile by tile. Returns after
    /// submitting; [`GBuffer::read`] (or later passes) wait for completion.
    ///
    /// # Errors
    /// [`RenderError::InvalidScene`] if the frame is invalid or the G-buffer has the wrong size.
    pub fn trace(
        &self,
        ctx: &GpuContext,
        frame: &FrameParams,
        params: &TraceParams<f32>,
        jitter: (f64, f64),
        gbuffer: &GBuffer,
    ) -> Result<(), RenderError> {
        frame.validate()?;
        if (gbuffer.width, gbuffer.height) != (frame.width, frame.height) {
            return Err(RenderError::InvalidScene(format!(
                "G-buffer is {}×{} but the frame is {}×{}",
                gbuffer.width, gbuffer.height, frame.width, frame.height
            )));
        }
        let kerr = Kerr::new(frame.kerr.spin() as f32)?;
        let dev = &ctx.device;
        let uniform = |label, bytes: &[u8]| {
            dev.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some(label),
                contents: bytes,
                usage: wgpu::BufferUsages::UNIFORM,
            })
        };
        let params_buf = uniform(
            "trace params",
            bytemuck::bytes_of(&GpuTraceParams::new(&kerr, params, 0)),
        );
        let camera_buf = uniform("camera", bytemuck::bytes_of(&GpuCamera::new(frame, jitter)));
        let layout = self.pipeline.get_bind_group_layout(0);
        let tile = self.tile.max(WORKGROUP_SIDE);
        for ty in (0..frame.height).step_by(tile as usize) {
            for tx in (0..frame.width).step_by(tile as usize) {
                let t = GpuTile {
                    origin: [tx, ty],
                    size: [tile.min(frame.width - tx), tile.min(frame.height - ty)],
                };
                let tile_buf = uniform("tile", bytemuck::bytes_of(&t));
                let bind_group = dev.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("trace pixels"),
                    layout: &layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: params_buf.as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: camera_buf.as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 2,
                            resource: tile_buf.as_entire_binding(),
                        },
                        wgpu::BindGroupEntry {
                            binding: 3,
                            resource: gbuffer.buffer.as_entire_binding(),
                        },
                    ],
                });
                let mut encoder = dev.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
                {
                    let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                        label: Some("trace tile"),
                        timestamp_writes: None,
                    });
                    pass.set_pipeline(&self.pipeline);
                    pass.set_bind_group(0, &bind_group, &[]);
                    pass.dispatch_workgroups(
                        t.size[0].div_ceil(WORKGROUP_SIDE),
                        t.size[1].div_ceil(WORKGROUP_SIDE),
                        1,
                    );
                }
                // One submission per tile: each command buffer stays short (watchdog).
                ctx.queue.submit([encoder.finish()]);
            }
        }
        Ok(())
    }

    /// Convenience: allocates a G-buffer, traces `frame` at pixel centres and reads the result back.
    ///
    /// # Errors
    /// As [`GpuTracer::trace`] and [`GBuffer::read`].
    pub fn trace_to_cpu(
        &self,
        ctx: &GpuContext,
        frame: &FrameParams,
        params: &TraceParams<f32>,
    ) -> Result<Vec<GBufferTexel>, RenderError> {
        let gbuffer = GBuffer::new(ctx, frame.width, frame.height);
        self.trace(ctx, frame, params, (0.5, 0.5), &gbuffer)?;
        gbuffer.read(ctx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gpu::shader::trace_pixel_shader;
    use core::mem::offset_of;

    fn naga_struct(name: &str) -> (u32, Vec<(String, u32)>) {
        let module = wgpu::naga::front::wgsl::parse_str(&trace_pixel_shader()).unwrap();
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
        let check = |name: &str, size: usize, offsets: &[(&str, usize)]| {
            let (span, members) = naga_struct(name);
            assert_eq!(span as usize, size, "{name} size");
            for (field, off) in offsets {
                let m = members.iter().find(|(n, _)| n == field).unwrap();
                assert_eq!(m.1 as usize, *off, "{name}.{field}");
            }
        };
        assert_eq!(size_of::<GpuCamera>(), 112);
        check(
            "Camera",
            size_of::<GpuCamera>(),
            &[
                ("e0", offset_of!(GpuCamera, e)),
                ("e3", offset_of!(GpuCamera, e) + 48),
                ("width", offset_of!(GpuCamera, width)),
                ("tan_half_fov", offset_of!(GpuCamera, tan_half_fov)),
                ("flags", offset_of!(GpuCamera, flags)),
                ("jitter", offset_of!(GpuCamera, jitter)),
                ("projection", offset_of!(GpuCamera, projection)),
            ],
        );
        check(
            "Tile",
            size_of::<GpuTile>(),
            &[("size", offset_of!(GpuTile, size))],
        );
        check(
            "GBufferTexel",
            size_of::<GBufferTexel>(),
            &[
                ("status", offset_of!(GBufferTexel, status)),
                ("steps", offset_of!(GBufferTexel, steps)),
                ("rejected", offset_of!(GBufferTexel, rejected)),
            ],
        );
        check(
            "TraceParams",
            size_of::<GpuTraceParams>(),
            &[("h_min", offset_of!(GpuTraceParams, h_min))],
        );
    }
}
