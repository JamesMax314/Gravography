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

// Hamilton's equations (Rust: ks_hamiltonian_rhs).
fn ks_hamiltonian_rhs(mass: f32, a: f32, x: vec4<f32>, p: vec4<f32>) -> Phase {
    // `var`, not `let`: d.dl_lo is indexed dynamically below, which WGSL only guarantees for memory views.
    var d = ks_field_derivatives(mass, a, x);
    let l_lo = d.fields.l_lo;
    let l_up = vec4<f32>(-l_lo.x, l_lo.y, l_lo.z, l_lo.w);
    let lp = l_dot_p(l_lo, p);
    let flp = d.fields.f * lp;
    let dx = vec4<f32>(-p.x - flp * l_up.x, p.y - flp * l_up.y, p.z - flp * l_up.z, p.w - flp * l_up.w);
    var dp = vec4<f32>(0.0);
    for (var mu = 0u; mu < 4u; mu++) {
        let dl = d.dl_lo[mu];
        let dlp = dl.y * p.y + dl.z * p.z + dl.w * p.w;
        dp[mu] = 0.5 * d.df[mu] * lp * lp + flp * dlp;
    }
    return Phase(dx, dp);
}
