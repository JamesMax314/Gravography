//! Frame statistics on the GPU (WP-12): log-average luminance (auto exposure) and ray-status counts of a
//! [`GpuFrame`], computed by `stats.wgsl` and read back asynchronously (≈ 8 bytes per 256 pixels), so the viewer no
//! longer copies the whole HDR image and G-buffer to the CPU while the camera moves.

use std::sync::Arc;
use std::sync::atomic::{AtomicU8, Ordering};

use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

use super::GpuContext;
use super::renderer::GpuFrame;
use super::shader::stats_shader;

/// Pixels per workgroup of `stats_main`.
const GROUP: u32 = 256;
/// Most workgroups per dispatch dimension (WebGPU's guaranteed minimum limit is 65 535).
const MAX_GROUPS_X: u32 = 32_768;
/// Readback slots (requests that can be in flight).
const SLOTS: usize = 2;
const FREE: u8 = 0;
const PENDING: u8 = 1;
const MAPPED: u8 = 2;
const FAILED: u8 = 3;

/// Statistics uniform (WGSL `StatsParams`; 16 bytes).
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Pod, Zeroable)]
struct GpuStatsParams {
    pixel_count: u32,
    _pad: [u32; 3],
}

/// Statistics of one frame.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct FrameStats {
    /// `exp(mean ln Y)` over lit pixels, as [`crate::tonemap::log_average_luminance`]; `None` if none is lit.
    pub log_average: Option<f64>,
    /// Ray-status counts of the frame's (last sample's) G-buffer, in [`crate::cpu::status_index`] order.
    pub status: [u64; 5],
}

impl FrameStats {
    /// Fractions of rays per status.
    pub fn status_fractions(&self) -> [f64; 5] {
        let n = self.status.iter().sum::<u64>().max(1) as f64;
        self.status.map(|c| c as f64 / n)
    }
}

/// Combines per-workgroup `(Σ ln Y, count)` partials (summed in f64 in a fixed order) and status counts.
pub fn combine(partials: &[[f32; 2]], status: [u32; 5]) -> FrameStats {
    let (mut sum, mut n) = (0.0_f64, 0.0_f64);
    for p in partials {
        sum += f64::from(p[0]);
        n += f64::from(p[1]);
    }
    FrameStats {
        log_average: (n > 0.0).then(|| (sum / n).exp()),
        status: status.map(u64::from),
    }
}

struct Slot {
    partials: wgpu::Buffer,
    counts: wgpu::Buffer,
    read: wgpu::Buffer,
    /// Partials the buffers can hold.
    capacity: u32,
    /// Partials written by the latest request.
    groups: u32,
    state: Arc<AtomicU8>,
}

/// The statistics pass with its asynchronous readback ring.
pub struct GpuStats {
    pipeline: wgpu::ComputePipeline,
    slots: Vec<Option<Slot>>,
    state: Vec<Arc<AtomicU8>>,
}

