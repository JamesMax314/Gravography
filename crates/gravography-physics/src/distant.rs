//! Distant-observer initial conditions from Bardeen image coordinates `(α, β)` (Bardeen 1973; `docs/PRD.md`
//! Appendix A; `docs/physics.md` § "Distant observer").
//!
//! For an observer at Boyer–Lindquist `(r_obs, θ_obs = i, φ_obs)`, a photon with image coordinates `(α, β)` has
//! (with `E = 1`) `ξ = L_z = −α sin i` and `η = Q = β² + cos²i (α² − a²)`. Its BL momentum at the observer is
//! `p_t = −1`, `p_φ = ξ`, `p_θ = β` (since `Θ(i) = η + a² cos²i − ξ² cot²i = β²`) and `p_r = +√R / Δ` (arriving,
//! i.e. outgoing), with `R(r) = (r² + a² − a ξ)² − Δ (η + (ξ − a)²)`.
//! Sign conventions: `β > 0` is the image half toward `+z` (the photon moves toward larger `θ`), `α > 0` the half
//! where retrograde photons (`ξ < 0`) appear.
//!
//! The photon's constants of motion are exactly the asymptotic ones, so there is no finite-distance error in
//! *which* geodesic is traced. `r_obs` only sets where the trace starts.

use crate::linalg::Vec4;
use crate::{Kerr, PhysicsError, Real};

impl<T: Real> Kerr<T> {
    /// KS event and **traced** (past-directed) covariant momentum for the photon seen at image coordinates
    /// `(alpha, beta)` by a distant observer at BL `(r_obs, inclination, phi_obs)`. The traced momentum is minus
    /// the physical one, so `p_t = +1` (consistent with [`crate::ray::camera_ray_momentum`], giving `g = 1`).
    ///
    /// # Errors
    /// [`PhysicsError::CoordinateSingularity`] on the axis (`sin i = 0`); [`PhysicsError::NoSuchRay`] if the
    /// radial potential is negative at `r_obs` (observer too close for these image coordinates).
    pub fn bardeen_ray(
        &self,
        r_obs: T,
        inclination: T,
        phi_obs: T,
        alpha: T,
        beta: T,
    ) -> Result<(Vec4<T>, Vec4<T>), PhysicsError> {
        let a = self.spin();
        let (si, ci) = inclination.sin_cos();
        let xi = -alpha * si;
        let eta = beta * beta + ci * ci * (alpha * alpha - a * a);
        let delta = self.delta(r_obs);
        let w = r_obs * r_obs + a * a - a * xi;
        let radial = w * w - delta * (eta + (xi - a) * (xi - a));
        if radial < T::zero() || delta <= T::zero() {
            return Err(PhysicsError::NoSuchRay(
                "radial potential R(r_obs) < 0 or observer inside the horizon",
            ));
        }
        let p_r = radial.sqrt() / delta;
        let bl = [T::zero(), r_obs, inclination, phi_obs];
        let p_traced_bl = [T::one(), -p_r, -beta, -xi];
        let p = self.bl_to_ks_covector(&bl, &p_traced_bl)?;
        Ok((self.bl_to_ks_position(&bl), p))
    }
}

#[cfg(test)]
mod tests {
    use crate::Kerr;
    use approx::assert_relative_eq;
    use std::f64::consts::FRAC_PI_2;

    #[test]
    fn bardeen_ray_has_the_requested_constants() {
        // (α, β) ↦ ξ = −α sin i, η = β² + cos² i (α² − a²); the traced momentum is null with p_t = +1.
        let k = Kerr::new(0.7_f64).unwrap();
        let (i, alpha, beta) = (1.1_f64, 3.0, -2.5);
        let (x, p) = k.bardeen_ray(1e4, i, 0.4, alpha, beta).unwrap();
        assert_relative_eq!(p[0], 1.0, max_relative = 1e-13);
        // The traced momentum flips E and L_z together, so ξ = L_z/E is unchanged; Q is quadratic.
        assert_relative_eq!(
            k.angular_momentum_z(&x, &p) / k.energy(&p),
            -alpha * i.sin(),
            max_relative = 1e-10
        );
        let eta = beta * beta + i.cos().powi(2) * (alpha * alpha - 0.49);
        assert_relative_eq!(k.carter_q(&x, &p), eta, max_relative = 1e-8);
        // H/|p|² ~ ε: p_r ≈ 1 at large r.
        assert!(k.hamiltonian(&x, &p).abs() < 1e-11);
    }

    #[test]
    fn traced_ray_heads_inward() {
        let k = Kerr::new(0.3_f64).unwrap();
        let (x, p) = k.bardeen_ray(1e3, FRAC_PI_2, 0.0, 4.0, 0.0).unwrap();
        let (dx, _) = k.hamiltonian_rhs(&x, &p);
        let radial = (x[1] * dx[1] + x[2] * dx[2] + x[3] * dx[3]) / k.r(&x);
        assert!(radial < 0.0);
        // Past-directed.
        assert!(dx[0] < 0.0);
    }

    #[test]
    fn errors_on_axis_and_too_close() {
        let k = Kerr::new(0.3_f64).unwrap();
        assert!(k.bardeen_ray(1e3, 0.0, 0.0, 1.0, 0.0).is_err());
        // Impact parameter far larger than r_obs: R(r_obs) < 0.
        assert!(k.bardeen_ray(10.0, FRAC_PI_2, 0.0, 50.0, 0.0).is_err());
    }
}
