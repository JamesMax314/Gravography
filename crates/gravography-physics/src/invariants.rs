//! Constants of motion along Kerr geodesics (`docs/PRD.md` §5.3; `docs/physics.md`
//! § "Conserved quantities"): energy `E = −p_t`, axial angular momentum `L_z = x p_y − y p_x`,
//! and Carter's constant `Q = p_θ² + cos²θ (a² (μ² − E²) + L_z² / sin²θ)`, with `μ² = −2H`.

use crate::linalg::Vec4;
use crate::{Kerr, Real};

impl<T: Real> Kerr<T> {
    /// Conserved energy `E = −p_t` (Killing vector `∂_t`, the same in BL and KS).
    pub fn energy(&self, p: &Vec4<T>) -> T {
        -p[0]
    }

    /// Conserved axial angular momentum `L_z = p_φ = x p_y − y p_x` (Killing vector `∂_φ = −y ∂_x + x ∂_y`).
    pub fn angular_momentum_z(&self, x: &Vec4<T>, p: &Vec4<T>) -> T {
        x[1] * p[2] - x[2] * p[1]
    }

    /// Carter constant `Q = p_θ² + cos²θ (a² (μ² − E²) + L_z² / sin²θ)` with `μ² = −2H` (`μ = 0` for photons).
    ///
    /// Evaluated in the regular Cartesian form (`docs/physics.md` § "Carter constant in Cartesian form")
    /// `p_θ² + cot²θ L_z² = cos²θ (r² + a²)(p_x² + p_y²) − 2 z (x p_x + y p_y) p_z + r² sin²θ p_z²`,
    /// which is finite on the spin axis. Undefined (NaN) at `r = 0`.
    pub fn carter_q(&self, x: &Vec4<T>, p: &Vec4<T>) -> T {
        let [_, px, py, pz] = *p;
        let [_, xx, yy, zz] = *x;
        let a = self.spin();
        let r = self.r(x);
        let r2 = r * r;
        let cos2 = zz * zz / r2;
        let sin2 = T::one() - cos2;
        let mu2 = -T::lit(2.0) * self.hamiltonian(x, p);
        let e = self.energy(p);
        cos2 * (r2 + a * a) * (px * px + py * py) - T::lit(2.0) * zz * (xx * px + yy * py) * pz
            + r2 * sin2 * pz * pz
            + cos2 * a * a * (mu2 - e * e)
    }
}

#[cfg(test)]
mod tests {
    use crate::Kerr;
    use crate::linalg::{Vec4, inverse};
    use crate::test_util::{event, max_abs4, momentum, richardson};
    use approx::assert_relative_eq;
    use proptest::prelude::*;
    use std::f64::consts::PI;

    /// Directional derivative of `q(x, p)` along the Hamiltonian vector field, by Richardson differences.
    fn flow_derivative(
        k: &Kerr<f64>,
        x: &Vec4<f64>,
        p: &Vec4<f64>,
        q: impl Fn(&Vec4<f64>, &Vec4<f64>) -> f64,
    ) -> f64 {
        let (dx, dp) = k.hamiltonian_rhs(x, p);
        let h = 1e-3 * k.r(x) / max_abs4(&dx).max(1.0);
        richardson(
            |s| {
                let y: Vec4<f64> = core::array::from_fn(|i| x[i] + s * dx[i]);
                let q_mom: Vec4<f64> = core::array::from_fn(|i| p[i] + s * dp[i]);
                q(&y, &q_mom)
            },
            h,
        )
    }

