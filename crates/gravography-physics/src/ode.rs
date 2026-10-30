//! Explicit Runge–Kutta integrators on fixed-size states `[T; N]` (`docs/PRD.md` §5.5).
//!
//! - [`rk4_step`]: classical fourth-order Runge–Kutta (fixed step, baseline/debugging).
//! - [`dopri5_step`]: Dormand–Prince 5(4) with an embedded error estimate and the FSAL property
//!   (Dormand & Prince 1980; tableau as in Hairer, Nørsett & Wanner, *Solving ODEs I*, Table 5.2).
//! - [`error_norm`] and [`StepController`]: the standard scaled RMS error and step-size update.
//!
//! The ray tracer (`crate::ray`) drives these. They know nothing about physics.

use crate::Real;

/// One classical RK4 step of size `h` for `y' = f(y)`.
pub fn rk4_step<T: Real, const N: usize>(f: impl Fn(&[T; N]) -> [T; N], y: &[T; N], h: T) -> [T; N] {
    let half = T::lit(0.5);
    let k1 = f(y);
    let k2 = f(&axpy(y, h * half, &k1));
    let k3 = f(&axpy(y, h * half, &k2));
    let k4 = f(&axpy(y, h, &k3));
    let sixth = h / T::lit(6.0);
    let two = T::lit(2.0);
    core::array::from_fn(|i| y[i] + sixth * (k1[i] + two * (k2[i] + k3[i]) + k4[i]))
}

/// `y + h k`.
fn axpy<T: Real, const N: usize>(y: &[T; N], h: T, k: &[T; N]) -> [T; N] {
    core::array::from_fn(|i| y[i] + h * k[i])
}

/// `y + h Σ_j c_j k_j`.
fn combine<T: Real, const N: usize>(y: &[T; N], h: T, terms: &[(f64, &[T; N])]) -> [T; N] {
    core::array::from_fn(|i| {
        let s = terms.iter().fold(T::zero(), |s, (c, k)| s + T::lit(*c) * k[i]);
        y[i] + h * s
    })
}

/// Result of one Dormand–Prince 5(4) step.
#[derive(Clone, Copy, Debug)]
pub struct Dopri5Step<T, const N: usize> {
    /// Fifth-order solution at `t + h` (used to continue: local extrapolation).
    pub y: [T; N],
    /// Embedded error estimate `y₅ − y₄`.
    pub err: [T; N],
    /// `f(y)` at the new point (FSAL): pass as `k1` to the next step if this one is accepted.
    pub k_last: [T; N],
}

// Dormand–Prince 5(4) tableau.
const A21: f64 = 1.0 / 5.0;
const A31: f64 = 3.0 / 40.0;
const A32: f64 = 9.0 / 40.0;
const A41: f64 = 44.0 / 45.0;
const A42: f64 = -56.0 / 15.0;
const A43: f64 = 32.0 / 9.0;
const A51: f64 = 19372.0 / 6561.0;
const A52: f64 = -25360.0 / 2187.0;
const A53: f64 = 64448.0 / 6561.0;
const A54: f64 = -212.0 / 729.0;
const A61: f64 = 9017.0 / 3168.0;
const A62: f64 = -355.0 / 33.0;
const A63: f64 = 46732.0 / 5247.0;
const A64: f64 = 49.0 / 176.0;
const A65: f64 = -5103.0 / 18656.0;
// Fifth-order weights (= last row of A, FSAL).
const B1: f64 = 35.0 / 384.0;
const B3: f64 = 500.0 / 1113.0;
const B4: f64 = 125.0 / 192.0;
const B5: f64 = -2187.0 / 6784.0;
const B6: f64 = 11.0 / 84.0;
// Error weights E = B(5th) − B*(4th), with B* = (5179/57600, 0, 7571/16695, 393/640, −92097/339200, 187/2100, 1/40).
const E1: f64 = 71.0 / 57600.0;
const E3: f64 = -71.0 / 16695.0;
const E4: f64 = 71.0 / 1920.0;
const E5: f64 = -17253.0 / 339200.0;
const E6: f64 = 22.0 / 525.0;
const E7: f64 = -1.0 / 40.0;

