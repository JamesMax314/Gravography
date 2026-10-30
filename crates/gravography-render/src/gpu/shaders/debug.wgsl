// debug.wgsl: FR-R6 debug views from the G-buffer, written as packed RGBA8 (Rust: gravography_render::gpu::DebugView).
// 0 = ray status, 1 = frequency shift g (false colour), 2 = step count (heat map).

// Rust: GpuDebugParams (16 bytes).
struct DebugParams {
    mode: u32,
    width: u32,
    height: u32,
    max_steps: u32,
}

@group(0) @binding(0) var<uniform> dbg: DebugParams;
@group(0) @binding(1) var<storage, read> gbuffer: array<GBufferTexel>;
@group(0) @binding(2) var<storage, read_write> rgba8: array<u32>;

fn pack_rgb(c: vec3<f32>) -> u32 {
    let b = vec3<u32>(floor(clamp(c, vec3<f32>(0.0), vec3<f32>(1.0)) * 255.0 + 0.5));
    return b.x | (b.y << 8u) | (b.z << 16u) | (255u << 24u);
}

// Status colours: escaped blue, captured black, singular yellow, max-steps red, error magenta.
fn status_colour(s: u32) -> vec3<f32> {
    switch s {
        case 0u: { return vec3<f32>(0.2, 0.4, 1.0); }
        case 1u: { return vec3<f32>(0.0, 0.0, 0.0); }
        case 2u: { return vec3<f32>(1.0, 1.0, 0.0); }
        case 3u: { return vec3<f32>(1.0, 0.0, 0.0); }
        default: { return vec3<f32>(1.0, 0.0, 1.0); }
    }
}

// Diverging map of log₂ g over [−2, 2]: red (redshift, g ≤ 1/4) → white (g = 1) → blue (blueshift, g ≥ 4).
// Non-escaped pixels are drawn dark grey.
fn g_colour(t: GBufferTexel) -> vec3<f32> {
    if (t.status != STATUS_ESCAPED) {
        return vec3<f32>(0.1);
    }
    let x = clamp(log2(t.dir_g.w) / 2.0, -1.0, 1.0);
    if (x < 0.0) {
        return mix(vec3<f32>(1.0), vec3<f32>(0.85, 0.1, 0.05), -x);
    }
    return mix(vec3<f32>(1.0), vec3<f32>(0.05, 0.2, 0.9), x);
}

// Heat map of log(1 + steps)/log(1 + max_steps): black → purple → orange → yellow-white.
fn steps_colour(t: GBufferTexel) -> vec3<f32> {
    let total = f32(t.steps + t.rejected);
    let x = clamp(log(1.0 + total) / log(1.0 + f32(dbg.max_steps)), 0.0, 1.0);
    let c0 = vec3<f32>(0.0, 0.0, 0.0);
    let c1 = vec3<f32>(0.45, 0.05, 0.55);
    let c2 = vec3<f32>(0.95, 0.45, 0.1);
    let c3 = vec3<f32>(1.0, 1.0, 0.75);
    if (x < 1.0 / 3.0) {
        return mix(c0, c1, x * 3.0);
    } else if (x < 2.0 / 3.0) {
        return mix(c1, c2, x * 3.0 - 1.0);
    }
    return mix(c2, c3, x * 3.0 - 2.0);
}

@compute @workgroup_size(64)
fn debug_main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let i = gid.x;
    if (i >= dbg.width * dbg.height) {
        return;
    }
    let t = gbuffer[i];
    var c: vec3<f32>;
    if (dbg.mode == 0u) {
        c = status_colour(t.status);
    } else if (dbg.mode == 1u) {
        c = g_colour(t);
    } else {
        c = steps_colour(t);
    }
    rgba8[i] = pack_rgb(c);
}
