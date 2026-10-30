//! Validated Kerr spacetime parameters, the Kerr–Schild metric wrapper, and characteristic radii
//! (`docs/PRD.md` §5.1–5.2, Appendix A).

use crate::kerr_schild::{KsFields, ks_fields, ks_g_lo, ks_g_up};
use crate::linalg::{Mat4, Vec4};
use crate::{PhysicsError, Real};

/// A Kerr black hole with mass `M > 0` and spin parameter `a = J/M` (a length, units of `M`), `|a| < M`.
///
/// The spin axis is `+z`. `a > 0` rotates in the `+φ` sense. With the project units (`M = 1`, `docs/PRD.md` §2)
/// use [`Kerr::new`]. `a = 0` is Schwarzschild.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Kerr<T> {
    mass: T,
    a: T,
}

impl<T: Real> Kerr<T> {
    /// Kerr black hole with `M = 1` and spin `a`, `|a| < 1`.
    ///
    /// # Errors
    /// [`PhysicsError::InvalidParameters`] if `|a| ≥ 1` or `a` is not finite.
    pub fn new(a: T) -> Result<Self, PhysicsError> {
        Self::with_mass(T::one(), a)
    }

    /// Kerr black hole with mass `mass > 0` and spin parameter `a`, `|a| < mass` (sub-extremal).
    ///
    /// # Errors
    /// [`PhysicsError::InvalidParameters`] if the parameters are outside that range or not finite.
    pub fn with_mass(mass: T, a: T) -> Result<Self, PhysicsError> {
        let err = |reason| PhysicsError::InvalidParameters {
            mass: mass.to_f64_lossy(),
            spin: a.to_f64_lossy(),
            reason,
        };
        if mass.is_nan() || mass <= T::zero() || !mass.is_finite() {
            return Err(err("mass must be positive and finite"));
        }
        if a.is_nan() || a.abs() >= mass {
            return Err(err("spin must satisfy |a| < M (sub-extremal)"));
        }
        Ok(Self { mass, a })
    }

    /// Schwarzschild black hole of mass 1 (`a = 0`).
    pub fn schwarzschild() -> Self {
        Self {
            mass: T::one(),
            a: T::zero(),
        }
    }

    /// Mass `M`.
    pub fn mass(&self) -> T {
        self.mass
    }

    /// Spin parameter `a = J/M` (units of `M`).
    pub fn spin(&self) -> T {
        self.a
    }

    /// Dimensionless spin `χ = a / M`.
    pub fn chi(&self) -> T {
        self.a / self.mass
    }

    /// Kerr–Schild radius `r` of the event `pos = [t, x, y, z]`.
    pub fn r(&self, pos: &Vec4<T>) -> T {
        crate::kerr_schild::ks_radius(self.a, pos[1], pos[2], pos[3])
    }

    /// Metric fields `r`, `f`, `l_μ` at `pos` (see [`ks_fields`]).
    pub fn fields(&self, pos: &Vec4<T>) -> KsFields<T> {
        ks_fields(self.mass, self.a, pos)
    }

    /// Covariant Kerr–Schild metric `g_{μν}` at `pos = [t, x, y, z]`.
    pub fn g_lo(&self, pos: &Vec4<T>) -> Mat4<T> {
        ks_g_lo(self.mass, self.a, pos)
    }

    /// Contravariant Kerr–Schild metric `g^{μν}` at `pos = [t, x, y, z]`.
    pub fn g_up(&self, pos: &Vec4<T>) -> Mat4<T> {
        ks_g_up(self.mass, self.a, pos)
    }

    // ---- Characteristic radii (docs/PRD.md Appendix A; Bardeen, Press & Teukolsky 1972) ----

    /// `sqrt(M² − a²)`, half the horizon separation.
    fn horizon_half_gap(&self) -> T {
        (self.mass * self.mass - self.a * self.a).sqrt()
    }

    /// Outer (event) horizon `r_+ = M + sqrt(M² − a²)`.
    pub fn outer_horizon(&self) -> T {
        self.mass + self.horizon_half_gap()
    }

    /// Inner (Cauchy) horizon `r_− = M − sqrt(M² − a²)`.
    pub fn inner_horizon(&self) -> T {
        self.mass - self.horizon_half_gap()
    }

