// common.wgsl: shared helpers. Assembled first (see crates/gravography-render/src/gpu/shader.rs).

// Ray status codes, identical to gravography_render::cpu::status_index.
const STATUS_ESCAPED: u32 = 0u;
const STATUS_CAPTURED: u32 = 1u;
const STATUS_SINGULAR: u32 = 2u;
const STATUS_MAX_STEPS: u32 = 3u;
const STATUS_ERROR: u32 = 4u;

// True unless v is NaN or ±Inf. WGSL has no isnan/isinf, and comparisons such as `v != v` may be optimised away
// under fast-math, so test the exponent bits directly.
fn is_finite_f32(v: f32) -> bool {
    return (bitcast<u32>(v) & 0x7f800000u) != 0x7f800000u;
}

fn is_finite_vec4(v: vec4<f32>) -> bool {
    return is_finite_f32(v.x) && is_finite_f32(v.y) && is_finite_f32(v.z) && is_finite_f32(v.w);
}

// max |v_i| over the four components (Rust: ray.rs max_abs).
fn max_abs4(v: vec4<f32>) -> f32 {
    let a = abs(v);
    return max(max(a.x, a.y), max(a.z, a.w));
}
