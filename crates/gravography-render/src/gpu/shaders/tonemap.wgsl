// tonemap.wgsl: exposure + tone mapping + sRGB encoding of the accumulation buffer to packed RGBA8 (port of
// gravography_render::tonemap::to_srgb8; the operators are in tonemap_ops.wgsl).

// Rust: gravography_render::gpu::GpuToneParams (16 bytes).
struct ToneParams {
    // Exposure scale applied to the mean HDR value.
    scale: f32,
    // 0 = ACES fitted, 1 = extended Reinhard.
    op: u32,
    pixel_count: u32,
    _pad: u32,
}

@group(0) @binding(0) var<uniform> tone: ToneParams;
@group(0) @binding(1) var<storage, read> accum_in: array<vec4<f32>>;
@group(0) @binding(2) var<storage, read_write> rgba8: array<u32>;

@compute @workgroup_size(64)
fn tonemap_main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let i = gid.x;
    if (i >= tone.pixel_count) {
        return;
    }
    let mapped = tone_map_accum(accum_in[i], tone.scale, tone.op);
    rgba8[i] = to_byte(mapped.x) | (to_byte(mapped.y) << 8u) | (to_byte(mapped.z) << 16u) | (255u << 24u);
}
