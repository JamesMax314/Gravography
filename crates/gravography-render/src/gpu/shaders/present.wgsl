// present.wgsl: present pass (docs/PRD.md §4.3 step 4). A full-screen triangle; each fragment reads the
// accumulation-buffer pixel under it (nearest, the internal image stretched over the destination rectangle),
// applies exposure and tone mapping (tonemap_ops.wgsl) and sRGB-encodes unless the target format does it.

// Rust: gravography_render::gpu::GpuPresentParams (48 bytes).
struct PresentParams {
    // Exposure scale.
    scale: f32,
    // 0 = ACES fitted, 1 = extended Reinhard.
    op: u32,
    // 1 = apply the sRGB transfer function (non-sRGB target format), 0 = the target encodes.
    encode_srgb: u32,
    _pad0: u32,
    // Internal image size in pixels.
    src_size: vec2<u32>,
    _pad1: vec2<u32>,
    // Destination rectangle in framebuffer pixels.
    dst_origin: vec2<f32>,
    dst_size: vec2<f32>,
}

@group(0) @binding(0) var<uniform> present: PresentParams;
@group(0) @binding(1) var<storage, read> accum_in: array<vec4<f32>>;

@vertex
fn present_vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    // Full-screen triangle: (−1, −1), (3, −1), (−1, 3).
    let x = f32(i32(i & 1u) * 4 - 1);
    let y = f32(i32(i >> 1u) * 4 - 1);
    return vec4<f32>(x, y, 0.0, 1.0);
}

@fragment
fn present_fs(@builtin(position) frag: vec4<f32>) -> @location(0) vec4<f32> {
    let p = (frag.xy - present.dst_origin) / present.dst_size;
    if (p.x < 0.0 || p.y < 0.0 || p.x >= 1.0 || p.y >= 1.0) {
        return vec4<f32>(0.0, 0.0, 0.0, 1.0);
    }
    let src = min(vec2<u32>(p * vec2<f32>(present.src_size)), present.src_size - vec2<u32>(1u));
    let mapped = tone_map_accum(accum_in[src.y * present.src_size.x + src.x], present.scale, present.op);
    if (present.encode_srgb == 1u) {
        return vec4<f32>(srgb_encode(mapped.x), srgb_encode(mapped.y), srgb_encode(mapped.z), 1.0);
    }
    return vec4<f32>(mapped, 1.0);
}
