//! Boyer–Lindquist (BL) coordinates `(t, r, θ, φ)` and transforms to and from ingoing Cartesian Kerr–Schild
//! (KS) coordinates `(t, x, y, z)` (`docs/PRD.md` §2, §5.1; derivation in `docs/physics.md`
//! § "Boyer–Lindquist ↔ Kerr–Schild").
//!
//! The KS event for a BL event is
//! `t_KS = t + T(r)`, `x + i y = (r + i a) sinθ e^{i φ̃}`, `z = r cosθ`, `φ̃ = φ + Φ(r)`,
//! with `T'(r) = 2Mr/Δ` and `Φ'(r) = a/Δ`. BL is singular on the horizons (`Δ = 0`) and on the
//! axis (`sinθ = 0`). Transforms are valid in each region `r > r_+` and `r_− < r < r_+` (and `0 < r < r_−`)
//! separately. Vector and covector transforms use the Jacobian `J^μ_ν = ∂x_KS^μ / ∂x_BL^ν`.

use crate::linalg::{Mat4, Vec4, inverse, mat_vec, transpose};
use crate::{Kerr, PhysicsError, Real};

impl<T: Real> Kerr<T> {
    /// `Δ(r) = r² − 2Mr + a²`.
    pub fn delta(&self, r: T) -> T {
        r * r - T::lit(2.0) * self.mass() * r + self.spin() * self.spin()
    }

    /// `Σ(r, θ) = r² + a² cos²θ`.
    pub fn sigma(&self, r: T, theta: T) -> T {
        let c = theta.cos();
        r * r + self.spin() * self.spin() * c * c
    }

    /// Covariant Kerr metric in BL coordinates `(t, r, θ, φ)` at `(r, θ)` (standard form, e.g. Visser 2007
    /// eq. 1): `g_tt = −(1 − 2Mr/Σ)`, `g_tφ = −2Mar sin²θ/Σ`, `g_rr = Σ/Δ`, `g_θθ = Σ`,
    /// `g_φφ = (r² + a² + 2Ma²r sin²θ/Σ) sin²θ`.
    pub fn bl_g_lo(&self, r: T, theta: T) -> Mat4<T> {
        let (m, a) = (self.mass(), self.spin());
        let two = T::lit(2.0);
        let sigma = self.sigma(r, theta);
        let s2 = theta.sin().powi(2);
        let z = T::zero();
        let g_tt = -(T::one() - two * m * r / sigma);
        let g_tp = -two * m * a * r * s2 / sigma;
        let g_rr = sigma / self.delta(r);
        let g_pp = (r * r + a * a + two * m * a * a * r * s2 / sigma) * s2;
        [
            [g_tt, z, z, g_tp],
            [z, g_rr, z, z],
            [z, z, sigma, z],
            [g_tp, z, z, g_pp],
        ]
    }

    /// Time offset `T(r) = t_KS − t_BL`, the antiderivative of `2Mr/Δ`:
    /// `T = (2M / (r_+ − r_−)) (r_+ ln|r − r_+| − r_− ln|r − r_−|)`.
    pub fn ks_time_offset(&self, r: T) -> T {
        let (rp, rm) = (self.outer_horizon(), self.inner_horizon());
        T::lit(2.0) * self.mass() / (rp - rm) * (rp * (r - rp).abs().ln() - rm * (r - rm).abs().ln())
    }

    /// Azimuth offset `Φ(r) = φ̃ − φ_BL`, the antiderivative of `a/Δ`:
    /// `Φ = (a / (r_+ − r_−)) ln|(r − r_+)/(r − r_−)|`.
    pub fn ks_phi_offset(&self, r: T) -> T {
        let (rp, rm) = (self.outer_horizon(), self.inner_horizon());
        self.spin() / (rp - rm) * ((r - rp) / (r - rm)).abs().ln()
    }

    /// Converts a BL event `[t, r, θ, φ]` to a KS event `[t, x, y, z]`.
    pub fn bl_to_ks_position(&self, bl: &Vec4<T>) -> Vec4<T> {
        let [t, r, theta, phi] = *bl;
        let a = self.spin();
        let (st, ct) = theta.sin_cos();
        let (sp, cp) = (phi + self.ks_phi_offset(r)).sin_cos();
        [
            t + self.ks_time_offset(r),
            st * (r * cp - a * sp),
            st * (r * sp + a * cp),
            r * ct,
        ]
    }

