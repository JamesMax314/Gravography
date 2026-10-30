//! GPU renderer (`docs/PRD.md` §4.3 steps 2–4, headless): per sample, the trace pass ([`GpuTracer`]) writes the
//! G-buffer, the shading pass adds the sample's linear sRGB to an accumulation buffer, and the tone-map pass turns
//! the accumulated HDR image into 8-bit sRGB. The debug pass renders FR-R6 views from the G-buffer.

use bytemuck::{Pod, Zeroable};
use gravography_physics::{RayStatus, TraceParams};
use wgpu::util::DeviceExt;

use super::GpuContext;
use super::shader::{debug_shader, shade_shader, tonemap_shader};
use super::test_rays::read_buffer;
use super::tracer::{GBuffer, GBufferTexel, GpuCamera, GpuTracer};
use crate::RenderError;
use crate::cpu::{RenderStats, sample_offset, status_from_index, status_index};
use crate::frame::FrameParams;
use crate::output::HdrImage;
use crate::sky::Sky;
use crate::tonemap::ToneMap;

/// Effect flag: intensity beaming (WGSL `FLAG_BEAMING`).
pub const FLAG_BEAMING: u32 = 4;

/// Shading uniform (WGSL `ShadeParams` in `sky.wgsl`; 80 bytes).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Pod, Zeroable)]
pub struct GpuShadeParams {
    /// Rows of the XYZ → linear sRGB matrix (`w` unused).
    pub xyz_to_rgb: [[f32; 4]; 3],
    /// LUT temperature range (K).
    pub lut_t_min: f32,
    /// See `lut_t_min`.
    pub lut_t_max: f32,
    /// Number of LUT entries.
    pub lut_n: u32,
    /// Cube-map face size of level 0.
    pub base_size: u32,
    /// Number of mip levels.
    pub mip_count: u32,
    /// Samples per pixel of the whole render (footprint per sample = pixel / samples).
    pub samples: u32,
    /// [`FLAG_BEAMING`].
    pub flags: u32,
    /// Index of the current sample (0 overwrites the accumulation buffer).
    pub sample_index: u32,
}

/// Tone-map uniform (WGSL `ToneParams`; 16 bytes).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Pod, Zeroable)]
pub struct GpuToneParams {
    /// Exposure scale.
    pub scale: f32,
    /// 0 = ACES fitted, 1 = extended Reinhard.
    pub op: u32,
    /// Number of pixels.
    pub pixel_count: u32,
    /// Padding.
    pub _pad: u32,
}

/// Debug-view uniform (WGSL `DebugParams`; 16 bytes).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Pod, Zeroable)]
pub struct GpuDebugParams {
    /// [`DebugView`] code.
    pub mode: u32,
    /// Image width.
    pub width: u32,
    /// Image height.
    pub height: u32,
    /// Step budget (normalises the heat map).
    pub max_steps: u32,
}

/// FR-R6 debug views.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DebugView {
    /// Ray status: escaped blue, captured black, singular yellow, max-steps red, error magenta.
    Status = 0,
    /// Frequency shift `g`, false colour: red (redshift, `g ≤ 1/4`) → white (`g = 1`) → blue (`g ≥ 4`);
    /// non-escaped pixels dark grey.
    Shift = 1,
    /// Integration steps (accepted + rejected), logarithmic heat map from black (0) to pale yellow (budget).
    Steps = 2,
}

/// The sky on the GPU: the `(w, w·T)` cube map as a 6-layer RG32Float 2D-array texture with all mips, and the
/// band LUT as a storage buffer.
pub struct GpuSky {
    view: wgpu::TextureView,
    lut: wgpu::Buffer,
    base_size: u32,
    mip_count: u32,
    lut_n: u32,
    lut_range: (f32, f32),
    /// Band display matrix (channels → linear sRGB), rows.
    display: [[f32; 4]; 3],
}

