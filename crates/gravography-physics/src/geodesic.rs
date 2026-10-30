//! Hamiltonian form of the geodesic equations (`docs/PRD.md` §5.3; `docs/physics.md`
//! § "Hamiltonian right-hand side").
//!
//! `H = ½ g^{μν} p_μ p_ν`, `dx^μ/dλ = g^{μν} p_ν`, `dp_μ/dλ = −½ (∂_μ g^{αβ}) p_α p_β`.
//! With `g^{μν} = η^{μν} − f l^μ l^ν` and `(l·p) = l^μ p_μ`, these reduce to
//! `dx^μ/dλ = η^{μν} p_ν − f (l·p) l^μ` and `dp_μ/dλ = ½ ∂_μ f (l·p)² + f (l·p) (∂_μ l^ν) p_ν`.
//! Only 4-vectors are needed, no 4×4×4 tensors, which keeps the future WGSL port cheap. `dp_t/dλ = 0`
//! holds exactly.

use crate::kerr_schild::{ks_field_derivatives, ks_fields};
use crate::linalg::Vec4;
use crate::{Kerr, Real};

/// `l^μ p_μ` with `l^μ = (−1, l_x, l_y, l_z)`.
fn l_dot_p<T: Real>(l_lo: &Vec4<T>, p: &Vec4<T>) -> T {
    -p[0] + l_lo[1] * p[1] + l_lo[2] * p[2] + l_lo[3] * p[3]
}

/// `η^{μν} p_μ p_ν`.
fn eta_pp<T: Real>(p: &Vec4<T>) -> T {
    -p[0] * p[0] + p[1] * p[1] + p[2] * p[2] + p[3] * p[3]
}

/// Hamiltonian `H = ½ g^{μν} p_μ p_ν = ½ (η^{μν} p_μ p_ν − f (l·p)²)` at the KS event `x` with covariant
/// momentum `p`. `H = 0` for photons, `−½` for unit-mass particles.
pub fn ks_hamiltonian<T: Real>(mass: T, a: T, x: &Vec4<T>, p: &Vec4<T>) -> T {
    let fl = ks_fields(mass, a, x);
    let lp = l_dot_p(&fl.l_lo, p);
    T::lit(0.5) * (eta_pp(p) - fl.f * lp * lp)
}

/// Hamilton's equations at `(x, p)`: returns `(dx^μ/dλ, dp_μ/dλ)`.
pub fn ks_hamiltonian_rhs<T: Real>(mass: T, a: T, x: &Vec4<T>, p: &Vec4<T>) -> (Vec4<T>, Vec4<T>) {
    let (fl, d) = ks_field_derivatives(mass, a, x);
    let l_up = fl.l_up();
    let lp = l_dot_p(&fl.l_lo, p);
    let flp = fl.f * lp;
    let dx = [
        -p[0] - flp * l_up[0],
        p[1] - flp * l_up[1],
        p[2] - flp * l_up[2],
        p[3] - flp * l_up[3],
    ];
    let half = T::lit(0.5);
    // (∂_μ l^ν) p_ν: ∂l^t = 0 and ∂_μ l^i = ∂_μ l_i.
    let dp = core::array::from_fn(|mu| {
        let dl = &d.dl_lo[mu];
        let dlp = dl[1] * p[1] + dl[2] * p[2] + dl[3] * p[3];
        half * d.df[mu] * lp * lp + flp * dlp
    });
    (dx, dp)
}

impl<T: Real> Kerr<T> {
    /// Hamiltonian `H = ½ g^{μν} p_μ p_ν` (see [`ks_hamiltonian`]).
    pub fn hamiltonian(&self, x: &Vec4<T>, p: &Vec4<T>) -> T {
        ks_hamiltonian(self.mass(), self.spin(), x, p)
    }

