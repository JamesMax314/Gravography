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

/// First derivatives of a rank-2 tensor field: `d[α][μ][ν] = ∂_α T_{μν}` (or `∂_α T^{μν}`).
pub type MetricDerivs<T> = [Mat4<T>; 4];

/// Coordinate gradients of the Kerr–Schild fields. Index `α` runs over `[t, x, y, z]`; all `t` components
/// are zero (the fields are stationary).
#[derive(Clone, Copy, Debug)]
pub struct KsFieldDerivs<T> {
    /// `∂_α r`.
    pub dr: Vec4<T>,
    /// `∂_α f`.
    pub df: Vec4<T>,
    /// `dl_lo[α][μ] = ∂_α l_μ`. Since `l^i = l_i` and `l^t = −1`, also `∂_α l^i = dl_lo[α][i]` and `∂_α l^t = 0`.
    pub dl_lo: [Vec4<T>; 4],
}

/// Evaluates the Kerr–Schild fields and their analytic first derivatives at `pos = [t, x, y, z]`
/// (`docs/physics.md` § "Derivatives of the Kerr–Schild fields"):
///
/// - `∂_i r = r³ (x, y, z (r² + a²)/r²)_i / D`, with `D = r⁴ + a² z²`;
/// - `∂_i f = f [3 ∂_i r / r − (4 r³ ∂_i r + 2 a² z δ_iz) / D]`;
/// - `∂_j l_x = (x ∂_j r + r δ_jx + a δ_jy − 2 r l_x ∂_j r) / (r² + a²)`,
///   `∂_j l_y = (y ∂_j r + r δ_jy − a δ_jx − 2 r l_y ∂_j r) / (r² + a²)`,
///   `∂_j l_z = δ_jz / r − z ∂_j r / r²`.
///
/// At `r = 0` (the disk, see [`ks_fields`]) the derivatives are singular or ill-defined. All of them are then
/// returned as zero, consistent with `f = 0` there.
pub fn ks_field_derivatives<T: Real>(mass: T, a: T, pos: &Vec4<T>) -> (KsFields<T>, KsFieldDerivs<T>) {
    let fields = ks_fields(mass, a, pos);
    let zero4 = [T::zero(); 4];
    let r = fields.r;
    let r2 = r * r;
    if r2 <= T::zero() {
        let derivs = KsFieldDerivs {
            dr: zero4,
            df: zero4,
            dl_lo: [zero4; 4],
        };
        return (fields, derivs);
    }
    let [_, x, y, z] = *pos;
    let (two, three, four) = (T::lit(2.0), T::lit(3.0), T::lit(4.0));
    let a2 = a * a;
    let q = r2 + a2;
    let d = r2 * r2 + a2 * z * z;
    let c = r * r2 / d;
    let dr = [T::zero(), x * c, y * c, z * q * r / d];

    let f = fields.f;
    let mut df = zero4;
    for j in 1..4 {
        let dz = if j == 3 { two * a2 * z } else { T::zero() };
        df[j] = f * (three * dr[j] / r - (four * r * r2 * dr[j] + dz) / d);
    }

    let [_, lx, ly, _] = fields.l_lo;
    let delta = |i: usize, j: usize| if i == j { T::one() } else { T::zero() };
    let mut dl_lo = [zero4; 4];
    for (j, dl) in dl_lo.iter_mut().enumerate().skip(1) {
        dl[1] = (x * dr[j] + r * delta(j, 1) + a * delta(j, 2) - two * r * lx * dr[j]) / q;
        dl[2] = (y * dr[j] + r * delta(j, 2) - a * delta(j, 1) - two * r * ly * dr[j]) / q;
        dl[3] = delta(j, 3) / r - z * dr[j] / r2;
    }
    (fields, KsFieldDerivs { dr, df, dl_lo })
}

/// `∂_α (f k_μ k_ν) = ∂_α f k_μ k_ν + f (∂_α k_μ k_ν + k_μ ∂_α k_ν)` for `k = l_lo` or `l_up`.
fn d_f_ll<T: Real>(f: T, k: &Vec4<T>, df: &Vec4<T>, dk: &[Vec4<T>; 4], sign: T) -> MetricDerivs<T> {
    core::array::from_fn(|al| {
        core::array::from_fn(|mu| {
            core::array::from_fn(|nu| {
                sign * (df[al] * k[mu] * k[nu] + f * (dk[al][mu] * k[nu] + k[mu] * dk[al][nu]))
            })
        })
    })
}