/// One Dormand–Prince 5(4) step of size `h` for `y' = f(y)`, given `k1 = f(y)`.
pub fn dopri5_step<T: Real, const N: usize>(
    f: impl Fn(&[T; N]) -> [T; N],
    y: &[T; N],
    k1: &[T; N],
    h: T,
) -> Dopri5Step<T, N> {
    let k2 = f(&combine(y, h, &[(A21, k1)]));
    let k3 = f(&combine(y, h, &[(A31, k1), (A32, &k2)]));
    let k4 = f(&combine(y, h, &[(A41, k1), (A42, &k2), (A43, &k3)]));
    let k5 = f(&combine(y, h, &[(A51, k1), (A52, &k2), (A53, &k3), (A54, &k4)]));
    let k6 = f(&combine(
        y,
        h,
        &[(A61, k1), (A62, &k2), (A63, &k3), (A64, &k4), (A65, &k5)],
    ));
    let y_new = combine(y, h, &[(B1, k1), (B3, &k3), (B4, &k4), (B5, &k5), (B6, &k6)]);
    let k7 = f(&y_new);
    let zero = [T::zero(); N];
    let err = combine(
        &zero,
        h,
        &[(E1, k1), (E3, &k3), (E4, &k4), (E5, &k5), (E6, &k6), (E7, &k7)],
    );
    Dopri5Step {
        y: y_new,
        err,
        k_last: k7,
    }
}

/// Scaled RMS error `sqrt(1/N Σ (errᵢ / (atol + rtol · max(|yᵢ|, |y_newᵢ|)))²)` (Hairer et al., eq. 4.11).
/// A step is acceptable when this is `≤ 1`.
pub fn error_norm<T: Real, const N: usize>(y: &[T; N], y_new: &[T; N], err: &[T; N], rtol: T, atol: T) -> T {
    let sum = (0..N).fold(T::zero(), |s, i| {
        let sc = atol + rtol * y[i].abs().max(y_new[i].abs());
        let e = err[i] / sc;
        s + e * e
    });
    (sum / T::lit(N as f64)).sqrt()
}

/// Step-size controller for a method whose error estimate is of order `p` (`h_new = h · fac`,
/// `fac = clamp(safety · err^{−1/(p+1)}, fac_min, fac_max)`).
#[derive(Clone, Copy, Debug)]
pub struct StepController<T> {
    /// Safety factor (default 0.9).
    pub safety: T,
    /// Smallest allowed shrink factor per step (default 0.2).
    pub fac_min: T,
    /// Largest allowed growth factor per step (default 5).
    pub fac_max: T,
    /// Order `p` of the embedded error estimate (4 for DOPRI5, so the exponent is −1/5).
    pub order: T,
}

impl<T: Real> StepController<T> {
    /// Defaults for DOPRI5 (Hairer et al. §II.4).
    pub fn dopri5() -> Self {
        Self {
            safety: T::lit(0.9),
            fac_min: T::lit(0.2),
            fac_max: T::lit(5.0),
            order: T::lit(4.0),
        }
    }