impl GpuSky {
    /// Uploads `sky`.
    ///
    /// # Errors
    /// [`RenderError::Gpu`] if the cube map exceeds the device's texture limits or the LUT is too short.
    pub fn upload(ctx: &GpuContext, sky: &Sky) -> Result<Self, RenderError> {
        let base = sky.cube.base_size();
        let limit = ctx.device.limits().max_texture_dimension_2d as usize;
        if base > limit || !base.is_power_of_two() {
            return Err(RenderError::Gpu(format!(
                "cube-map face size {base} must be a power of two ≤ the device limit {limit}"
            )));
        }
        if sky.lut.entries.len() < 2 {
            return Err(RenderError::Gpu("the LUT needs at least 2 entries".into()));
        }
        let base_u32 = base as u32;
        let mip_count = sky.cube.levels.len() as u32;
        let texture = ctx.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("sky cube (w, wT)"),
            size: wgpu::Extent3d {
                width: base_u32,
                height: base_u32,
                depth_or_array_layers: 6,
            },
            mip_level_count: mip_count,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rg32Float,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        for (level, data) in sky.cube.levels.iter().enumerate() {
            // CubeLevel data is face-major, then row-major: exactly the layer/row layout of a 2D-array texture.
            let n = data.size as u32;
            ctx.queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &texture,
                    mip_level: level as u32,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                bytemuck::cast_slice(&data.data),
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(n * 8),
                    rows_per_image: Some(n),
                },
                wgpu::Extent3d {
                    width: n,
                    height: n,
                    depth_or_array_layers: 6,
                },
            );
        }
        let view = texture.create_view(&wgpu::TextureViewDescriptor {
            dimension: Some(wgpu::TextureViewDimension::D2Array),
            ..Default::default()
        });
        let lut = ctx.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("blackbody LUT"),
            contents: bytemuck::cast_slice(&sky.lut.entries),
            usage: wgpu::BufferUsages::STORAGE,
        });
        Ok(Self {
            view,
            lut,
            base_size: base_u32,
            mip_count,
            lut_n: sky.lut.entries.len() as u32,
            lut_range: (sky.lut.t_min as f32, sky.lut.t_max as f32),
            display: display_rows(&sky.lut),
        })
    }

    /// Replaces the band table (switching observing bands without re-uploading the cube map).
    ///
    /// # Errors
    /// [`RenderError::Gpu`] if the table has fewer than 2 entries.
    pub fn set_lut(
        &mut self,
        ctx: &GpuContext,
        lut: &gravography_sky::BlackbodyLut,
    ) -> Result<(), RenderError> {
        if lut.entries.len() < 2 {
            return Err(RenderError::Gpu("the LUT needs at least 2 entries".into()));
        }
        self.lut = ctx.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("blackbody LUT"),
            contents: bytemuck::cast_slice(&lut.entries),
            usage: wgpu::BufferUsages::STORAGE,
        });
        self.lut_n = lut.entries.len() as u32;
        self.lut_range = (lut.t_min as f32, lut.t_max as f32);
        self.display = display_rows(lut);
        Ok(())
    }
}

fn display_rows(lut: &gravography_sky::BlackbodyLut) -> [[f32; 4]; 3] {
    let m = lut.display;
    core::array::from_fn(|i| [m[i][0] as f32, m[i][1] as f32, m[i][2] as f32, 0.0])
}

/// The result of a GPU render that stays on the GPU: the G-buffer of the last sample and the accumulation buffer
/// (`rgb` sum and sample count per pixel).
pub struct GpuFrame {
    /// G-buffer of the last traced sample.
    pub gbuffer: GBuffer,
    /// Accumulation buffer, `vec4<f32>` per pixel.
    pub accum: wgpu::Buffer,
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// Step budget used for the trace (for the steps debug view).
    pub max_steps: u32,
}

impl GpuFrame {
    /// Allocates the G-buffer and accumulation buffer for a `width × height` image. They persist across frames:
    /// the viewer adds one sample per frame with [`GpuRenderer::render_sample`] (progressive refinement).
    pub fn new(ctx: &GpuContext, width: u32, height: u32, max_steps: u32) -> Self {
        let accum = ctx.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("accumulation"),
            size: u64::from(width) * u64::from(height) * 16,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        Self {
            gbuffer: GBuffer::new(ctx, width, height),
            accum,
            width,
            height,
            max_steps,
        }
    }