    /// Converts a KS event `[t, x, y, z]` (with `r > 0`, off the horizons) to a BL event `[t, r, θ, φ]`,
    /// `θ ∈ [0, π]`, `φ ∈ (−π, π]`. On the axis `φ` is undefined and an arbitrary value is returned.
    pub fn ks_to_bl_position(&self, ks: &Vec4<T>) -> Vec4<T> {
        let [t, x, y, z] = *ks;
        let r = self.r(ks);
        let theta = (z / r).max(-T::one()).min(T::one()).acos();
        // arg(x + i y) = φ̃ + arg(r + i a) for sinθ > 0.
        let phi_ks = y.atan2(x) - self.spin().atan2(r);
        let phi = phi_ks - self.ks_phi_offset(r);
        [t - self.ks_time_offset(r), r, theta, wrap_angle(phi)]
    }

    /// Jacobian `J^μ_ν = ∂x_KS^μ / ∂x_BL^ν` at the BL event `bl` (rows `t, x, y, z`, columns `t, r, θ, φ`).
    pub fn jacobian_ks_from_bl(&self, bl: &Vec4<T>) -> Mat4<T> {
        let [_, r, theta, _] = *bl;
        let [_, x, y, _] = self.bl_to_ks_position(bl);
        let a = self.spin();
        let delta = self.delta(r);
        let dt_dr = T::lit(2.0) * self.mass() * r / delta;
        let dphi_dr = a / delta;
        let (st, ct) = theta.sin_cos();
        let (sp, cp) = (bl[3] + self.ks_phi_offset(r)).sin_cos();
        let (o, z) = (T::one(), T::zero());
        // ∂x/∂φ̃ = −y, ∂y/∂φ̃ = x, and φ̃ depends on r through Φ(r).
        [
            [o, dt_dr, z, z],
            [z, st * cp - y * dphi_dr, ct * (r * cp - a * sp), -y],
            [z, st * sp + x * dphi_dr, ct * (r * sp + a * cp), x],
            [z, ct, -r * st, z],
        ]
    }

    /// Transforms a contravariant vector from BL to KS components at the BL event `bl`: `v_KS = J v_BL`.
    pub fn bl_to_ks_vector(&self, bl: &Vec4<T>, v_bl: &Vec4<T>) -> Vec4<T> {
        mat_vec(&self.jacobian_ks_from_bl(bl), v_bl)
    }

    /// Transforms a contravariant vector from KS to BL components at the BL event `bl`: `v_BL = J⁻¹ v_KS`.
    ///
    /// # Errors
    /// [`PhysicsError::CoordinateSingularity`] on the axis or a horizon, where `J` is singular.
    pub fn ks_to_bl_vector(&self, bl: &Vec4<T>, v_ks: &Vec4<T>) -> Result<Vec4<T>, PhysicsError> {
        Ok(mat_vec(&self.inverse_jacobian(bl)?, v_ks))
    }

    /// Transforms a covector from KS to BL components at the BL event `bl`: `p_BL = Jᵀ p_KS`.
    pub fn ks_to_bl_covector(&self, bl: &Vec4<T>, p_ks: &Vec4<T>) -> Vec4<T> {
        mat_vec(&transpose(&self.jacobian_ks_from_bl(bl)), p_ks)
    }

    /// Transforms a covector from BL to KS components at the BL event `bl`: `p_KS = J⁻ᵀ p_BL`.
    ///
    /// # Errors
    /// [`PhysicsError::CoordinateSingularity`] on the axis or a horizon, where `J` is singular.
    pub fn bl_to_ks_covector(&self, bl: &Vec4<T>, p_bl: &Vec4<T>) -> Result<Vec4<T>, PhysicsError> {
        Ok(mat_vec(&transpose(&self.inverse_jacobian(bl)?), p_bl))
    }

    fn inverse_jacobian(&self, bl: &Vec4<T>) -> Result<Mat4<T>, PhysicsError> {
        inverse(&self.jacobian_ks_from_bl(bl)).ok_or(PhysicsError::CoordinateSingularity(
            "BL Jacobian is singular (axis or horizon)",
        ))
    }
}

