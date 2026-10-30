// sky.wgsl: sky sampling and spectral shading. Ports of gravography_sky::cubemap::{dir_to_face_uv,
// CubeLevel::sample_bilinear, CubeMap::sample_trilinear}, gravography_sky::BlackbodyLut::lookup and
// gravography_render::sky::Sky::{texel_solid_angle, lod, shade} (docs/physics.md § "Shading", § "Sky cube map").
// The cube map is a 2D-array texture (6 layers = faces +X, −X, +Y, −Y, +Z, −Z; full mip chain; RG32Float texels
// (w, w·T)) read with textureLoad, so the filtering is exactly the CPU's (bilinear within a face, clamped at face
// edges, linear between levels).

// Shading parameters (Rust: gravography_render::gpu::GpuShadeParams; 80 bytes).
struct ShadeParams {
    // Rows of the XYZ → linear sRGB matrix (w unused).
    xyz_to_rgb0: vec4<f32>,
    xyz_to_rgb1: vec4<f32>,
    xyz_to_rgb2: vec4<f32>,
    lut_t_min: f32,
    lut_t_max: f32,
    lut_n: u32,
    base_size: u32,
    mip_count: u32,
    samples: u32,
    flags: u32,
    sample_index: u32,
}

// Effect flag: intensity beaming (Rust: gravography_render::gpu::FLAG_BEAMING).
const FLAG_BEAMING: u32 = 4u;

struct FaceUv {
    face: u32,
    u: f32,
    v: f32,
}

// Face and (u, v) of a direction (Rust: dir_to_face_uv; same tie-breaking).
fn dir_to_face_uv(d: vec3<f32>) -> FaceUv {
    let a = abs(d);
    var face: u32;
    var sc: f32;
    var tc: f32;
    var ma: f32;
    if (a.x >= a.y && a.x >= a.z) {
        if (d.x >= 0.0) { face = 0u; sc = -d.z; } else { face = 1u; sc = d.z; }
        tc = -d.y;
        ma = a.x;
    } else if (a.y >= a.z) {
        if (d.y >= 0.0) { face = 2u; tc = d.z; } else { face = 3u; tc = -d.z; }
        sc = d.x;
        ma = a.y;
    } else {
        if (d.z >= 0.0) { face = 4u; sc = d.x; } else { face = 5u; sc = -d.x; }
        tc = -d.y;
        ma = a.z;
    }
    return FaceUv(face, 0.5 * (sc / ma + 1.0), 0.5 * (tc / ma + 1.0));
}

fn cube_texel(face: u32, col: u32, row: u32, level: u32) -> vec2<f32> {
    return textureLoad(sky_cube, vec2<u32>(col, row), face, level).xy;
}

// Bilinear (w, w·T) at d on mip `level` (Rust: CubeLevel::sample_bilinear).
fn sample_bilinear(d: vec3<f32>, level: u32) -> vec2<f32> {
    let fuv = dir_to_face_uv(d);
    let n = max(shade.base_size >> level, 1u);
    let mx = f32(n - 1u);
    let s = clamp(fuv.u * f32(n) - 0.5, 0.0, mx);
    let t = clamp(fuv.v * f32(n) - 0.5, 0.0, mx);
    let c0 = u32(floor(s));
    let r0 = u32(floor(t));
    let c1 = min(c0 + 1u, n - 1u);
    let r1 = min(r0 + 1u, n - 1u);
    let fs = s - f32(c0);
    let ft = t - f32(r0);
    let a = cube_texel(fuv.face, c0, r0, level);
    let b = cube_texel(fuv.face, c1, r0, level);
    let c = cube_texel(fuv.face, c0, r1, level);
    let e = cube_texel(fuv.face, c1, r1, level);
    return (1.0 - ft) * ((1.0 - fs) * a + fs * b) + ft * ((1.0 - fs) * c + fs * e);
}

// Trilinear (w, w·T) at d and level of detail `lod` (Rust: CubeMap::sample_trilinear).
fn sample_trilinear(d: vec3<f32>, lod: f32) -> vec2<f32> {
    let mx = f32(shade.mip_count - 1u);
    var l = 0.0;
    if (is_finite_f32(lod)) {
        l = clamp(lod, 0.0, mx);
    }
    let i = min(u32(floor(l)), shade.mip_count - 1u);
    let f = l - f32(i);
    let a = sample_bilinear(d, i);
    if (f <= 0.0 || i + 1u >= shade.mip_count) {
        return a;
    }
    let b = sample_bilinear(d, i + 1u);
    return (1.0 - f) * a + f * b;
}

// Solid angle of a level-0 texel at unit direction d: (2/N)² |d_major|³ (Rust: Sky::texel_solid_angle).
fn sky_texel_solid_angle(d: vec3<f32>) -> f32 {
    let a = abs(d);
    let major = max(max(a.x, a.y), a.z);
    let pitch = 2.0 / f32(shade.base_size);
    return pitch * pitch * major * major * major;
}

// ½ log₂(Ω / Ω_texel(d)) (Rust: Sky::lod).
fn sky_lod(d: vec3<f32>, solid_angle: f32) -> f32 {
    return 0.5 * log2(solid_angle / sky_texel_solid_angle(d));
}

struct LutSample {
    response: vec3<f32>,
    clamped: bool,
}

// Linearly interpolated band response at temperature t (Rust: BlackbodyLut::lookup); entries are
// [ln S, x, y, 0] on a uniform grid in ln T.
fn lut_lookup(t: f32) -> LutSample {
    let n = shade.lut_n;
    let u = log(t / shade.lut_t_min) / log(shade.lut_t_max / shade.lut_t_min);
    let clamped = !(u >= 0.0 && u <= 1.0);
    var uc = 0.0;
    if (is_finite_f32(u)) {
        uc = clamp(u, 0.0, 1.0);
    }
    let s = uc * f32(n - 1u);
    let i = min(u32(floor(s)), n - 2u);
    let f = s - f32(i);
    let a = lut[i];
    let b = lut[i + 1u];
    let v = a + f * (b - a);
    let sum = exp(v.x);
    return LutSample(vec3<f32>(sum * v.y, sum * v.z, sum * (1.0 - v.y - v.z)), clamped);
}

// XYZ seen along escape direction d with shift g and footprint solid_angle (Rust: Sky::shade). The frequency
// shift toggle is already applied in the G-buffer (g = 1 when off).
fn sky_shade(d: vec3<f32>, solid_angle: f32, g: f32) -> vec3<f32> {
    let wt = sample_trilinear(d, sky_lod(d, solid_angle));
    let w = wt.x;
    if (!(w > 0.0)) {
        return vec3<f32>(0.0);
    }
    let s = lut_lookup(g * wt.y / w);
    var scale = w;
    if ((shade.flags & FLAG_BEAMING) == 0u) {
        scale = w / (g * g * g * g);
    }
    return s.response * scale;
}

fn xyz_to_linear_srgb(xyz: vec3<f32>) -> vec3<f32> {
    return vec3<f32>(dot(shade.xyz_to_rgb0.xyz, xyz), dot(shade.xyz_to_rgb1.xyz, xyz), dot(shade.xyz_to_rgb2.xyz, xyz));
}
