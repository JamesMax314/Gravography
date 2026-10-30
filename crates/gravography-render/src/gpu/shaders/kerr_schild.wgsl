// kerr_schild.wgsl: port of crates/gravography-physics/src/kerr_schild.rs (docs/physics.md § "Kerr–Schild
// metric", § "Derivatives of the Kerr–Schild fields"). Function names and operation order follow the Rust code so
// that f32 results match the CPU f32 path as closely as possible.

// Kerr–Schild fields at one point (Rust: KsFields).
struct KsFields {
    r: f32,
    f: f32,
    l_lo: vec4<f32>,
}

// Fields and their gradients (Rust: ks_field_derivatives). dl_lo[α][μ] = ∂_α l_μ; all ∂_t vanish.
struct KsFieldDerivs {
    fields: KsFields,
    dr: vec4<f32>,
    df: vec4<f32>,
    dl_lo: array<vec4<f32>, 4>,
}

// Cancellation-free r(x, y, z) (Rust: ks_radius; docs/physics.md § "Cancellation-free r").
fn ks_radius(a: f32, x: f32, y: f32, z: f32) -> f32 {
    let a2 = a * a;
    let w = 0.5 * (x * x + y * y + z * z - a2);
    let az2 = a2 * z * z;
    let s = sqrt(w * w + az2);
    var r2: f32;
    if (w >= 0.0) {
        r2 = w + s;
    } else {
        r2 = az2 / (s - w);
    }
    return sqrt(r2);
}

// r, f = 2 M r³/(r⁴ + a² z²) and l_μ (Rust: ks_fields), with the r = 0 disk limit f = 0.
fn ks_fields(mass: f32, a: f32, pos: vec4<f32>) -> KsFields {
    let x = pos.y;
    let y = pos.z;
    let z = pos.w;
    let r = ks_radius(a, x, y, z);
    let r2 = r * r;
    let a2 = a * a;
    if (r2 <= 0.0) {
        var inv = 0.0;
        if (a2 > 0.0) {
            inv = 1.0 / a2;
        }
        return KsFields(r, 0.0, vec4<f32>(1.0, a * y * inv, -a * x * inv, 0.0));
    }
    let f = 2.0 * mass * r * r2 / (r2 * r2 + a2 * z * z);
    let inv = 1.0 / (r2 + a2);
    return KsFields(r, f, vec4<f32>(1.0, (r * x + a * y) * inv, (r * y - a * x) * inv, z / r));
}

fn kronecker(i: u32, j: u32) -> f32 {
    return select(0.0, 1.0, i == j);
}

// Analytic gradients of r, f and l_μ (Rust: ks_field_derivatives).
fn ks_field_derivatives(mass: f32, a: f32, pos: vec4<f32>) -> KsFieldDerivs {
    let fields = ks_fields(mass, a, pos);
    var out: KsFieldDerivs;
    out.fields = fields;
    out.dr = vec4<f32>(0.0);
    out.df = vec4<f32>(0.0);
    for (var k = 0u; k < 4u; k++) {
        out.dl_lo[k] = vec4<f32>(0.0);
    }
    let r = fields.r;
    let r2 = r * r;
    if (r2 <= 0.0) {
        return out;
    }
    let x = pos.y;
    let y = pos.z;
    let z = pos.w;
    let a2 = a * a;
    let q = r2 + a2;
    let d = r2 * r2 + a2 * z * z;
    let c = r * r2 / d;
    let dr = vec4<f32>(0.0, x * c, y * c, z * q * r / d);
    out.dr = dr;

    let f = fields.f;
    for (var j = 1u; j < 4u; j++) {
        var dz = 0.0;
        if (j == 3u) {
            dz = 2.0 * a2 * z;
        }
        out.df[j] = f * (3.0 * dr[j] / r - (4.0 * r * r2 * dr[j] + dz) / d);
    }

    let lx = fields.l_lo.y;
    let ly = fields.l_lo.z;
    for (var j = 1u; j < 4u; j++) {
        var dl = vec4<f32>(0.0);
        dl.y = (x * dr[j] + r * kronecker(j, 1u) + a * kronecker(j, 2u) - 2.0 * r * lx * dr[j]) / q;
        dl.z = (y * dr[j] + r * kronecker(j, 2u) - a * kronecker(j, 1u) - 2.0 * r * ly * dr[j]) / q;
        dl.w = kronecker(j, 3u) / r - z * dr[j] / r2;
        out.dl_lo[j] = dl;
    }
    return out;
}

// Covariant metric g_{μν} = η_{μν} + f l_μ l_ν (Rust: ks_g_lo). Symmetric, so column/row order does not matter.
fn ks_g_lo(mass: f32, a: f32, pos: vec4<f32>) -> mat4x4<f32> {
    let fl = ks_fields(mass, a, pos);
    let l = fl.l_lo;
    let eta = mat4x4<f32>(
        vec4<f32>(-1.0, 0.0, 0.0, 0.0),
        vec4<f32>(0.0, 1.0, 0.0, 0.0),
        vec4<f32>(0.0, 0.0, 1.0, 0.0),
        vec4<f32>(0.0, 0.0, 0.0, 1.0),
    );
    return eta + mat4x4<f32>(fl.f * l.x * l, fl.f * l.y * l, fl.f * l.z * l, fl.f * l.w * l);
}
