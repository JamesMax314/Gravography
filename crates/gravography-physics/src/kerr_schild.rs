//! Kerr metric in ingoing Cartesian Kerr–Schild coordinates `(t, x, y, z)` (`docs/PRD.md` §5.1,
//! derivation in `docs/physics.md` § "Kerr–Schild metric").
//!
//! `g_{μν} = η_{μν} + f l_μ l_ν`, `g^{μν} = η^{μν} − f l^μ l^ν`, with
//! `f = 2 M r³ / (r⁴ + a² z²)` and `l_μ = (1, (r x + a y)/(r² + a²), (r y − a x)/(r² + a²), z / r)`.
//!
//! These are free functions of `(M, a, position)` so they also cover `M = 0` (flat spacetime in oblate
//! spheroidal form). Most code should use the validated [`crate::Kerr`] wrapper instead.

use crate::Real;
use crate::linalg::{Mat4, Vec4, eta};

/// Kerr–Schild radial coordinate `r(x, y, z)`: the non-negative root of
/// `r⁴ − (ρ² − a²) r² − a² z² = 0`, `ρ² = x² + y² + z²`.
///
/// Uses the cancellation-free branch `r² = a² z² / (s − w)` when `w = ½(ρ² − a²) < 0`
/// (inside the oblate shell around the ring), where `s = sqrt(w² + a² z²)` and the naive
/// `r² = w + s` loses all precision (`docs/physics.md` § "Cancellation-free r").
pub fn ks_radius<T: Real>(a: T, x: T, y: T, z: T) -> T {
    let half = T::lit(0.5);
    let a2 = a * a;
    let w = half * (x * x + y * y + z * z - a2);
    let az2 = a2 * z * z;
    let s = (w * w + az2).sqrt();
    let r2 = if w >= T::zero() { w + s } else { az2 / (s - w) };
    r2.sqrt()
}

/// Scalar and null-vector fields of the Kerr–Schild metric at one point.
#[derive(Clone, Copy, Debug)]
pub struct KsFields<T> {
    /// Kerr–Schild (= Boyer–Lindquist) radius `r` (units of `M`).
    pub r: T,
    /// `f = 2 M r³ / (r⁴ + a² z²)` (dimensionless).
    pub f: T,
    /// Covariant null vector `l_μ`, indexed `[t, x, y, z]`, with `l_t = 1`.
    pub l_lo: Vec4<T>,
}

impl<T: Real> KsFields<T> {
    /// Contravariant null vector `l^μ = η^{μν} l_ν` (`l` is null for both `η` and `g`).
    pub fn l_up(&self) -> Vec4<T> {
        [-self.l_lo[0], self.l_lo[1], self.l_lo[2], self.l_lo[3]]
    }
}

/// Evaluates `r`, `f` and `l_μ` at the event `pos = [t, x, y, z]` for mass `mass` and spin `a`.
///
/// On the disk `r = 0` (`z = 0`, `x² + y² ≤ a²`, including the ring singularity and, for `a = 0`, the
/// central point singularity) the fields are returned with `f = 0` and `l_z = 0` instead of NaN. This is
/// the correct limit inside the disk, but **not** at the singularity itself. Callers that may come near
/// `r = 0` must check `r` (`docs/PRD.md` §5.6).
pub fn ks_fields<T: Real>(mass: T, a: T, pos: &Vec4<T>) -> KsFields<T> {
    let [_, x, y, z] = *pos;
    let r = ks_radius(a, x, y, z);
    let r2 = r * r;
    let a2 = a * a;
    if r2 <= T::zero() {
        // Disk interior: f → 0 (docs/physics.md § "Behaviour at r = 0").
        let inv = if a2 > T::zero() { T::one() / a2 } else { T::zero() };
        return KsFields {
            r,
            f: T::zero(),
            l_lo: [T::one(), a * y * inv, -a * x * inv, T::zero()],
        };
    }
    let f = T::lit(2.0) * mass * r * r2 / (r2 * r2 + a2 * z * z);
    let inv = T::one() / (r2 + a2);
    KsFields {
        r,
        f,
        l_lo: [T::one(), (r * x + a * y) * inv, (r * y - a * x) * inv, z / r],
    }
}

/// Covariant metric `g_{μν} = η_{μν} + f l_μ l_ν` at `pos = [t, x, y, z]`.
pub fn ks_g_lo<T: Real>(mass: T, a: T, pos: &Vec4<T>) -> Mat4<T> {
    let fl = ks_fields(mass, a, pos);
    let mut g = eta::<T>();
    for (row, &li) in g.iter_mut().zip(&fl.l_lo) {
        for (gij, &lj) in row.iter_mut().zip(&fl.l_lo) {
            *gij = *gij + fl.f * li * lj;
        }
    }
    g
}

