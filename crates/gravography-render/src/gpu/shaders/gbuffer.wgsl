// gbuffer.wgsl: G-buffer and tile structs shared by the trace, shade and debug passes.

// G-buffer texel (Rust: gravography_render::gpu::GBufferTexel; 32 bytes).
struct GBufferTexel {
    // Unit escape direction (xyz, zero unless escaped) and frequency shift g = 1/p_t (w).
    dir_g: vec4<f32>,
    status: u32,
    steps: u32,
    rejected: u32,
    _pad: u32,
}

// Tile of the image traced by one dispatch (Rust: GpuTile; 16 bytes).
struct Tile {
    origin: vec2<u32>,
    size: vec2<u32>,
}
