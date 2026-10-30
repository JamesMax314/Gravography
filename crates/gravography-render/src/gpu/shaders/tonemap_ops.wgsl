// tonemap_ops.wgsl: tone-mapping functions shared by the tone-map compute pass and the present pass (ports of
// gravography_render::tonemap::{aces_fitted, reinhard_extended, ToneMap::apply} and
// gravography_sky::colour::srgb_encode).

const LUMA: vec3<f32> = vec3<f32>(0.2126, 0.7152, 0.0722);
const REINHARD_WHITE: f32 = 16.0;

fn aces_fitted(rgb: vec3<f32>) -> vec3<f32> {
    // Rows of M_in and M_out (S. Hill, BakingLab).
    let v = vec3<f32>(
        dot(vec3<f32>(0.59719, 0.35458, 0.04823), rgb),
        dot(vec3<f32>(0.07600, 0.90834, 0.01566), rgb),
        dot(vec3<f32>(0.02840, 0.13383, 0.83777), rgb));
    let a = v * (v + 0.0245786) - 0.000090537;
    let b = v * (0.983729 * v + 0.432951) + 0.238081;
    let c = a / b;
    let out = vec3<f32>(
        dot(vec3<f32>(1.60475, -0.53108, -0.07367), c),
        dot(vec3<f32>(-0.10208, 1.10813, -0.00605), c),
        dot(vec3<f32>(-0.00327, -0.07276, 1.07602), c));
    return clamp(out, vec3<f32>(0.0), vec3<f32>(1.0));
}

fn reinhard_extended(rgb: vec3<f32>, white: f32) -> vec3<f32> {
    let l = dot(LUMA, rgb);
    if (l <= 0.0) {
        return vec3<f32>(0.0);
    }
    let ld = l * (1.0 + l / (white * white)) / (1.0 + l);
    return clamp(rgb * (ld / l), vec3<f32>(0.0), vec3<f32>(1.0));
}

fn srgb_encode(c: f32) -> f32 {
    let x = clamp(c, 0.0, 1.0);
    if (x <= 0.0031308) {
        return 12.92 * x;
    }
    return 1.055 * pow(x, 1.0 / 2.4) - 0.055;
}

// Round half away from zero for x ≥ 0 (Rust f64::round), not WGSL round() (half to even).
fn to_byte(c: f32) -> u32 {
    return u32(floor(srgb_encode(c) * 255.0 + 0.5));
}

// Exposure and tone mapping of the mean HDR value `acc.xyz / acc.w` (Rust: to_srgb8 before encoding):
// NaN → 0, negatives → 0, then the operator (0 = ACES fitted, 1 = extended Reinhard). Returns display-linear rgb.
fn tone_map_accum(acc: vec4<f32>, scale: f32, op: u32) -> vec3<f32> {
    var rgb = acc.xyz * (scale / max(acc.w, 1.0));
    rgb = vec3<f32>(
        select(0.0, max(rgb.x, 0.0), is_finite_f32(rgb.x)),
        select(0.0, max(rgb.y, 0.0), is_finite_f32(rgb.y)),
        select(0.0, max(rgb.z, 0.0), is_finite_f32(rgb.z)));
    if (op == 0u) {
        return aces_fitted(rgb);
    }
    return reinhard_extended(rgb, REINHARD_WHITE);
}