/// Contravariant (inverse) metric `g^{μν} = η^{μν} − f l^μ l^ν` at `pos = [t, x, y, z]`.
pub fn ks_g_up<T: Real>(mass: T, a: T, pos: &Vec4<T>) -> Mat4<T> {
    let fl = ks_fields(mass, a, pos);
    let l = fl.l_up();
    let mut g = eta::<T>();
    for (row, &li) in g.iter_mut().zip(&l) {
        for (gij, &lj) in row.iter_mut().zip(&l) {
            *gij = *gij - fl.f * li * lj;
        }
    }
    g
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::linalg::test_util::{max_abs, max_abs_diff};
    use crate::linalg::{determinant, identity, inner, mat_mul};
    use approx::assert_relative_eq;
    use proptest::prelude::*;

    /// Machine-precision identities in f64 accumulate O(10) roundings of entries of size `1 + f`; with
    /// products of two entries this gives ≲ 1e-14 · (1 + f)².
    fn identity_tol(f: f64) -> f64 {
        1e-14 * (1.0 + f).powi(2)
    }

    /// Random events away from the ring singularity, with extra weight near the axis and equator.
    fn event() -> impl Strategy<Value = (f64, Vec4<f64>)> {
        let spin = 0.0..0.999_f64;
        let coord = -30.0..30.0_f64;
        let tiny = -1e-6..1e-6_f64;
        let generic = (coord.clone(), coord.clone(), coord.clone()).prop_map(|(x, y, z)| [x, y, z]);
        let near_axis = (tiny.clone(), tiny.clone(), coord.clone()).prop_map(|(x, y, z)| [x, y, z]);
        let near_equator = (coord.clone(), coord, tiny).prop_map(|(x, y, z)| [x, y, z]);
        let near_hole = (-3.0..3.0_f64, -3.0..3.0_f64, -3.0..3.0_f64).prop_map(|(x, y, z)| [x, y, z]);
        (
            spin,
            prop_oneof![generic, near_axis, near_equator, near_hole],
            -100.0..100.0_f64,
        )
            .prop_filter("stay away from the ring singularity", |(a, [x, y, z], _)| {
                ks_radius(*a, *x, *y, *z) > 0.3
            })
            .prop_map(|(a, [x, y, z], t)| (a, [t, x, y, z]))
    }

    #[test]
    fn radius_far_away_is_euclidean_distance() {
        // r → ρ as ρ → ∞ with corrections O(a²/ρ) (Visser 2007, eq. for r(x, y, z)).
        let (a, x, y, z): (f64, f64, f64, f64) = (0.9, 3.0e3, -4.0e3, 1.2e4);
        let rho = (x * x + y * y + z * z).sqrt();
        assert_relative_eq!(ks_radius(a, x, y, z), rho, max_relative = a * a / (rho * rho));
    }

    #[test]
    fn radius_is_spheroidal_root() {
        // Spheroidal coordinates: x + iy = (r + ia) sinθ e^{iφ}, z = r cosθ (docs/PRD.md §5.3).
        // Includes tiny r inside the shell, where the naive formula r² = w + s cancels catastrophically.
        let a = 0.9_f64;
        for &r in &[1e-6, 1e-3, 0.1, 0.5, 1.0, 2.0, 50.0] {
            for &theta in &[0.1, 0.7, 1.5, std::f64::consts::FRAC_PI_2 - 1e-4, 2.9] {
                let phi = 0.3_f64;
                let (s, c) = theta.sin_cos();
                let x = s * (r * phi.cos() - a * phi.sin());
                let y = s * (r * phi.sin() + a * phi.cos());
                let z = r * c;
                // The inputs only determine w = ½(ρ² − a²) = ½(r² − a² cos²θ) to an absolute ~ε·a²
                // (rounding of x, y of size ~a), so the achievable relative accuracy of r is
                // ~ε · a² / (r² + a² cos²θ) — the condition number of the problem, not of the formula.
                // The naive branch r² = w + s would instead lose everything for r ≪ a cosθ.
                let cond = (a * a / (r * r + a * a * c * c)).max(1.0);
                assert_relative_eq!(ks_radius(a, x, y, z), r, max_relative = 1e-13 * cond);
            }
        }
    }

    #[test]
    fn radius_in_f32_stays_accurate_inside_shell() {
        // f32 epsilon ≈ 1.2e-7; the cancellation-free branch keeps r accurate to a few ε at r ≪ a
        // (the naive branch would give r = 0 or garbage here).
        let (a, r, theta) = (0.9_f32, 1e-2_f32, 1.0_f32);
        let x = theta.sin() * r;
        let y = theta.sin() * a;
        let z = r * theta.cos();
        assert_relative_eq!(ks_radius(a, x, y, z), r, max_relative = 1e-5);
    }

    #[test]
    fn zero_mass_is_minkowski() {
        let pos = [0.0, 0.3, -0.2, 0.1];
        assert!(max_abs_diff(&ks_g_lo(0.0, 0.7, &pos), &eta()) < 1e-16);
        assert!(max_abs_diff(&ks_g_up(0.0, 0.7, &pos), &eta()) < 1e-16);
    }

    #[test]
    fn schwarzschild_is_ingoing_eddington_finkelstein() {
        // a = 0: g = η + (2M/r) l l with l = (1, x/r, y/r, z/r) (MTW §31.4, Cartesian form).
        let pos = [1.0, 2.0, -3.0, 4.0];
        let r = (4.0_f64 + 9.0 + 16.0).sqrt();
        let l = [1.0, 2.0 / r, -3.0 / r, 4.0 / r];
        let g = ks_g_lo(1.0, 0.0, &pos);
        for i in 0..4 {
            for j in 0..4 {
                let expected = eta::<f64>()[i][j] + 2.0 / r * l[i] * l[j];
                assert_relative_eq!(g[i][j], expected, epsilon = 1e-15);
            }
        }
    }

    #[test]
    fn disk_interior_is_finite() {
        // Inside the disk r = 0, z = 0, ρ < a the metric is flat (f → 0) and must not be NaN.
        let pos = [0.0_f64, 0.2, 0.1, 0.0];
        let fl = ks_fields(1.0, 0.9, &pos);
        assert_relative_eq!(fl.r, 0.0);
        let g = ks_g_lo(1.0, 0.9, &pos);
        assert!(g.iter().flatten().all(|v| v.is_finite()));
        assert!(max_abs_diff(&g, &eta()) < 1e-16);
    }

    #[test]
    fn f32_identity_holds_to_single_precision() {
        // f32: ε ≈ 1.2e-7; entries ≲ 1 + f ≈ 2 here, ~10 operations → ≲ 1e-5.
        let pos = [0.0_f32, 3.0, -2.0, 1.5];
        let prod = mat_mul(&ks_g_lo(1.0, 0.9, &pos), &ks_g_up(1.0, 0.9, &pos));
        for (i, row) in prod.iter().enumerate() {
            for (j, &v) in row.iter().enumerate() {
                let e = if i == j { 1.0 } else { 0.0 };
                assert!((v - e).abs() < 1e-5, "({i},{j}) = {v}");
            }
        }
    }

    proptest! {
        #[test]
        fn metric_times_inverse_is_identity((a, pos) in event()) {
            let f = ks_fields(1.0, a, &pos).f;
            let prod = mat_mul(&ks_g_lo(1.0, a, &pos), &ks_g_up(1.0, a, &pos));
            prop_assert!(max_abs_diff(&prod, &identity()) < identity_tol(f));
        }

        #[test]
        fn determinant_is_minus_one((a, pos) in event()) {
            // Kerr–Schild form has det g = det η = −1 exactly (Visser 2007 §3). The determinant of
            // entries of size (1 + f) has cancellation of order (1 + f)⁴ ε.
            let f = ks_fields(1.0, a, &pos).f;
            let g = ks_g_lo(1.0, a, &pos);
            prop_assert!((determinant(&g) + 1.0).abs() < 1e-14 * (1.0 + f).powi(4));
        }

        #[test]
        fn l_is_null_for_eta_and_g((a, pos) in event()) {
            let fl = ks_fields(1.0, a, &pos);
            let l = fl.l_up();
            let g = ks_g_lo(1.0, a, &pos);
            prop_assert!(inner(&eta(), &l, &l).abs() < 1e-14);
            prop_assert!(inner(&g, &l, &l).abs() < identity_tol(fl.f));
        }

        #[test]
        fn metric_is_symmetric_and_stationary((a, pos) in event(), dt in -50.0..50.0_f64) {
            let g = ks_g_lo(1.0, a, &pos);
            let shifted = ks_g_lo(1.0, a, &[pos[0] + dt, pos[1], pos[2], pos[3]]);
            // t is cyclic: the metric must not depend on it at all (bit-identical).
            prop_assert!(max_abs_diff(&g, &shifted) <= 0.0);
            // f l_i l_j and f l_j l_i round differently: allow a few ε of the largest entry.
            prop_assert!(max_abs_diff(&g, &crate::linalg::transpose(&g)) < 4.0 * f64::EPSILON * max_abs(&g));
        }
    }
}