    /// Hamilton's equations `(dx^μ/dλ, dp_μ/dλ)` (see [`ks_hamiltonian_rhs`]).
    pub fn hamiltonian_rhs(&self, x: &Vec4<T>, p: &Vec4<T>) -> (Vec4<T>, Vec4<T>) {
        ks_hamiltonian_rhs(self.mass(), self.spin(), x, p)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::connection::ks_christoffel;
    use crate::kerr_schild::{ks_dg_up, ks_g_up};
    use crate::linalg::{inner, raise};
    use crate::test_util::{event, max_abs4, momentum, richardson};
    use proptest::prelude::*;

    /// Natural scale of the RHS: |p|² (1 + f)² / r for dp, |p| (1 + f) for dx.
    fn scales(a: f64, x: &Vec4<f64>, p: &Vec4<f64>) -> (f64, f64) {
        let fl = ks_fields(1.0, a, x);
        let pn = max_abs4(p);
        (pn * (1.0 + fl.f), pn * pn * (1.0 + fl.f).powi(2) / fl.r)
    }

    proptest! {
        #[test]
        fn rhs_matches_tensor_form((a, x) in event(), p in momentum()) {
            // dx^μ = g^{μν} p_ν and dp_μ = −½ ∂_μ g^{αβ} p_α p_β from the full tensors.
            let (dx, dp) = ks_hamiltonian_rhs(1.0, a, &x, &p);
            let dx_ref = raise(&ks_g_up(1.0, a, &x), &p);
            let dg = ks_dg_up(1.0, a, &x);
            let (sx, sp) = scales(a, &x, &p);
            for mu in 0..4 {
                let dp_ref = -0.5 * inner(&dg[mu], &p, &p);
                prop_assert!((dx[mu] - dx_ref[mu]).abs() <= 1e-14 * sx);
                prop_assert!((dp[mu] - dp_ref).abs() <= 1e-13 * sp);
            }
            prop_assert!(dp[0].abs() <= 0.0, "p_t must be conserved exactly");
        }

        #[test]
        fn rhs_is_the_gradient_of_h((a, x) in event(), p in momentum()) {
            // Independent of the analytic derivatives: dx^μ = ∂H/∂p_μ, dp_μ = −∂H/∂x^μ by Richardson finite
            // differences of H itself (h = 1e-3 r for x, 1e-3 for p; error ~1e-12 of scale; margin ×1000).
            let (dx, dp) = ks_hamiltonian_rhs(1.0, a, &x, &p);
            let r = ks_fields(1.0, a, &x).r;
            let (sx, sp) = scales(a, &x, &p);
            for mu in 0..4 {
                let dh_dp = richardson(|s| { let mut q = p; q[mu] += s; ks_hamiltonian(1.0, a, &x, &q) }, 1e-3);
                let dh_dx = richardson(|s| { let mut y = x; y[mu] += s; ks_hamiltonian(1.0, a, &y, &p) }, 1e-3 * r);
                prop_assert!((dx[mu] - dh_dp).abs() <= 1e-9 * sx);
                prop_assert!((dp[mu] + dh_dx).abs() <= 1e-9 * sp, "mu {mu}: {} vs {}", dp[mu], -dh_dx);
            }
        }

        #[test]
        fn hamilton_flow_is_the_geodesic_equation((a, x) in event(), p in momentum()) {
            // With u^μ = g^{μν} p_ν: du^μ/dλ = (∂_α g^{μν}) u^α p_ν + g^{μν} dp_ν/dλ must equal −Γ^μ_{αβ} u^α u^β.
            // This ties the Hamiltonian RHS to the Christoffel symbols (WP-02 acceptance).
            let (u, dp) = ks_hamiltonian_rhs(1.0, a, &x, &p);
            let dg = ks_dg_up(1.0, a, &x);
            let g_up = ks_g_up(1.0, a, &x);
            let gamma = ks_christoffel(1.0, a, &x);
            let fl = ks_fields(1.0, a, &x);
            let tol = 1e-12 * max_abs4(&p).powi(2) * (1.0 + fl.f).powi(4) / fl.r;
            for mu in 0..4 {
                let mut du = 0.0;
                for al in 0..4 {
                    for nu in 0..4 {
                        du += dg[al][mu][nu] * u[al] * p[nu];
                    }
                    du += g_up[mu][al] * dp[al];
                }
                let mut geo = 0.0;
                for al in 0..4 {
                    for be in 0..4 {
                        geo -= gamma[mu][al][be] * u[al] * u[be];
                    }
                }
                prop_assert!((du - geo).abs() <= tol, "mu {mu}: {du} vs {geo}");
            }
        }

        #[test]
        fn hamiltonian_matches_metric_contraction((a, x) in event(), p in momentum()) {
            let h = ks_hamiltonian(1.0, a, &x, &p);
            let h_ref = 0.5 * inner(&ks_g_up(1.0, a, &x), &p, &p);
            let (_, sp) = scales(a, &x, &p);
            prop_assert!((h - h_ref).abs() <= 1e-14 * sp * ks_fields(1.0, a, &x).r);
        }
    }

    #[test]
    fn flat_space_rays_are_straight() {
        // M = 0: dp/dλ = 0 and dx^μ/dλ = η^{μν} p_ν.
        let (dx, dp) = ks_hamiltonian_rhs(0.0_f64, 0.7, &[0.0, 1.0, 2.0, 3.0], &[-1.0, 0.6, 0.0, 0.8]);
        // Exact: f = 0 makes every curvature term vanish (possibly as −0.0, so compare values, not bits).
        assert!(dp.iter().all(|v| v.abs() <= 0.0));
        for (v, e) in dx.iter().zip([1.0_f64, 0.6, 0.0, 0.8]) {
            assert!((v - e).abs() <= 0.0);
        }
    }
}