    /// Proposed next step size after a step of size `h` with scaled error `err` (accepted or not).
    pub fn next_h(&self, h: T, err: T) -> T {
        let fac = if err > T::zero() {
            self.safety * err.powf(-T::one() / (self.order + T::one()))
        } else {
            self.fac_max
        };
        h * fac.max(self.fac_min).min(self.fac_max)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_relative_eq;

    /// Harmonic oscillator y = (q, p), q' = p, p' = −q; exact solution (cos t, −sin t) from (1, 0).
    fn oscillator(y: &[f64; 2]) -> [f64; 2] {
        [y[1], -y[0]]
    }

    fn integrate_fixed(n: usize, t_end: f64, dopri: bool) -> [f64; 2] {
        let h = t_end / n as f64;
        let mut y = [1.0, 0.0];
        for _ in 0..n {
            y = if dopri {
                dopri5_step(oscillator, &y, &oscillator(&y), h).y
            } else {
                rk4_step(oscillator, &y, h)
            };
        }
        y
    }

    fn global_error(n: usize, dopri: bool) -> f64 {
        let t = 10.0_f64;
        let y = integrate_fixed(n, t, dopri);
        ((y[0] - t.cos()).powi(2) + (y[1] + t.sin()).powi(2)).sqrt()
    }

    #[test]
    fn tableau_is_consistent() {
        // Row sums equal the nodes c = (1/5, 3/10, 4/5, 8/9, 1); weights sum to 1; error weights sum to 0.
        let rows = [
            (A21, 0.2),
            (A31 + A32, 0.3),
            (A41 + A42 + A43, 0.8),
            (A51 + A52 + A53 + A54, 8.0 / 9.0),
            (A61 + A62 + A63 + A64 + A65, 1.0),
        ];
        for (sum, c) in rows {
            assert_relative_eq!(sum, c, epsilon = 1e-14);
        }
        assert_relative_eq!(B1 + B3 + B4 + B5 + B6, 1.0, epsilon = 1e-14);
        assert_relative_eq!(E1 + E3 + E4 + E5 + E6 + E7, 0.0, epsilon = 1e-15);
    }

    #[test]
    fn rk4_is_fourth_order() {
        // Halving h divides the global error by 2⁴ = 16 (asymptotic regime; ±15 % allowance).
        let ratio = global_error(200, false) / global_error(400, false);
        assert_relative_eq!(ratio, 16.0, max_relative = 0.15);
    }

    #[test]
    fn dopri5_is_fifth_order() {
        // Halving h divides the global error of the 5th-order solution by 2⁵ = 32 (±15 %); stay well above
        // rounding (errors here ~1e-8 → 3e-10).
        let ratio = global_error(50, true) / global_error(100, true);
        assert_relative_eq!(ratio, 32.0, max_relative = 0.15);
    }

    #[test]
    fn dopri5_error_estimate_tracks_true_local_error() {
        // The embedded estimate is O(h⁵) while the true local error of the 5th-order solution is O(h⁶):
        // the estimate must be an upper bound, and scale by ≈ 2⁵ when h halves.
        let y0 = [1.0, 0.0];
        let est = |h: f64| {
            let s = dopri5_step(oscillator, &y0, &oscillator(&y0), h);
            let true_err = ((s.y[0] - h.cos()).powi(2) + (s.y[1] + h.sin()).powi(2)).sqrt();
            let est = (s.err[0].powi(2) + s.err[1].powi(2)).sqrt();
            (est, true_err)
        };
        let (e1, t1) = est(0.2);
        let (e2, _) = est(0.1);
        assert!(e1 > t1);
        assert_relative_eq!(e1 / e2, 32.0, max_relative = 0.2);
    }

    #[test]
    fn fsal_stage_is_f_of_new_state() {
        let y0 = [0.3, -0.7];
        let s = dopri5_step(oscillator, &y0, &oscillator(&y0), 0.37);
        let k = oscillator(&s.y);
        assert_relative_eq!(s.k_last[0], k[0]);
        assert_relative_eq!(s.k_last[1], k[1]);
    }

    #[test]
    fn adaptive_driver_meets_tolerance() {
        // Minimal adaptive loop to t = 10 with rtol = atol = 1e-10: the global error should be within a
        // small multiple (here 100×, error accumulation over ~100 steps) of the tolerance.
        let ctl = StepController::<f64>::dopri5();
        let (rtol, atol) = (1e-10, 1e-10);
        let (mut t, mut h, mut y): (f64, f64, [f64; 2]) = (0.0, 0.01, [1.0, 0.0]);
        let mut k1 = oscillator(&y);
        while t < 10.0 {
            h = h.min(10.0 - t);
            let s = dopri5_step(oscillator, &y, &k1, h);
            let e = error_norm(&y, &s.y, &s.err, rtol, atol);
            if e <= 1.0 {
                t += h;
                y = s.y;
                k1 = s.k_last;
            }
            h = ctl.next_h(h, e);
        }
        let err = ((y[0] - 10.0_f64.cos()).powi(2) + (y[1] + 10.0_f64.sin()).powi(2)).sqrt();
        assert!(err < 1e-8, "global error {err}");
    }

    #[test]
    fn controller_bounds() {
        let c = StepController::<f64>::dopri5();
        assert_relative_eq!(c.next_h(1.0, 0.0), 5.0);
        assert_relative_eq!(c.next_h(1.0, 1e12), 0.2);
        assert_relative_eq!(c.next_h(1.0, 1.0), 0.9);
    }

    #[test]
    fn error_norm_is_scaled_rms() {
        let y = [1.0, 100.0];
        let err = [1e-6, 1e-4];
        // Both components are exactly at tolerance with rtol = 1e-6, atol = 0.
        assert_relative_eq!(error_norm(&y, &y, &err, 1e-6, 0.0), 1.0, max_relative = 1e-12);
    }
}