    fn pixels(&self) -> u32 {
        self.width * self.height
    }
}

/// Trace, shade, tone-map and debug pipelines.
pub struct GpuRenderer {
    /// The per-pixel tracer (its `tile` controls dispatch tiling).
    pub tracer: GpuTracer,
    shade: wgpu::ComputePipeline,
    shade_layout: wgpu::BindGroupLayout,
    tonemap: wgpu::ComputePipeline,
    debug: wgpu::ComputePipeline,
}

fn storage_entry(binding: u32, read_only: bool) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Storage { read_only },
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

fn uniform_entry(binding: u32) -> wgpu::BindGroupLayoutEntry {
    wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::COMPUTE,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    }
}

fn compute_pipeline(
    ctx: &GpuContext,
    label: &str,
    source: String,
    layout: Option<&wgpu::PipelineLayout>,
) -> wgpu::ComputePipeline {
    let module = ctx.device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some(label),
        source: wgpu::ShaderSource::Wgsl(source.into()),
    });
    ctx.device
        .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some(label),
            layout,
            module: &module,
            entry_point: None,
            compilation_options: Default::default(),
            cache: None,
        })
}

impl GpuRenderer {
    /// Compiles all pipelines.
    pub fn new(ctx: &GpuContext) -> Self {
        // Explicit layout for the shading pass: the RG32Float cube map is read with textureLoad only, so it is bound
        // as a non-filterable float texture (no FLOAT32_FILTERABLE feature needed).
        let shade_layout = ctx
            .device
            .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("shade"),
                entries: &[
                    uniform_entry(0),
                    uniform_entry(1),
                    storage_entry(2, true),
                    wgpu::BindGroupLayoutEntry {
                        binding: 3,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: false },
                            view_dimension: wgpu::TextureViewDimension::D2Array,
                            multisampled: false,
                        },
                        count: None,
                    },
                    storage_entry(4, true),
                    storage_entry(5, false),
                ],
            });
        let pipeline_layout = ctx
            .device
            .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("shade"),
                bind_group_layouts: &[Some(&shade_layout)],
                immediate_size: 0,
            });
        Self {
            tracer: GpuTracer::new(ctx),
            shade: compute_pipeline(ctx, "shade", shade_shader(), Some(&pipeline_layout)),
            shade_layout,
            tonemap: compute_pipeline(ctx, "tonemap", tonemap_shader(), None),
            debug: compute_pipeline(ctx, "debug", debug_shader(), None),
        }
    }

    /// Renders all `frame.samples` samples (jittered with [`sample_offset`], as on the CPU) into a new
    /// accumulation buffer. Returns after submitting; reading results waits for the GPU.
    ///
    /// # Errors
    /// Invalid frame parameters.
    pub fn render(
        &self,
        ctx: &GpuContext,
        frame: &FrameParams,
        sky: &GpuSky,
        params: &TraceParams<f32>,
    ) -> Result<GpuFrame, RenderError> {
        frame.validate()?;
        let target = GpuFrame::new(ctx, frame.width, frame.height, params.max_steps);
        for s in 0..frame.samples {
            self.render_sample(ctx, frame, sky, params, &target, s)?;
        }
        Ok(target)
    }

    /// Traces and shades sample `sample_index` (jitter [`sample_offset`]`(sample_index)`) into `target`. Sample 0
    /// resets the accumulation; each sample's footprint is `1/frame.samples` of a pixel. Returns after submitting.
    ///
    /// # Errors
    /// Invalid frame parameters, or a target whose size differs from the frame.
    pub fn render_sample(
        &self,
        ctx: &GpuContext,
        frame: &FrameParams,
        sky: &GpuSky,
        params: &TraceParams<f32>,
        target: &GpuFrame,
        sample_index: u32,
    ) -> Result<(), RenderError> {
        let dev = &ctx.device;
        let jitter = sample_offset(sample_index);
        self.tracer.trace(ctx, frame, params, jitter, &target.gbuffer)?;
        let shade = GpuShadeParams {
            xyz_to_rgb: sky.display,
            lut_t_min: sky.lut_range.0,
            lut_t_max: sky.lut_range.1,
            lut_n: sky.lut_n,
            base_size: sky.base_size,
            mip_count: sky.mip_count,
            samples: frame.samples,
            flags: if frame.effects.beaming { FLAG_BEAMING } else { 0 },
            sample_index,
        };
        let uniform = |label, bytes: &[u8]| {
            dev.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some(label),
                contents: bytes,
                usage: wgpu::BufferUsages::UNIFORM,
            })
        };
        let shade_buf = uniform("shade params", bytemuck::bytes_of(&shade));
        let camera_buf = uniform("camera", bytemuck::bytes_of(&GpuCamera::new(frame, jitter)));
        let bind_group = dev.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("shade"),
            layout: &self.shade_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: shade_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: camera_buf.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: target.gbuffer.buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: wgpu::BindingResource::TextureView(&sky.view),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: sky.lut.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 5,
                    resource: target.accum.as_entire_binding(),
                },
            ],
        });
        let mut encoder = dev.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("shade"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.shade);
            pass.set_bind_group(0, &bind_group, &[]);
            pass.dispatch_workgroups(frame.width.div_ceil(8), frame.height.div_ceil(8), 1);
        }
        ctx.queue.submit([encoder.finish()]);
        Ok(())
    }

    /// Reads the accumulated HDR image (mean linear sRGB per pixel).
    ///
    /// # Errors
    /// [`RenderError::Gpu`] on readback failure.
    pub fn read_hdr(&self, ctx: &GpuContext, frame: &GpuFrame) -> Result<HdrImage, RenderError> {
        let acc: Vec<[f32; 4]> = read_storage(ctx, &frame.accum)?;
        Ok(HdrImage {
            width: frame.width,
            height: frame.height,
            pixels: acc
                .iter()
                .map(|a| {
                    let n = a[3].max(1.0);
                    [a[0] / n, a[1] / n, a[2] / n]
                })
                .collect(),
        })
    }

    /// Ray-status counts of the last sample's G-buffer (in [`RenderStats::status`] order) and step totals.
    ///
    /// # Errors
    /// [`RenderError::Gpu`] on readback failure.
    pub fn read_stats(&self, ctx: &GpuContext, frame: &GpuFrame) -> Result<RenderStats, RenderError> {
        let texels: Vec<GBufferTexel> = frame.gbuffer.read(ctx)?;
        let mut stats = RenderStats::default();
        for t in &texels {
            let s = status_from_index(t.status).unwrap_or(RayStatus::Error);
            stats.status[status_index(s)] += 1;
            let steps = t.steps + t.rejected;
            stats.rays += 1;
            stats.steps += u64::from(steps);
            stats.max_steps = stats.max_steps.max(steps);
        }
        Ok(stats)
    }

    /// Exposure, tone mapping and sRGB encoding on the GPU: 8-bit RGB, row-major.
    ///
    /// # Errors
    /// [`RenderError::Gpu`] on readback failure.
    pub fn tonemap(
        &self,
        ctx: &GpuContext,
        frame: &GpuFrame,
        scale: f64,
        op: ToneMap,
    ) -> Result<Vec<u8>, RenderError> {
        let params = GpuToneParams {
            scale: scale as f32,
            op: match op {
                ToneMap::Aces => 0,
                ToneMap::Reinhard => 1,
            },
            pixel_count: frame.pixels(),
            _pad: 0,
        };
        self.run_rgba8(
            ctx,
            &self.tonemap,
            bytemuck::bytes_of(&params),
            &frame.accum,
            frame.pixels(),
        )
    }

    /// Renders an FR-R6 debug view of the last sample's G-buffer: 8-bit RGB, row-major.
    ///
    /// # Errors
    /// [`RenderError::Gpu`] on readback failure.
    pub fn debug_view(
        &self,
        ctx: &GpuContext,
        frame: &GpuFrame,
        view: DebugView,
    ) -> Result<Vec<u8>, RenderError> {
        self.debug_view_of(ctx, &frame.gbuffer, view, frame.max_steps)
    }

    /// [`GpuRenderer::debug_view`] for any G-buffer.
    ///
    /// # Errors
    /// [`RenderError::Gpu`] on readback failure.
    pub fn debug_view_of(
        &self,
        ctx: &GpuContext,
        gbuffer: &GBuffer,
        view: DebugView,
        max_steps: u32,
    ) -> Result<Vec<u8>, RenderError> {
        let params = GpuDebugParams {
            mode: view as u32,
            width: gbuffer.width,
            height: gbuffer.height,
            max_steps: max_steps.max(1),
        };
        let n = gbuffer.width * gbuffer.height;
        self.run_rgba8(ctx, &self.debug, bytemuck::bytes_of(&params), &gbuffer.buffer, n)
    }

    /// Runs a 64-wide `(uniform, input storage) → packed RGBA8` pass and returns RGB bytes.
    fn run_rgba8(
        &self,
        ctx: &GpuContext,
        pipeline: &wgpu::ComputePipeline,
        params: &[u8],
        input: &wgpu::Buffer,
        pixels: u32,
    ) -> Result<Vec<u8>, RenderError> {
        let dev = &ctx.device;
        let uniform = dev.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("rgba8 params"),
            contents: params,
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let out = dev.create_buffer(&wgpu::BufferDescriptor {
            label: Some("rgba8"),
            size: u64::from(pixels) * 4,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let bind_group = dev.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("rgba8"),
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
                    binding: 2,
                    resource: out.as_entire_binding(),
                },
            ],
        });
        let mut encoder = dev.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("rgba8"),
                timestamp_writes: None,
            });
            pass.set_pipeline(pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            pass.dispatch_workgroups(pixels.div_ceil(64), 1, 1);
        }
        ctx.queue.submit([encoder.finish()]);
        let packed: Vec<u32> = read_storage(ctx, &out)?;
        Ok(packed
            .iter()
            .flat_map(|p| [*p as u8, (p >> 8) as u8, (p >> 16) as u8])
            .collect())
    }
}