    /// Outer ergosurface `r_E(θ) = M + sqrt(M² − a² cos²θ)`, `θ` the polar angle from `+z`.
    pub fn ergosurface(&self, theta: T) -> T {
        let c = theta.cos();
        self.mass + (self.mass * self.mass - self.a * self.a * c * c).sqrt()
    }

    /// Equatorial circular photon orbit radius `r_ph = 2M {1 + cos[(2/3) arccos(∓χ)]}`.
    /// `prograde = true` selects the co-rotating orbit (upper sign).
    pub fn photon_orbit(&self, prograde: bool) -> T {
        let chi = if prograde { -self.chi() } else { self.chi() };
        let two = T::lit(2.0);
        two * self.mass * (T::one() + (two / T::lit(3.0) * chi.acos()).cos())
    }

    /// Innermost stable circular orbit radius (Bardeen, Press & Teukolsky 1972, eq. 2.21):
    /// `Z₁ = 1 + (1 − χ²)^{1/3}[(1 + χ)^{1/3} + (1 − χ)^{1/3}]`, `Z₂ = sqrt(3χ² + Z₁²)`,
    /// `r = M (3 + Z₂ ∓ sqrt((3 − Z₁)(3 + Z₁ + 2Z₂)))`, upper sign prograde.
    pub fn isco(&self, prograde: bool) -> T {
        let chi = self.chi();
        let one = T::one();
        let three = T::lit(3.0);
        let z1 = one + (one - chi * chi).cbrt() * ((one + chi).cbrt() + (one - chi).cbrt());
        let z2 = (three * chi * chi + z1 * z1).sqrt();
        let root = ((three - z1) * (three + z1 + T::lit(2.0) * z2)).sqrt();
        let sign = if prograde { -one } else { one };
        self.mass * (three + z2 + sign * root)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_relative_eq;
    use proptest::prelude::*;

    // Closed-form radii evaluated in f64: a handful of transcendental calls, so ≲ 1e-14 relative.
    // Root-condition residuals involve r^{3/2} ~ 10 and cancellation, so use 1e-12 absolute.
    const RADIUS_TOL: f64 = 1e-13;
    const RESIDUAL_TOL: f64 = 1e-12;

    #[test]
    fn parameter_validation() {
        assert!(Kerr::new(0.999_f64).is_ok());
        assert!(Kerr::new(-0.5_f64).is_ok());
        assert!(Kerr::new(1.0_f64).is_err());
        assert!(Kerr::new(f64::NAN).is_err());
        assert!(Kerr::with_mass(0.0_f64, 0.0).is_err());
        assert!(Kerr::with_mass(2.0_f64, 1.5).is_ok());
    }

    #[test]
    fn schwarzschild_radii() {
        // Appendix A: horizon 2M, photon sphere 3M, ISCO 6M; ergosurface coincides with the horizon.
        let k = Kerr::<f64>::schwarzschild();
        assert_relative_eq!(k.outer_horizon(), 2.0, max_relative = RADIUS_TOL);
        assert_relative_eq!(k.inner_horizon(), 0.0, epsilon = RADIUS_TOL);
        assert_relative_eq!(k.ergosurface(0.4), 2.0, max_relative = RADIUS_TOL);
        for pro in [true, false] {
            assert_relative_eq!(k.photon_orbit(pro), 3.0, max_relative = RADIUS_TOL);
            assert_relative_eq!(k.isco(pro), 6.0, max_relative = RADIUS_TOL);
        }
    }

    #[test]
    fn near_extremal_limits() {
        // χ → 1 (BPT 1972): r_ph,pro → M, r_ph,retro → 4M, r_isco,pro → M, r_isco,retro → 9M.
        // For χ = 1 − 1e-5 the ISCOs deviate by O((1 − χ)^{1/3}) ≈ 0.03 (through Z₁), r_ph,pro by
        // O((1 − χ)^{1/2}) ≈ 5e-3, and r_ph,retro by O(1 − χ).
        let k = Kerr::new(1.0_f64 - 1e-5).unwrap();
        assert_relative_eq!(k.photon_orbit(true), 1.0, epsilon = 0.03);
        assert_relative_eq!(k.isco(true), 1.0, epsilon = 0.05);
        assert_relative_eq!(k.photon_orbit(false), 4.0, epsilon = 1e-4);
        assert_relative_eq!(k.isco(false), 9.0, epsilon = 0.05);
        assert_relative_eq!(
            k.ergosurface(std::f64::consts::FRAC_PI_2),
            2.0,
            max_relative = RADIUS_TOL
        );
    }

    #[test]
    fn published_values_for_moderate_spin() {
        // a = 0.5, commonly tabulated values to 4 s.f.: r_isco = 4.233 (pro) / 7.555 (retro),
        // r_ph = 2.347 (pro) / 3.532 (retro); tolerance = half a unit in the last digit (+ margin).
        let k = Kerr::new(0.5_f64).unwrap();
        assert_relative_eq!(
            k.outer_horizon(),
            1.0 + 0.75_f64.sqrt(),
            max_relative = RADIUS_TOL
        );
        assert_relative_eq!(k.isco(true), 4.233, epsilon = 5e-4);
        assert_relative_eq!(k.isco(false), 7.555, epsilon = 5e-4);
        assert_relative_eq!(k.photon_orbit(true), 2.347, epsilon = 5e-4);
        assert_relative_eq!(k.photon_orbit(false), 3.532, epsilon = 5e-4);
    }

    #[test]
    fn mass_scaling() {
        // All radii scale linearly with M at fixed χ.
        let k1 = Kerr::new(0.6_f64).unwrap();
        let k3 = Kerr::with_mass(3.0_f64, 1.8).unwrap();
        assert_relative_eq!(k3.isco(true), 3.0 * k1.isco(true), max_relative = RADIUS_TOL);
        assert_relative_eq!(
            k3.photon_orbit(false),
            3.0 * k1.photon_orbit(false),
            max_relative = RADIUS_TOL
        );
        assert_relative_eq!(
            k3.outer_horizon(),
            3.0 * k1.outer_horizon(),
            max_relative = RADIUS_TOL
        );
    }

    proptest! {
        #[test]
        fn horizons_are_roots_of_delta(a in 0.0..0.999_f64) {
            // Δ = r² − 2Mr + a² vanishes at r_±.
            let k = Kerr::new(a).unwrap();
            for r in [k.outer_horizon(), k.inner_horizon()] {
                prop_assert!((r * r - 2.0 * r + a * a).abs() < RESIDUAL_TOL);
            }
            prop_assert!(k.inner_horizon() <= k.outer_horizon());
        }

        #[test]
        fn ergosurface_is_where_g_tt_vanishes(a in 0.05..0.999_f64, theta in 0.0..std::f64::consts::PI) {
            // BL: g_tt = −(1 − 2Mr/Σ) = 0 at r_E(θ); r_+ ≤ r_E ≤ 2M.
            let k = Kerr::new(a).unwrap();
            let r = k.ergosurface(theta);
            let sigma = r * r + a * a * theta.cos().powi(2);
            prop_assert!((1.0 - 2.0 * r / sigma).abs() < RESIDUAL_TOL);
            prop_assert!(r >= k.outer_horizon() - RADIUS_TOL && r <= 2.0 + RADIUS_TOL);
        }

        #[test]
        fn photon_orbits_satisfy_circular_orbit_condition(a in 0.0..0.999_f64) {
            // Circular equatorial orbits have E ∝ 1/sqrt(r^{3/2} − 3M r^{1/2} ± 2a M^{1/2}); the photon orbit is
            // the zero of the denominator (BPT 1972, eq. 2.12), upper sign prograde.
            let k = Kerr::new(a).unwrap();
            for (pro, s) in [(true, 1.0), (false, -1.0)] {
                let r = k.photon_orbit(pro);
                prop_assert!((r.powf(1.5) - 3.0 * r.sqrt() + s * 2.0 * a).abs() < RESIDUAL_TOL);
            }
        }

        #[test]
        fn isco_satisfies_marginal_stability(a in 0.0..0.999_f64) {
            // Marginal stability: r² − 6Mr ± 8a sqrt(Mr) − 3a² = 0 (BPT 1972, eq. 2.20), upper sign prograde.
            let k = Kerr::new(a).unwrap();
            for (pro, s) in [(true, 1.0), (false, -1.0)] {
                let r = k.isco(pro);
                let residual = r * r - 6.0 * r + s * 8.0 * a * r.sqrt() - 3.0 * a * a;
                prop_assert!(residual.abs() < 1e-10, "residual {residual} at r = {r}");
            }
            prop_assert!(k.isco(true) <= k.isco(false));
            prop_assert!(k.photon_orbit(true) <= k.photon_orbit(false));
            prop_assert!(k.outer_horizon() <= k.photon_orbit(true));
        }
    }
}