/// Wraps an angle into `(−π, π]`.
fn wrap_angle<T: Real>(phi: T) -> T {
    let two_pi = T::lit(2.0) * T::PI();
    let w = phi - two_pi * ((phi + T::PI()) / two_pi).floor();
    // w ∈ [−π, π); map −π to π.
    if w <= -T::PI() { w + two_pi } else { w }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::linalg::test_util::{max_abs, max_abs_diff};
    use crate::linalg::{inner, mat_mul, raise};
    use approx::assert_relative_eq;
    use proptest::prelude::*;
    use std::f64::consts::{FRAC_PI_2, PI};

    /// Random spin and BL event with `r` outside, between or (rarely) inside the horizons, kept a margin
    /// `|r − r_±| > 0.05` away from them and `sinθ > 0.05` away from the axis, where BL is singular.
    fn bl_event() -> impl Strategy<Value = (f64, Vec4<f64>)> {
        (
            0.0..0.99_f64,
            0.0..1.0_f64,
            0.05..PI - 0.05,
            -PI..PI,
            -50.0..50.0_f64,
            any::<bool>(),
        )
            .prop_filter_map("keep away from horizons", |(a, u, theta, phi, t, inside)| {
                let k = Kerr::new(a).unwrap();
                let (rp, rm) = (k.outer_horizon(), k.inner_horizon());
                let r = if inside {
                    rm + (rp - rm) * u
                } else {
                    rp + 0.05 + 60.0 * u
                };
                ((r - rp).abs() > 0.05 && (r - rm).abs() > 0.05).then_some((a, [t, r, theta, phi]))
            })
    }

    /// Relative-to-size comparison of 4-vectors.
    fn close(a: &Vec4<f64>, b: &Vec4<f64>, rel: f64) -> bool {
        let scale = b.iter().fold(1.0_f64, |m, v| m.max(v.abs()));
        a.iter().zip(b).all(|(x, y)| (x - y).abs() <= rel * scale)
    }

    #[test]
    fn schwarzschild_time_offset_is_eddington_finkelstein() {
        // a = 0: v = t + r + 2M ln|r/2M − 1| and t_KS = v − r, i.e. T(r) = 2M ln|r − 2M| up to a constant
        // (MTW §31.4). Our constant choice makes it exactly 2 ln|r − 2|.
        let k = Kerr::<f64>::schwarzschild();
        for r in [0.5, 1.5, 2.5, 10.0, 1e3] {
            assert_relative_eq!(
                k.ks_time_offset(r),
                2.0 * (r - 2.0_f64).abs().ln(),
                epsilon = 1e-14
            );
            assert_relative_eq!(k.ks_phi_offset(r), 0.0);
        }
    }

    #[test]
    fn offsets_have_the_right_derivatives() {
        // Central differences, h = 1e-5: truncation O(h²·T''') ≈ 1e-10 relative at r = 4, rounding
        // O(ε/h) ≈ 1e-11.
        let k = Kerr::new(0.8_f64).unwrap();
        let h = 1e-5;
        for r in [0.3, 1.2, 4.0, 25.0] {
            let dt = (k.ks_time_offset(r + h) - k.ks_time_offset(r - h)) / (2.0 * h);
            let dphi = (k.ks_phi_offset(r + h) - k.ks_phi_offset(r - h)) / (2.0 * h);
            let delta = k.delta(r);
            assert_relative_eq!(dt, 2.0 * r / delta, max_relative = 1e-8);
            assert_relative_eq!(dphi, 0.8 / delta, max_relative = 1e-8);
        }
    }

    #[test]
    fn wrap_angle_range() {
        for (input, expected) in [
            (0.0, 0.0),
            (PI, PI),
            (-PI, PI),
            (3.0 * PI, PI),
            (-0.5, -0.5),
            (7.0, 7.0 - 2.0 * PI),
        ] {
            assert_relative_eq!(wrap_angle(input), expected, epsilon = 1e-14);
        }
    }

    #[test]
    fn frame_dragging_sign_for_positive_spin() {
        // ZAMO: u_μ ∝ −∇_μ t_BL, i.e. p_BL = (−1, 0, 0, 0). Its angular velocity dφ/dt = u^φ/u^t is
        // ω = 2Mar / ((r² + a²)² − Δ a² sin²θ) > 0 for a > 0 (Bardeen, Press & Teukolsky 1972, eq. 3.3).
        // Computed here entirely through the KS metric and the BL ↔ KS transforms, which checks that
        // a > 0 means rotation in the +φ sense (docs/PRD.md §2).
        let k = Kerr::new(0.9_f64).unwrap();
        for (r, theta) in [(3.0, FRAC_PI_2), (1.7, 1.0), (10.0, 0.4)] {
            let bl = [0.0, r, theta, 0.7];
            let p_ks = k.bl_to_ks_covector(&bl, &[-1.0, 0.0, 0.0, 0.0]).unwrap();
            let u_ks = raise(&k.g_up(&k.bl_to_ks_position(&bl)), &p_ks);
            let u_bl = k.ks_to_bl_vector(&bl, &u_ks).unwrap();
            let omega = u_bl[3] / u_bl[0];
            let a = 0.9;
            let expected = 2.0 * a * r / ((r * r + a * a).powi(2) - k.delta(r) * a * a * theta.sin().powi(2));
            assert!(omega > 0.0, "ZAMO must co-rotate with the hole");
            // Four Jacobian/metric evaluations in f64: ~1e-14; margin for J⁻¹ conditioning.
            assert_relative_eq!(omega, expected, max_relative = 1e-11);
            assert_relative_eq!(u_bl[1], 0.0, epsilon = 1e-12);
            assert_relative_eq!(u_bl[2], 0.0, epsilon = 1e-12);
        }
    }

    #[test]
    fn axis_vector_transform_reports_singularity() {
        let k = Kerr::new(0.5_f64).unwrap();
        let bl = [0.0, 5.0, 0.0, 0.0];
        assert!(matches!(
            k.ks_to_bl_vector(&bl, &[1.0, 0.0, 0.0, 0.0]),
            Err(PhysicsError::CoordinateSingularity(_))
        ));
    }

    proptest! {
        #[test]
        fn position_round_trip((a, bl) in bl_event()) {
            // f64 round trip: atan2/acos/ln near r_± margin amplify rounding by ≲ 1/|r − r_±| ≈ 20.
            let k = Kerr::new(a).unwrap();
            let back = k.ks_to_bl_position(&k.bl_to_ks_position(&bl));
            prop_assert!(close(&back, &bl, 1e-11), "{bl:?} -> {back:?}");
        }

        #[test]
        fn ks_radius_matches_bl_radius((a, bl) in bl_event()) {
            let k = Kerr::new(a).unwrap();
            prop_assert!((k.r(&k.bl_to_ks_position(&bl)) - bl[1]).abs() < 1e-13 * bl[1].max(1.0));
        }

        #[test]
        fn jacobian_matches_finite_differences((a, bl) in bl_event()) {
            // Central differences with h = 1e-6 · scale: truncation ~h² · |∂³x| and rounding ~ε/h; the r
            // column involves 1/Δ (≲ 1/0.05²), so compare with 1e-6 relative to the column norm.
            let k = Kerr::new(a).unwrap();
            let jac = k.jacobian_ks_from_bl(&bl);
            for col in 0..4 {
                let h = 1e-6 * bl[col].abs().max(1.0);
                let mut plus = bl;
                let mut minus = bl;
                plus[col] += h;
                minus[col] -= h;
                let (xp, xm) = (k.bl_to_ks_position(&plus), k.bl_to_ks_position(&minus));
                let fd: Vec4<f64> = core::array::from_fn(|row| (xp[row] - xm[row]) / (2.0 * h));
                let analytic: Vec4<f64> = core::array::from_fn(|row| jac[row][col]);
                prop_assert!(close(&fd, &analytic, 1e-6), "col {col}: fd {fd:?} vs {analytic:?}");
            }
        }

        #[test]
        fn ks_metric_pulls_back_to_bl_metric((a, bl) in bl_event()) {
            // g_BL = Jᵀ g_KS J: the strongest check that the KS metric, the coordinate map and the sign of
            // a are mutually consistent. Entries of J reach 1/|Δ| ~ 1e3 near the horizon margins, so the
            // comparison is relative to the largest entry of Jᵀ |g_KS| J.
            let k = Kerr::new(a).unwrap();
            let jac = k.jacobian_ks_from_bl(&bl);
            let g_ks = k.g_lo(&k.bl_to_ks_position(&bl));
            let pulled = mat_mul(&transpose(&jac), &mat_mul(&g_ks, &jac));
            let g_bl = k.bl_g_lo(bl[1], bl[2]);
            let scale = max_abs(&g_ks) * max_abs(&jac).powi(2);
            prop_assert!(max_abs_diff(&pulled, &g_bl) < 1e-13 * scale, "{pulled:?}\nvs\n{g_bl:?}");
        }

        #[test]
        fn vector_and_covector_transforms_preserve_contractions(
            (a, bl) in bl_event(),
            v in proptest::array::uniform4(-1.0..1.0_f64),
            p in proptest::array::uniform4(-1.0..1.0_f64),
        ) {
            // p_μ v^μ and g(v, v) are coordinate invariants; round trips are identities.
            let k = Kerr::new(a).unwrap();
            let v_ks = k.bl_to_ks_vector(&bl, &v);
            let p_ks = k.bl_to_ks_covector(&bl, &p).unwrap();
            let contraction_bl: f64 = v.iter().zip(&p).map(|(x, y)| x * y).sum();
            let contraction_ks: f64 = v_ks.iter().zip(&p_ks).map(|(x, y)| x * y).sum();
            let tol = 1e-12 * max_abs(&k.jacobian_ks_from_bl(&bl)).powi(2);
            prop_assert!((contraction_bl - contraction_ks).abs() < tol);

            let norm_bl = inner(&k.bl_g_lo(bl[1], bl[2]), &v, &v);
            let norm_ks = inner(&k.g_lo(&k.bl_to_ks_position(&bl)), &v_ks, &v_ks);
            prop_assert!((norm_bl - norm_ks).abs() < tol * max_abs(&k.bl_g_lo(bl[1], bl[2])));

            prop_assert!(close(&k.ks_to_bl_vector(&bl, &v_ks).unwrap(), &v, tol));
            prop_assert!(close(&k.ks_to_bl_covector(&bl, &p_ks), &p, tol));
        }
    }
}
