//! Christoffel symbols of the Kerr–Schild metric, used for observer transport (`docs/PRD.md` §4.2, §5.8).
//!
//! `Γ^μ_{αβ} = ½ g^{μν} (∂_α g_{νβ} + ∂_β g_{να} − ∂_ν g_{αβ})`, computed from the analytic metric derivatives
//! (`docs/physics.md` § "Christoffel symbols"). Rays do not need them: they use the Hamiltonian form.

use crate::kerr_schild::{ks_dg_lo, ks_g_up};
use crate::linalg::Vec4;
use crate::{Kerr, Real};

/// Christoffel symbols of the second kind, `gamma[μ][α][β] = Γ^μ_{αβ}` (symmetric in `α, β`).
pub type Christoffel<T> = [[[T; 4]; 4]; 4];

/// Christoffel symbols `Γ^μ_{αβ}` of the Kerr–Schild metric with mass `mass` and spin `a` at `pos`.
pub fn ks_christoffel<T: Real>(mass: T, a: T, pos: &Vec4<T>) -> Christoffel<T> {
    let dg = ks_dg_lo(mass, a, pos);
    let g_up = ks_g_up(mass, a, pos);
    let half = T::lit(0.5);
    // Christoffel symbols of the first kind: Γ_{ναβ} = ½ (∂_α g_{νβ} + ∂_β g_{να} − ∂_ν g_{αβ}).
    let first: Christoffel<T> = core::array::from_fn(|nu| {
        core::array::from_fn(|al| {
            core::array::from_fn(|be| half * (dg[al][nu][be] + dg[be][nu][al] - dg[nu][al][be]))
        })
    });
    core::array::from_fn(|mu| {
        core::array::from_fn(|al| {
            core::array::from_fn(|be| (0..4).fold(T::zero(), |s, nu| s + g_up[mu][nu] * first[nu][al][be]))
        })
    })
}

impl<T: Real> Kerr<T> {
    /// Christoffel symbols `Γ^μ_{αβ}` at the KS event `pos` (see [`ks_christoffel`]).
    pub fn christoffel(&self, pos: &Vec4<T>) -> Christoffel<T> {
        ks_christoffel(self.mass(), self.spin(), pos)
    }

    /// Geodesic acceleration `−Γ^μ_{αβ} u^α u^β` for 4-velocity `u_up` at `pos` (no thrust).
    pub fn geodesic_acceleration(&self, pos: &Vec4<T>, u_up: &Vec4<T>) -> Vec4<T> {
        let gamma = self.christoffel(pos);
        core::array::from_fn(|mu| {
            let mut s = T::zero();
            for (al, row) in gamma[mu].iter().enumerate() {
                for (be, &g) in row.iter().enumerate() {
                    s = s + g * u_up[al] * u_up[be];
                }
            }
            -s
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kerr_schild::{ks_fields, ks_g_lo};
    use crate::test_util::event;
    use approx::assert_relative_eq;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn christoffels_are_symmetric((a, pos) in event()) {
            let gamma = ks_christoffel(1.0, a, &pos);
            let scale = gamma.iter().flatten().flatten().fold(0.0_f64, |m, v| m.max(v.abs()));
            for mu in 0..4 {
                for al in 0..4 {
                    for be in 0..4 {
                        // The formula is symmetric up to the rounding order of the two ∂g terms.
                        prop_assert!((gamma[mu][al][be] - gamma[mu][be][al]).abs() <= 4.0 * f64::EPSILON * scale);
                    }
                }
            }
        }

        #[test]
        fn connection_is_metric_compatible((a, pos) in event()) {
            // ∇g = 0 ⇔ ∂_γ g_{αβ} = Γ^λ_{γα} g_{λβ} + Γ^λ_{γβ} g_{αλ}. Terms are ~(1 + f)³/r; f64 rounding over
            // ~30 terms gives ≲ 1e-14 of that.
            let gamma = ks_christoffel(1.0, a, &pos);
            let g = ks_g_lo(1.0, a, &pos);
            let dg = ks_dg_lo(1.0, a, &pos);
            let fl = ks_fields(1.0, a, &pos);
            let tol = 1e-13 * (1.0 + fl.f).powi(3) / fl.r;
            for ga in 0..4 {
                for al in 0..4 {
                    for be in 0..4 {
                        let rhs: f64 = (0..4).map(|l| gamma[l][ga][al] * g[l][be] + gamma[l][ga][be] * g[al][l]).sum();
                        prop_assert!((dg[ga][al][be] - rhs).abs() <= tol, "{ga}{al}{be}: {} vs {rhs}", dg[ga][al][be]);
                    }
                }
            }
        }
    }

    #[test]
    fn static_observer_acceleration_in_schwarzschild() {
        // A static observer u = ∂_t / sqrt(1 − 2M/r) has geodesic acceleration −Γ^μ_tt (u^t)². Its magnitude
        // ‖Γ^μ_tt u^t u^t‖ = M / (r² sqrt(1 − 2M/r)) (MTW, static observers), pointing radially inward.
        let k = Kerr::<f64>::schwarzschild();
        let r = 5.0_f64;
        let pos = [0.0, 3.0, 0.0, 4.0];
        let ut = 1.0 / (1.0 - 2.0 / r).sqrt();
        let acc = k.geodesic_acceleration(&pos, &[ut, 0.0, 0.0, 0.0]);
        let g = k.g_lo(&pos);
        let norm = crate::linalg::inner(&g, &acc, &acc).sqrt();
        assert_relative_eq!(norm, 1.0 / (r * r * (1.0 - 2.0 / r).sqrt()), max_relative = 1e-12);
        // Inward: the spatial part is anti-parallel to the position vector.
        let radial = (acc[1] * pos[1] + acc[2] * pos[2] + acc[3] * pos[3]) / r;
        assert!(radial < 0.0);
    }
}