/// Copies a `STORAGE | COPY_SRC` buffer to the CPU.
fn read_storage<T: Pod>(ctx: &GpuContext, buffer: &wgpu::Buffer) -> Result<Vec<T>, RenderError> {
    let size = buffer.size();
    let readback = ctx.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("readback"),
        size,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let mut encoder = ctx
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
    encoder.copy_buffer_to_buffer(buffer, 0, &readback, 0, size);
    ctx.queue.submit([encoder.finish()]);
    read_buffer(ctx, &readback)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gpu::shader::shade_shader;
    use core::mem::offset_of;

    fn naga_struct(src: &str, name: &str) -> (u32, Vec<(String, u32)>) {
        let module = wgpu::naga::front::wgsl::parse_str(src).unwrap();
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
        let check = |src: &str, name: &str, size: usize, offsets: &[(&str, usize)]| {
            let (span, members) = naga_struct(src, name);
            assert_eq!(span as usize, size, "{name} size");
            for (field, off) in offsets {
                let m = members.iter().find(|(n, _)| n == field).unwrap();
                assert_eq!(m.1 as usize, *off, "{name}.{field}");
            }
        };
        check(
            &shade_shader(),
            "ShadeParams",
            size_of::<GpuShadeParams>(),
            &[
                ("xyz_to_rgb2", offset_of!(GpuShadeParams, xyz_to_rgb) + 32),
                ("lut_t_min", offset_of!(GpuShadeParams, lut_t_min)),
                ("base_size", offset_of!(GpuShadeParams, base_size)),
                ("samples", offset_of!(GpuShadeParams, samples)),
                ("sample_index", offset_of!(GpuShadeParams, sample_index)),
            ],
        );
        check(
            &tonemap_shader(),
            "ToneParams",
            size_of::<GpuToneParams>(),
            &[("op", 4), ("pixel_count", offset_of!(GpuToneParams, pixel_count))],
        );
        check(
            &debug_shader(),
            "DebugParams",
            size_of::<GpuDebugParams>(),
            &[("max_steps", offset_of!(GpuDebugParams, max_steps))],
        );
        assert_eq!(size_of::<GpuShadeParams>(), 80);
    }
}