impl GpuStats {
    /// Compiles the pass.
    pub fn new(ctx: &GpuContext) -> Self {
        let module = ctx.device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("stats"),
            source: wgpu::ShaderSource::Wgsl(stats_shader().into()),
        });
        let pipeline = ctx
            .device
            .create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some("stats"),
                layout: None,
                module: &module,
                entry_point: Some("stats_main"),
                compilation_options: Default::default(),
                cache: None,
            });
        Self {
            pipeline,
            slots: (0..SLOTS).map(|_| None).collect(),
            state: (0..SLOTS).map(|_| Arc::new(AtomicU8::new(FREE))).collect(),
        }
    }

    /// Queues the statistics of `frame` (after the work already submitted for it). Returns `false` without doing
    /// anything if every readback slot is still in flight.
    pub fn request(&mut self, ctx: &GpuContext, frame: &GpuFrame) -> bool {
        let Some(k) = self.state.iter().position(|s| s.load(Ordering::Acquire) == FREE) else {
            return false;
        };
        let dev = &ctx.device;
        let pixels = frame.width * frame.height;
        let groups = pixels.div_ceil(GROUP);
        if self.slots[k].as_ref().is_none_or(|s| s.capacity < groups) {
            let partial_bytes = u64::from(groups) * 8;
            self.slots[k] = Some(Slot {
                partials: dev.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("stats partials"),
                    size: partial_bytes,
                    usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
                    mapped_at_creation: false,
                }),
                counts: dev.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("stats counts"),
                    size: 32,
                    usage: wgpu::BufferUsages::STORAGE
                        | wgpu::BufferUsages::COPY_SRC
                        | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                }),
                read: dev.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("stats readback"),
                    size: 32 + partial_bytes,
                    usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                }),
                capacity: groups,
                groups,
                state: self.state[k].clone(),
            });
        }
        let slot = self.slots[k].as_mut().expect("allocated above");
        slot.groups = groups;
        let slot = &*slot;
        let params = dev.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("stats params"),
            contents: bytemuck::bytes_of(&GpuStatsParams {
                pixel_count: pixels,
                _pad: [0; 3],
            }),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let bind_group = dev.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("stats"),
            layout: &self.pipeline.get_bind_group_layout(0),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: params.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: frame.accum.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: frame.gbuffer.buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 3,
                    resource: slot.partials.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: slot.counts.as_entire_binding(),
                },
            ],
        });
        let mut encoder =
            dev.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("stats") });
        encoder.clear_buffer(&slot.counts, 0, None);
        {
            let mut pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("stats"),
                timestamp_writes: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &bind_group, &[]);
            let gx = groups.min(MAX_GROUPS_X);
            pass.dispatch_workgroups(gx, groups.div_ceil(gx), 1);
        }
        encoder.copy_buffer_to_buffer(&slot.counts, 0, &slot.read, 0, 32);
        encoder.copy_buffer_to_buffer(&slot.partials, 0, &slot.read, 32, u64::from(groups) * 8);
        ctx.queue.submit([encoder.finish()]);
        slot.state.store(PENDING, Ordering::Release);
        let state = slot.state.clone();
        let used = 32 + u64::from(groups) * 8;
        slot.read.slice(..used).map_async(wgpu::MapMode::Read, move |r| {
            state.store(if r.is_ok() { MAPPED } else { FAILED }, Ordering::Release);
        });
        true
    }

    /// Whether a request is still in flight (the caller should keep polling).
    pub fn in_flight(&self) -> bool {
        self.state.iter().any(|s| s.load(Ordering::Acquire) != FREE)
    }

    /// Non-blocking: the statistics of the most recent request that has completed, if any.
    pub fn poll(&mut self, ctx: &GpuContext) -> Option<FrameStats> {
        let _ = ctx.device.poll(wgpu::PollType::Poll);
        self.collect()
    }

    /// Like [`GpuStats::poll`] but waits for the GPU first (tests).
    pub fn wait(&mut self, ctx: &GpuContext) -> Option<FrameStats> {
        let _ = ctx.device.poll(wgpu::PollType::wait_indefinitely());
        self.collect()
    }

    fn collect(&mut self) -> Option<FrameStats> {
        let mut latest = None;
        for slot in self.slots.iter().flatten() {
            match slot.state.load(Ordering::Acquire) {
                MAPPED => {
                    let used = 32 + u64::from(slot.groups) * 8;
                    let stats = slot.read.slice(..used).get_mapped_range().ok().map(|v| {
                        let counts: &[u32] = bytemuck::cast_slice(&v[..20]);
                        let partials: &[[f32; 2]] = bytemuck::cast_slice(&v[32..]);
                        combine(partials, [counts[0], counts[1], counts[2], counts[3], counts[4]])
                    });
                    slot.read.unmap();
                    slot.state.store(FREE, Ordering::Release);
                    if stats.is_some() {
                        latest = stats;
                    }
                }
                FAILED => slot.state.store(FREE, Ordering::Release),
                _ => {}
            }
        }
        latest
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn partials_combine_like_the_cpu_log_average() {
        // Two groups: ln Y sums 2·ln 2 over 2 pixels and ln 8 over 1 pixel → exp((2 ln 2 + 3 ln 2)/3).
        let ln2 = 2.0_f32.ln();
        let s = combine(&[[2.0 * ln2, 2.0], [3.0 * ln2, 1.0]], [3, 1, 0, 0, 0]);
        let expect = (5.0 * f64::from(ln2) / 3.0).exp();
        assert!((s.log_average.unwrap() - expect).abs() < 1e-12);
        assert_eq!(s.status, [3, 1, 0, 0, 0]);
        assert_eq!(s.status_fractions(), [0.75, 0.25, 0.0, 0.0, 0.0]);
        assert_eq!(combine(&[[0.0, 0.0]], [0; 5]).log_average, None);
    }
}
