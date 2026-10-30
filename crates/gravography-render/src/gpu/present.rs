//! Present pass (`docs/PRD.md` §4.3 step 4): tone-maps the accumulation buffer of a [`GpuFrame`] straight into a
//! render target (the window surface), stretched over a destination rectangle. Exposure and the operator are the
//! same as the headless tone map (`tonemap_ops.wgsl`).

use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

use super::GpuContext;
use super::renderer::GpuFrame;
use super::shader::present_shader;
use crate::tonemap::ToneMap;

/// Present uniform (WGSL `PresentParams` in `present.wgsl`; 48 bytes).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Pod, Zeroable)]
pub struct GpuPresentParams {
    /// Exposure scale.
    pub scale: f32,
    /// 0 = ACES fitted, 1 = extended Reinhard.
    pub op: u32,
    /// 1 = apply the sRGB transfer function in the shader (non-sRGB target format).
    pub encode_srgb: u32,
    /// Padding.
    pub _pad0: u32,
    /// Internal image size.
    pub src_size: [u32; 2],
    /// Padding.
    pub _pad1: [u32; 2],
    /// Destination rectangle origin (framebuffer pixels).
    pub dst_origin: [f32; 2],
    /// Destination rectangle size (framebuffer pixels).
    pub dst_size: [f32; 2],
}

/// Tone-map operator code shared by the tone-map and present shaders.
pub fn tone_map_code(op: ToneMap) -> u32 {
    match op {
        ToneMap::Aces => 0,
        ToneMap::Reinhard => 1,
    }
}

/// Render pipeline drawing a [`GpuFrame`] into a colour target of a fixed format.
pub struct Presenter {
    pipeline: wgpu::RenderPipeline,
    encode_srgb: bool,
}

impl Presenter {
    /// Builds the pipeline for colour targets of `format`. For `*Srgb` formats the hardware encodes; otherwise the
    /// shader applies the sRGB transfer function.
    pub fn new(ctx: &GpuContext, format: wgpu::TextureFormat) -> Self {
        let module = ctx.device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("present"),
            source: wgpu::ShaderSource::Wgsl(present_shader().into()),
        });
        let pipeline = ctx
            .device
            .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("present"),
                layout: None,
                vertex: wgpu::VertexState {
                    module: &module,
                    entry_point: Some("present_vs"),
                    compilation_options: Default::default(),
                    buffers: &[],
                },
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &module,
                    entry_point: Some("present_fs"),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                multiview_mask: None,
                cache: None,
            });
        Self {
            pipeline,
            encode_srgb: !format.is_srgb(),
        }
    }

    /// Records the present draw into `pass`: `frame` tone-mapped with exposure `scale` and operator `op`, stretched
    /// over `dst = (x, y, w, h)` framebuffer pixels (black outside).
    pub fn draw(
        &self,
        ctx: &GpuContext,
        pass: &mut wgpu::RenderPass<'_>,
        frame: &GpuFrame,
        scale: f64,
        op: ToneMap,
        dst: [f32; 4],
    ) {
        let params = GpuPresentParams {
            scale: scale as f32,
            op: tone_map_code(op),
            encode_srgb: u32::from(self.encode_srgb),
            _pad0: 0,
            src_size: [frame.width, frame.height],
            _pad1: [0; 2],
            dst_origin: [dst[0], dst[1]],
            dst_size: [dst[2].max(1.0), dst[3].max(1.0)],
        };
        let uniform = ctx.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("present params"),
            contents: bytemuck::bytes_of(&params),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let bind_group = ctx.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("present"),
            layout: &self.pipeline.get_bind_group_layout(0),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: frame.accum.as_entire_binding(),
                },
            ],
        });
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &bind_group, &[]);
        pass.draw(0..3, 0..1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gpu::shader::present_shader;
    use core::mem::offset_of;

    #[test]
    fn layout_matches_naga() {
        let module = wgpu::naga::front::wgsl::parse_str(&present_shader()).unwrap();
        let (span, members) = module
            .types
            .iter()
            .find_map(|(_, ty)| match (&ty.name, &ty.inner) {
                (Some(n), wgpu::naga::TypeInner::Struct { members, span }) if n == "PresentParams" => {
                    Some((*span, members.clone()))
                }
                _ => None,
            })
            .unwrap();
        assert_eq!(span as usize, size_of::<GpuPresentParams>());
        let off = |name: &str| {
            members
                .iter()
                .find(|m| m.name.as_deref() == Some(name))
                .unwrap()
                .offset as usize
        };
        assert_eq!(off("encode_srgb"), offset_of!(GpuPresentParams, encode_srgb));
        assert_eq!(off("src_size"), offset_of!(GpuPresentParams, src_size));
        assert_eq!(off("dst_origin"), offset_of!(GpuPresentParams, dst_origin));
        assert_eq!(off("dst_size"), offset_of!(GpuPresentParams, dst_size));
        assert_eq!(tone_map_code(ToneMap::Reinhard), 1);
    }
}