    proptest! {
        #[test]
        fn invariants_are_conserved_by_the_flow((a, x) in event(), p in momentum()) {
            // {Q, H} = {L_z, H} = {E, H} = {H, H} = 0. Measured as d/dλ along the RHS by Richardson differences:
            // error ~1e-12 of the scale |Q|·|RHS|/r; margin ×1000. Exercises the RHS and the invariant formulas
            // together (proper conservation along integrated rays is WP-03, test 4).
            let k = Kerr::new(a).unwrap();
            let (dx, _) = k.hamiltonian_rhs(&x, &p);
            let scale = (1.0 + k.carter_q(&x, &p).abs() + max_abs4(&p).powi(2) * 100.0) * max_abs4(&dx).max(1.0) / k.r(&x);
            let checks = [
                ("H", flow_derivative(&k, &x, &p, |y, q| k.hamiltonian(y, q))),
                ("E", flow_derivative(&k, &x, &p, |_, q| k.energy(q))),
                ("Lz", flow_derivative(&k, &x, &p, |y, q| k.angular_momentum_z(y, q))),
                ("Q", flow_derivative(&k, &x, &p, |y, q| k.carter_q(y, q))),
            ];
            for (name, d) in checks {
                prop_assert!(d.abs() <= 1e-9 * scale, "d{name}/dλ = {d} (scale {scale})");
            }
        }

        #[test]
        fn invariants_match_boyer_lindquist_definitions(
            a in 0.0..0.99_f64,
            r_u in 0.0..1.0_f64,
            theta in 0.1..PI - 0.1,
            phi in -PI..PI,
            p in momentum(),
        ) {
            // p_BL = Jᵀ p_KS (WP-01 transforms); then E = −p_t, L_z = p_φ and the textbook Q with p_θ from BL.
            let k = Kerr::new(a).unwrap();
            let r = k.outer_horizon() + 0.1 + 30.0 * r_u;
            let bl = [0.0, r, theta, phi];
            let x = k.bl_to_ks_position(&bl);
            let p_bl = k.ks_to_bl_covector(&bl, &p);
            let mu2 = -2.0 * k.hamiltonian(&x, &p);
            let e = -p_bl[0];
            let (s, c) = theta.sin_cos();
            let q_bl = p_bl[2].powi(2) + c * c * (a * a * (mu2 - e * e) + p_bl[3].powi(2) / (s * s));
            let scale = 1.0 + max_abs4(&p_bl).powi(2) / (s * s);
            prop_assert!((k.energy(&p) - e).abs() <= 1e-15);
            prop_assert!((k.angular_momentum_z(&x, &p) - p_bl[3]).abs() <= 1e-13 * (1.0 + p_bl[3].abs()));
            prop_assert!((k.carter_q(&x, &p) - q_bl).abs() <= 1e-12 * scale, "{} vs {q_bl}", k.carter_q(&x, &p));
        }

        #[test]
        fn schwarzschild_q_plus_lz2_is_total_angular_momentum((_a, x) in event(), p in momentum()) {
            // a = 0: Q + L_z² = L² = |x × p|² (spherical symmetry; MTW).
            let k = Kerr::<f64>::schwarzschild();
            let (xs, ps) = ([x[1], x[2], x[3]], [p[1], p[2], p[3]]);
            let cross = [
                xs[1] * ps[2] - xs[2] * ps[1],
                xs[2] * ps[0] - xs[0] * ps[2],
                xs[0] * ps[1] - xs[1] * ps[0],
            ];
            let l2: f64 = cross.iter().map(|c| c * c).sum();
            let lhs = k.carter_q(&x, &p) + k.angular_momentum_z(&x, &p).powi(2);
            prop_assert!((lhs - l2).abs() <= 1e-12 * (1.0 + l2));
        }
    }

    #[test]
    fn spherical_photon_orbit_constants_are_consistent() {
        // Bardeen (1973), docs/PRD.md Appendix A: a spherical photon orbit of radius r has
        // ξ = L_z/E = [r²(3 − r) − a²(r + 1)] / [a (r − 1)], η = Q/E² = r³ [4a² − r(r − 3)²] / [a² (r − 1)²].
        // At the equator with E = 1, p_r = 0 and p_θ = ±√η (since Q = p_θ² there), the BL momentum must be
        // null, and its KS image must reproduce ξ and η through the invariant functions.
        let a = 0.9_f64;
        let k = Kerr::new(a).unwrap();
        for r in [
            k.photon_orbit(true) + 0.05,
            0.5 * (k.photon_orbit(true) + k.photon_orbit(false)),
            k.photon_orbit(false) - 0.05,
        ] {
            let xi = (r * r * (3.0 - r) - a * a * (r + 1.0)) / (a * (r - 1.0));
            let eta = r.powi(3) * (4.0 * a * a - r * (r - 3.0).powi(2)) / (a * a * (r - 1.0).powi(2));
            assert!(eta > 0.0);
            let bl = [0.0, r, PI / 2.0, 0.3];
            let p_bl = [-1.0, 0.0, eta.sqrt(), xi];
            let g_up_bl = inverse(&k.bl_g_lo(r, PI / 2.0)).unwrap();
            let h_bl = 0.5 * crate::linalg::inner(&g_up_bl, &p_bl, &p_bl);
            // H involves terms of size ~ξ²/r² ≈ 1; f64 rounding through the inverse ≲ 1e-14.
            assert!(h_bl.abs() < 1e-12, "H = {h_bl} at r = {r}");

            let x = k.bl_to_ks_position(&bl);
            let p = k.bl_to_ks_covector(&bl, &p_bl).unwrap();
            assert_relative_eq!(k.hamiltonian(&x, &p), 0.0, epsilon = 1e-12);
            assert_relative_eq!(k.energy(&p), 1.0, max_relative = 1e-14);
            assert_relative_eq!(k.angular_momentum_z(&x, &p), xi, max_relative = 1e-12);
            assert_relative_eq!(k.carter_q(&x, &p), eta, max_relative = 1e-12);
        }
    }
}
