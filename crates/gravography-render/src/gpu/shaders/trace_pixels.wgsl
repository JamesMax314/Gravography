// trace_pixels.wgsl: per-pixel trace pass (docs/PRD.md §4.3 step 2). One invocation per pixel of the current tile;
// writes the G-buffer {escape direction, g, status, steps}. Port of CpuRenderer::cast_pixel with DOPRI5.

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

@group(0) @binding(0) var<uniform> params: TraceParams;
@group(0) @binding(1) var<uniform> camera: Camera;
@group(0) @binding(2) var<uniform> tile: Tile;
@group(0) @binding(3) var<storage, read_write> gbuffer: array<GBufferTexel>;

// Traces the camera ray through continuous pixel coordinate (x, y) (Rust: CpuRenderer::cast_pixel / cast).
fn trace_pixel(x: f32, y: f32) -> GBufferTexel {
    let n = projection_direction(camera, x, y);
    let p0 = camera_ray_momentum(params, camera, n);
    var g = 1.0;
    if ((camera.flags & FLAG_FREQUENCY_SHIFT) != 0u) {
        g = 1.0 / p0.x;
    }
    if ((camera.flags & FLAG_LENSING) == 0u) {
        // Straight ray along the spatial part of k = −e_(0) + nⁱ e_(i): keeps aberration, removes bending.
        let k = tetrad_to_coordinate(camera, vec4<f32>(-1.0, n));
        if (is_finite_f32(g) && g > 0.0) {
            return GBufferTexel(vec4<f32>(escape_direction(k), g), STATUS_ESCAPED, 0u, 0u, 0u);
        }
        return GBufferTexel(vec4<f32>(0.0, 0.0, 0.0, g), STATUS_ERROR, 0u, 0u, 0u);
    }
    let r = trace_ray_dopri5(params, camera.position, p0);
    return GBufferTexel(vec4<f32>(r.dir.xyz, g), r.status, r.steps, r.rejected, 0u);
}

@compute @workgroup_size(8, 8)
fn trace_pixels_main(@builtin(global_invocation_id) gid: vec3<u32>) {
    if (gid.x >= tile.size.x || gid.y >= tile.size.y) {
        return;
    }
    let px = tile.origin.x + gid.x;
    let py = tile.origin.y + gid.y;
    if (px >= camera.width || py >= camera.height) {
        return;
    }
    gbuffer[py * camera.width + px] = trace_pixel(f32(px) + camera.jitter.x, f32(py) + camera.jitter.y);
}