/// Analytic derivatives of the covariant metric, `∂_α g_{μν} = ∂_α(f l_μ l_ν)`, at `pos = [t, x, y, z]`.
pub fn ks_dg_lo<T: Real>(mass: T, a: T, pos: &Vec4<T>) -> MetricDerivs<T> {
    let (fl, d) = ks_field_derivatives(mass, a, pos);
    d_f_ll(fl.f, &fl.l_lo, &d.df, &d.dl_lo, T::one())
}

/// Analytic derivatives of the contravariant metric, `∂_α g^{μν} = −∂_α(f l^μ l^ν)`, at `pos = [t, x, y, z]`
/// (`docs/PRD.md` §5.3).
pub fn ks_dg_up<T: Real>(mass: T, a: T, pos: &Vec4<T>) -> MetricDerivs<T> {
    let (fl, d) = ks_field_derivatives(mass, a, pos);
    // ∂l^t = 0 because l^t = −1; spatial components agree with dl_lo.
    let mut dl_up = d.dl_lo;
    for dl in &mut dl_up {
        dl[0] = T::zero();
    }
    d_f_ll(fl.f, &fl.l_up(), &d.df, &dl_up, -T::one())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::linalg::{determinant, identity, inner, mat_mul};
    use crate::test_util::{event, max_abs, max_abs_diff, richardson};
    use approx::assert_relative_eq;
    use proptest::prelude::*;

    /// Machine-precision identities in f64 accumulate O(10) roundings of entries of size `1 + f`; with
    /// products of two entries this gives ≲ 1e-14 · (1 + f)².
    fn identity_tol(f: f64) -> f64 {
        1e-14 * (1.0 + f).powi(2)
    }

    #[test]
    fn radius_far_away_is_euclidean_distance() {
        // r → ρ as ρ → ∞ with corrections O(a²/ρ) (follows from the quartic for r).
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
        // a = 0: g = η + (2M/r) l l with l = (1, x/r, y/r, z/r) (Cartesian form).
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
            // Kerr–Schild form has det g = det η = −1 exactly (Visser 2007). The determinant of
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

    // ---- Derivatives (docs/PRD.md §9 test 2) ----

    /// Finite-difference step relative to the local length scale `r`: Richardson truncation error
    /// ~(h/r)⁴ ≈ 1e-12 and rounding ~ε r / h ≈ 1e-13 relative to the field scale.
    const FD_STEP: f64 = 1e-3;
    /// Agreement of analytic and Richardson finite-difference derivatives, relative to the derivative's
    /// natural scale (field value / r, plus the analytic value). Expected error ~1e-12; margin ×1000.
    const FD_TOL: f64 = 1e-9;

    /// Shifts `pos` by `s` along the spatial axis `j ∈ 1..4`.
    fn shifted(pos: &Vec4<f64>, j: usize, s: f64) -> Vec4<f64> {
        let mut p = *pos;
        p[j] += s;
        p
    }

    proptest! {
        #[test]
        fn field_derivatives_match_finite_differences((a, pos) in event()) {
            let (fl, d) = ks_field_derivatives(1.0, a, &pos);
            let h = FD_STEP * fl.r;
            for j in 1..4 {
                let field = |s: f64| ks_fields(1.0, a, &shifted(&pos, j, s));
                let checks = [
                    (richardson(|s| field(s).r, h), d.dr[j], fl.r),
                    (richardson(|s| field(s).f, h), d.df[j], fl.f),
                    (richardson(|s| field(s).l_lo[1], h), d.dl_lo[j][1], 1.0),
                    (richardson(|s| field(s).l_lo[2], h), d.dl_lo[j][2], 1.0),
                    (richardson(|s| field(s).l_lo[3], h), d.dl_lo[j][3], 1.0),
                ];
                for (k, (fd, an, value)) in checks.into_iter().enumerate() {
                    let scale = value.abs() / fl.r + an.abs();
                    prop_assert!((fd - an).abs() <= FD_TOL * scale, "axis {j}, quantity {k}: fd {fd} vs {an}");
                }
            }
            // Stationarity: no t-dependence, and l_t = 1 is constant.
            prop_assert!(d.dr[0].abs() + d.df[0].abs() <= 0.0);
            prop_assert!(d.dl_lo.iter().all(|dl| dl[0].abs() <= 0.0));
        }

        #[test]
        fn metric_derivatives_match_finite_differences((a, pos) in event()) {
            let dg_lo = ks_dg_lo(1.0, a, &pos);
            let dg_up = ks_dg_up(1.0, a, &pos);
            let r = ks_fields(1.0, a, &pos).r;
            let h = FD_STEP * r;
            for j in 1..4 {
                for mu in 0..4 {
                    for nu in 0..4 {
                        let fd_lo = richardson(|s| ks_g_lo(1.0, a, &shifted(&pos, j, s))[mu][nu], h);
                        let fd_up = richardson(|s| ks_g_up(1.0, a, &shifted(&pos, j, s))[mu][nu], h);
                        // Scale: entries of g are ~(1 + f), varying on the length scale r.
                        let scale = (1.0 + ks_fields(1.0, a, &pos).f) / r;
                        prop_assert!((fd_lo - dg_lo[j][mu][nu]).abs() <= FD_TOL * scale);
                        prop_assert!((fd_up - dg_up[j][mu][nu]).abs() <= FD_TOL * scale);
                    }
                }
            }
            prop_assert!(max_abs(&dg_lo[0]) + max_abs(&dg_up[0]) <= 0.0);
        }

        #[test]
        fn inverse_metric_derivative_identity((a, pos) in event()) {
            // ∂_α g^{μν} = −g^{μρ} (∂_α g_{ρσ}) g^{σν}, from ∂(g g⁻¹) = 0. Entries of size (1 + f)³/r.
            let g_up = ks_g_up(1.0, a, &pos);
            let dg_lo = ks_dg_lo(1.0, a, &pos);
            let dg_up = ks_dg_up(1.0, a, &pos);
            let fl = ks_fields(1.0, a, &pos);
            for al in 1..4 {
                let rhs = mat_mul(&g_up, &mat_mul(&dg_lo[al], &g_up));
                let neg: Mat4<f64> = core::array::from_fn(|i| core::array::from_fn(|j| -rhs[i][j]));
                prop_assert!(max_abs_diff(&dg_up[al], &neg) < 1e-13 * (1.0 + fl.f).powi(3) / fl.r);
            }
        }
    }

    #[test]
    fn derivatives_vanish_for_zero_mass() {
        let pos = [0.0, 1.2, -0.4, 0.7];
        assert!(ks_dg_up(0.0, 0.9, &pos).iter().all(|m| max_abs(m) <= 0.0));
        assert!(ks_dg_lo(0.0, 0.9, &pos).iter().all(|m| max_abs(m) <= 0.0));
    }

    #[test]
    fn schwarzschild_radius_gradient_is_radial_unit_vector() {
        // a = 0: r = |x|, so ∂_i r = x_i / r and f = 2M/r gives ∂_i f = −2M x_i / r³.
        let pos = [0.0, 1.0, 2.0, -2.0];
        let (_, d) = ks_field_derivatives(1.0, 0.0, &pos);
        for j in 1..4 {
            assert_relative_eq!(d.dr[j], pos[j] / 3.0, max_relative = 1e-15);
            assert_relative_eq!(d.df[j], -2.0 * pos[j] / 27.0, max_relative = 1e-14);
        }
    }

    #[test]
    fn f32_derivatives_track_f64() {
        // f32 evaluation of the same formulas: relative error ~ a few ε_f32 · condition ≈ 1e-6; allow 1e-5.
        let (a, pos) = (0.9, [0.0, 2.5, -1.5, 1.0]);
        let d64 = ks_dg_up(1.0, a, &pos);
        let d32 = ks_dg_up(1.0_f32, a as f32, &pos.map(|v| v as f32));
        let scale = d64.iter().map(max_abs).fold(0.0, f64::max);
        for al in 0..4 {
            for mu in 0..4 {
                for nu in 0..4 {
                    let diff = (f64::from(d32[al][mu][nu]) - d64[al][mu][nu]).abs();
                    assert!(diff < 1e-5 * scale, "{al}{mu}{nu}: {diff}");
                }
            }
        }
    }
}
