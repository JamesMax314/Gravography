// geodesic.wgsl: port of crates/gravography-physics/src/geodesic.rs (docs/physics.md § "Hamiltonian right-hand
// side"). dx^μ/dλ = η^{μν} p_ν − f (l·p) l^μ, dp_μ/dλ = ½ ∂_μ f (l·p)² + f (l·p) (∂_μ l^ν) p_ν.

// Phase-space state (x^μ, p_μ) or its derivative (Rust: PhaseState as two 4-vectors).
struct Phase {
    x: vec4<f32>,
    p: vec4<f32>,
}

// l^μ p_μ with l^μ = (−1, l_x, l_y, l_z) (Rust: l_dot_p).
fn l_dot_p(l_lo: vec4<f32>, p: vec4<f32>) -> f32 {
    return -p.x + l_lo.y * p.y + l_lo.z * p.z + l_lo.w * p.w;
}

// Hamilton's equations (Rust: ks_hamiltonian_rhs), in fused vector form for speed (WP-12; docs/physics.md
// § "GPU right-hand side in vector form"). Same maths as the Rust code, which builds ∂_α l_μ as a matrix and then
// contracts it with p: here the contraction (∂_j l)·p is written out analytically, so no arrays or dynamically
// indexed vectors are needed. naga lowers those to thread-local memory on Metal, which made the RHS ~10× slower.
fn ks_hamiltonian_rhs(mass: f32, a: f32, x: vec4<f32>, p: vec4<f32>) -> Phase {
    let fields = ks_fields(mass, a, x);
    let l_lo = fields.l_lo;
    let l_up = vec4<f32>(-l_lo.x, l_lo.y, l_lo.z, l_lo.w);
    let lp = l_dot_p(l_lo, p);
    let flp = fields.f * lp;
    let dx = vec4<f32>(-p.x, p.y, p.z, p.w) - flp * l_up;
    let r = fields.r;
    let r2 = r * r;
    if (r2 <= 0.0) {
        // Ring disk r = 0: the Rust code sets all gradients to zero there.
        return Phase(dx, vec4<f32>(0.0));
    }
    let pos = x.yzw;
    let ps = p.yzw;
    let a2 = a * a;
    let q = r2 + a2;
    let d = r2 * r2 + a2 * pos.z * pos.z;
    let c = r * r2 / d;
    // ∂_j r, ∂_j f (Rust: ks_field_derivatives).
    let dr = vec3<f32>(pos.x * c, pos.y * c, pos.z * q * r / d);
    let df = fields.f * (3.0 * dr / r - (4.0 * r * r2 * dr + vec3<f32>(0.0, 0.0, 2.0 * a2 * pos.z)) / d);
    // (∂_j l_i) p_i = s ∂_j r + (r p_x − a p_y, a p_x + r p_y, 0)_j / q + δ_j3 p_z / r.
    let s = (pos.x * ps.x + pos.y * ps.y - 2.0 * r * (l_lo.y * ps.x + l_lo.z * ps.y)) / q - ps.z * pos.z / r2;
    let dlp = s * dr + vec3<f32>(r * ps.x - a * ps.y, a * ps.x + r * ps.y, 0.0) / q + vec3<f32>(0.0, 0.0, ps.z / r);
    let dps = 0.5 * df * lp * lp + flp * dlp;
    return Phase(dx, vec4<f32>(0.0, dps));
}
