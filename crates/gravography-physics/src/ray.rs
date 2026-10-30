//! Single-ray tracing on the CPU (`docs/PRD.md` §5.4–5.7; `docs/physics.md` § "Ray tracing").
//!
//! Rays are traced **backward in time**. The traced covector is `p = g(k)` with the past-directed null vector
//! `k = −e_(0) + nⁱ e_(i)`, so `p_t = E_∞ > 0` for rays that reach infinity, and the sky frequency shift is
//! `g = 1/p_t` ([`sky_frequency_shift`]). Phase-space state `[t, x, y, z, p_t, p_x, p_y, p_z]` is integrated with
//! Hamilton's equations ([`crate::geodesic`]) by DOPRI5 (adaptive) or RK4 (heuristic steps, no error control).
//!
//! Termination ([`RayStatus`]), checked after every accepted step, in this order:
//! 1. non-finite state → `Error`;
//! 2. `r < r_singular` → `Singular`;
//! 3. `r > r_escape` and `r` increasing → `Escaped`, `r_escape = max(escape_factor · r_camera, escape_min_radius)`;
//! 4. `r < r_+ + horizon_epsilon` and `r` decreasing → `Captured` (approach to the past horizon);
//! 5. `max |p_i| > momentum_blowup · max |p(0)|` → `Captured` (momentum blow-up near the past horizon);
//! 6. step budget exhausted → `MaxSteps`.

use crate::geodesic::ks_hamiltonian_rhs;
use crate::linalg::{Mat4, Vec4, lower};
use crate::ode::{StepController, dopri5_step, error_norm, rk4_step};
use crate::tetrad::Tetrad;
use crate::{Kerr, Real};

/// Phase-space state `[t, x, y, z, p_t, p_x, p_y, p_z]`.
pub type PhaseState<T> = [T; 8];

/// Packs an event and a covariant momentum into a [`PhaseState`].
pub fn pack<T: Real>(x: &Vec4<T>, p: &Vec4<T>) -> PhaseState<T> {
    [x[0], x[1], x[2], x[3], p[0], p[1], p[2], p[3]]
}

/// Splits a [`PhaseState`] into the event and the covariant momentum.
pub fn unpack<T: Real>(y: &PhaseState<T>) -> (Vec4<T>, Vec4<T>) {
    ([y[0], y[1], y[2], y[3]], [y[4], y[5], y[6], y[7]])
}

/// Covariant momentum `p_μ = g_{μν} k^ν` of the past-directed null vector `k = −e_(0) + nⁱ e_(i)` for a viewing
/// direction `n = (n_forward, n_left, n_up)` in the camera frame. `n` must be a unit vector; it points from the
/// camera toward the sky (`docs/PRD.md` §5.4).
pub fn camera_ray_momentum<T: Real>(g_lo: &Mat4<T>, tetrad: &Tetrad<T>, n: &[T; 3]) -> Vec4<T> {
    let k = tetrad.to_coordinate(&[-T::one(), n[0], n[1], n[2]]);
    lower(g_lo, &k)
}

/// Frequency shift `g = ν_obs/ν_emit` for a source at rest at infinity, `g = 1/p_t`, for a momentum built by
/// [`camera_ray_momentum`] (normalisation `−P·u = 1` of the physical photon `P = −k`; `docs/PRD.md` §5.7).
/// Only meaningful for rays that escape (`p_t > 0`).
pub fn sky_frequency_shift<T: Real>(p_lo: &Vec4<T>) -> T {
    T::one() / p_lo[0]
}

/// Integrator choice.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Integrator {
    /// Adaptive Dormand–Prince 5(4) with the tolerances of [`TraceParams`] (default).
    Dopri5,
    /// Classical RK4 with the heuristic step caps of [`TraceParams`] only (no error control).
    Rk4,
}

/// Ray classification (`docs/PRD.md` §5.6).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RayStatus {
    /// Reached `r_escape` moving outward: sees the sky.
    Escaped,
    /// Fell toward the past horizon (or momentum blew up there): rendered black.
    Captured,
    /// Came within `r_singular` of `r = 0` (should not happen for allowed cameras).
    Singular,
    /// Step budget exhausted (rendered as captured; tracked as a quality metric).
    MaxSteps,
    /// NaN/Inf encountered.
    Error,
}

/// Tracing parameters. Lengths in units of `M`; step sizes in affine parameter `λ`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TraceParams<T> {
    /// Integrator.
    pub integrator: Integrator,
    /// DOPRI5 relative tolerance.
    pub rtol: T,
    /// DOPRI5 absolute tolerance.
    pub atol: T,
    /// Step cap near the outer horizon (exterior rays): the coordinate displacement per step is at most
    /// `horizon_step_factor · (r − r_+)`.
    pub horizon_step_factor: T,
    /// Step cap everywhere: the coordinate displacement per step is at most `far_step_factor · r`.
    pub far_step_factor: T,
    /// Smallest step: an adaptive step is accepted anyway once `h ≤ h_min` (avoids stalling).
    pub h_min: T,
    /// Largest step in `λ`.
    pub h_max: T,
    /// Maximum number of steps (accepted + rejected).
    pub max_steps: u32,
    /// `r_escape = max(escape_factor · r_camera, escape_min_radius)`.
    pub escape_factor: T,
    /// See `escape_factor`.
    pub escape_min_radius: T,
    /// Capture when `r < r_+ + horizon_epsilon` and moving inward.
    pub horizon_epsilon: T,
    /// Capture when `max |p_i|` exceeds this multiple of the initial `max |p_μ|`.
    pub momentum_blowup: T,
    /// `Singular` when `r` drops below this.
    pub r_singular: T,
}

impl<T: Real> TraceParams<T> {
    /// CPU reference defaults (`docs/PRD.md` §5.5: `rtol = 1e-10`). `r_escape = max(16 r_camera, 400)` keeps the
    /// finite-radius escape-direction error ≲ 1e-6 rad (`docs/physics.md` § "Escape direction").
    pub fn cpu_reference() -> Self {
        Self {
            integrator: Integrator::Dopri5,
            rtol: T::lit(1e-10),
            atol: T::lit(1e-12),
            horizon_step_factor: T::lit(0.1),
            far_step_factor: T::lit(0.1),
            h_min: T::lit(1e-14),
            h_max: T::lit(1e6),
            max_steps: 100_000,
            escape_factor: T::lit(16.0),
            escape_min_radius: T::lit(400.0),
            horizon_epsilon: T::lit(1e-3),
            momentum_blowup: T::lit(1e8),
            r_singular: T::lit(1e-3),
        }
    }
}

/// Outcome of tracing one ray.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RayResult<T> {
    /// Classification.
    pub status: RayStatus,
    /// Final event `[t, x, y, z]`.
    pub x: Vec4<T>,
    /// Final covariant momentum.
    pub p: Vec4<T>,
    /// Unit escape direction on the celestial sphere (KS Cartesian axes), for `Escaped` rays.
    pub escape_dir: Option<[T; 3]>,
    /// Accepted steps.
    pub steps: u32,
    /// Rejected (DOPRI5) steps.
    pub rejected: u32,
    /// Affine parameter at termination.
    pub lambda: T,
}

/// Euclidean norm of the spatial part.
fn spatial_norm<T: Real>(v: &Vec4<T>) -> T {
    (v[1] * v[1] + v[2] * v[2] + v[3] * v[3]).sqrt()
}

fn max_abs<T: Real>(v: &Vec4<T>) -> T {
    v.iter().fold(T::zero(), |m, c| m.max(c.abs()))
}

/// `dr/dλ = ∂_i r ẋⁱ` with `∂_i r = r (r² x, r² y, (r² + a²) z)_i / (r⁴ + a² z²)` (`docs/physics.md`
/// § "Derivatives of the Kerr–Schild fields").
fn radial_rate<T: Real>(a: T, r: T, x: &Vec4<T>, dx: &Vec4<T>) -> T {
    let r2 = r * r;
    let d = r2 * r2 + a * a * x[3] * x[3];
    r * (r2 * (x[1] * dx[1] + x[2] * dx[2]) + (r2 + a * a) * x[3] * dx[3]) / d
}

/// Heuristic step cap (`docs/PRD.md` §5.5), converted to `λ` with the current coordinate rates:
/// coordinate displacement `≤ far_step_factor · r` everywhere, and, for exterior rays moving inward (approaching
/// the past horizon), radial displacement `|Δr| ≤ horizon_step_factor · (r − r_+)`. The second bound limits only
/// radial motion, so near-critical rays orbiting just outside the horizon (`a → 1`) are not slowed down.
fn step_cap<T: Real>(params: &TraceParams<T>, a: T, r: T, r_plus: T, x: &Vec4<T>, dx: &Vec4<T>) -> T {
    let tiny = T::lit(1e-300);
    let speed = spatial_norm(dx).max(tiny);
    let mut h = params.far_step_factor * r / speed;
    let dr = radial_rate(a, r, x, dx);
    if r > r_plus && dr < T::zero() {
        h = h.min(params.horizon_step_factor * (r - r_plus) / (-dr).max(tiny));
    }
    h.min(params.h_max)
}

/// Escape direction: the unit spatial part of the coordinate velocity `dx/dλ` (the spatial part of `k`).
///
/// No weak-field correction is applied: in ingoing Kerr–Schild coordinates the outgoing traced ray (an ingoing
/// photon in forward time) is nearly parallel to the straight ingoing null congruence `l`. The coordinate
/// direction therefore converges like `~M b³/R⁴` rather than `M b/R²` (`docs/physics.md` § "Escape direction").
pub fn escape_direction<T: Real>(dx: &Vec4<T>) -> [T; 3] {
    let inv = T::one() / spatial_norm(dx);
    [dx[1] * inv, dx[2] * inv, dx[3] * inv]
}

/// Traces a ray from event `x0` with traced covariant momentum `p0` (see [`camera_ray_momentum`]).
pub fn trace_ray<T: Real>(
    kerr: &Kerr<T>,
    x0: &Vec4<T>,
    p0: &Vec4<T>,
    params: &TraceParams<T>,
) -> RayResult<T> {
    trace_ray_observed(kerr, x0, p0, params, |_, _, _| {})
}

/// [`trace_ray`] that calls `observe(λ, x, p)` at the start and after every accepted step.
pub fn trace_ray_observed<T: Real>(
    kerr: &Kerr<T>,
    x0: &Vec4<T>,
    p0: &Vec4<T>,
    params: &TraceParams<T>,
    mut observe: impl FnMut(T, &Vec4<T>, &Vec4<T>),
) -> RayResult<T> {
    let (mass, a) = (kerr.mass(), kerr.spin());
    let rhs = |y: &PhaseState<T>| {
        let (x, p) = unpack(y);
        let (dx, dp) = ks_hamiltonian_rhs(mass, a, &x, &p);
        pack(&dx, &dp)
    };
    let r_plus = kerr.outer_horizon();
    let r_escape = (params.escape_factor * kerr.r(x0)).max(params.escape_min_radius);
    let p_limit = params.momentum_blowup * max_abs(p0);
    let ctl = StepController::<T>::dopri5();

    let mut y = pack(x0, p0);
    let mut k1 = rhs(&y);
    let mut r_prev = kerr.r(x0);
    let mut lambda = T::zero();
    let (mut steps, mut rejected) = (0_u32, 0_u32);
    let mut h = T::lit(0.1) * step_cap(params, a, r_prev, r_plus, x0, &unpack(&k1).0);
    observe(lambda, x0, p0);

    let finish = |status, y: &PhaseState<T>, dx: &Vec4<T>, steps, rejected, lambda| {
        let (x, p) = unpack(y);
        let escape_dir = (status == RayStatus::Escaped).then(|| escape_direction(dx));
        RayResult {
            status,
            x,
            p,
            escape_dir,
            steps,
            rejected,
            lambda,
        }
    };

    loop {
        if steps + rejected >= params.max_steps {
            return finish(RayStatus::MaxSteps, &y, &unpack(&k1).0, steps, rejected, lambda);
        }
        let dx = unpack(&k1).0;
        let h_try = h.min(step_cap(params, a, r_prev, r_plus, &unpack(&y).0, &dx));

        let (y_new, k_new) = match params.integrator {
            Integrator::Dopri5 => {
                let s = dopri5_step(rhs, &y, &k1, h_try);
                let err = error_norm(&y, &s.y, &s.err, params.rtol, params.atol);
                if !err.is_finite() {
                    return finish(RayStatus::Error, &s.y, &dx, steps, rejected, lambda);
                }
                let accept = err <= T::one() || h_try <= params.h_min;
                h = ctl.next_h(h_try, err).max(params.h_min);
                if !accept {
                    rejected += 1;
                    continue;
                }
                (s.y, s.k_last)
            }
            Integrator::Rk4 => {
                let y_new = rk4_step(rhs, &y, h_try);
                // No error control: the heuristic cap alone sets the next step.
                h = params.h_max;
                (y_new, rhs(&y_new))
            }
        };
        y = y_new;
        k1 = k_new;
        lambda = lambda + h_try;
        steps += 1;
        let (x, p) = unpack(&y);
        observe(lambda, &x, &p);

        if y.iter().any(|v| !v.is_finite()) || k1.iter().any(|v| !v.is_finite()) {
            return finish(RayStatus::Error, &y, &unpack(&k1).0, steps, rejected, lambda);
        }
        let r = kerr.r(&x);
        let dx = unpack(&k1).0;
        let status = if r < params.r_singular {
            Some(RayStatus::Singular)
        } else if r > r_escape && r > r_prev {
            Some(RayStatus::Escaped)
        } else if (r < r_plus + params.horizon_epsilon && r < r_prev) || max_abs(&p) > p_limit {
            Some(RayStatus::Captured)
        } else {
            None
        };
        if let Some(status) = status {
            return finish(status, &y, &dx, steps, rejected, lambda);
        }
        r_prev = r;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::linalg::eta;
    use approx::assert_relative_eq;

    fn identity_tetrad() -> Tetrad<f64> {
        Tetrad {
            e_up: [
                [1.0, 0.0, 0.0, 0.0],
                [0.0, 1.0, 0.0, 0.0],
                [0.0, 0.0, 1.0, 0.0],
                [0.0, 0.0, 0.0, 1.0],
            ],
        }
    }

    #[test]
    fn camera_momentum_is_null_and_normalised() {
        // Static observer in Schwarzschild at r = 10 on the x axis: u = ∂_t / sqrt(1 − 2/r).
        let k = Kerr::<f64>::schwarzschild();
        let pos = [0.0, 10.0, 0.0, 0.0];
        let g = k.g_lo(&pos);
        let u = [1.0, 0.0, 0.0, 0.0];
        let t = Tetrad::gram_schmidt(
            &g,
            &[
                u,
                [0.0, -1.0, 0.0, 0.0],
                [0.0, 0.0, -1.0, 0.0],
                [0.0, 0.0, 0.0, 1.0],
            ],
        )
        .unwrap();
        let n = [0.6, 0.0, 0.8];
        let p = camera_ray_momentum(&g, &t, &n);
        // Null: H = 0.
        assert!(k.hamiltonian(&pos, &p).abs() < 1e-15);
        // Physical photon P = −k has −P·u = 1, i.e. p·u = 1 for the traced covector.
        let pu: f64 = (0..4).map(|m| p[m] * t.u()[m]).sum();
        assert_relative_eq!(pu, 1.0, max_relative = 1e-14);
        // Static observer at r: uniform blueshift g = (1 − 2/r)^{−1/2} (PRD §9 test 10, here at the camera).
        assert_relative_eq!(
            sky_frequency_shift(&p),
            1.0 / (1.0 - 0.2_f64).sqrt(),
            max_relative = 1e-14
        );
    }

    #[test]
    fn flat_space_ray_escapes_in_a_straight_line() {
        // M → 0 is not allowed by Kerr; use a tiny mass so the bending is negligible.
        let k = Kerr::with_mass(1e-12_f64, 0.0).unwrap();
        let pos = [0.0, 1.0, 2.0, 3.0];
        let n = [0.0, 0.6, 0.8];
        let p = camera_ray_momentum(&eta(), &identity_tetrad(), &n);
        let res = trace_ray(&k, &pos, &p, &TraceParams::cpu_reference());
        assert_eq!(res.status, RayStatus::Escaped);
        let d = res.escape_dir.unwrap();
        for i in 0..3 {
            assert_relative_eq!(d[i], [0.0, 0.6, 0.8][i], epsilon = 1e-12);
        }
        // p is conserved exactly in flat space.
        assert_relative_eq!(res.p[2], 0.6, epsilon = 1e-15);
    }

    #[test]
    fn radial_ingoing_ray_is_captured_and_outgoing_escapes() {
        let k = Kerr::<f64>::schwarzschild();
        let pos = [0.0, 10.0, 0.0, 0.0];
        let g = k.g_lo(&pos);
        let t = Tetrad::gram_schmidt(
            &g,
            &[
                [1.0, 0.0, 0.0, 0.0],
                [0.0, -1.0, 0.0, 0.0],
                [0.0, 0.0, -1.0, 0.0],
                [0.0, 0.0, 0.0, 1.0],
            ],
        )
        .unwrap();
        let params = TraceParams::cpu_reference();
        let inward = trace_ray(&k, &pos, &camera_ray_momentum(&g, &t, &[1.0, 0.0, 0.0]), &params);
        assert_eq!(inward.status, RayStatus::Captured);
        assert!(k.r(&inward.x) < 2.0 + 1e-3);
        let outward = trace_ray(&k, &pos, &camera_ray_momentum(&g, &t, &[-1.0, 0.0, 0.0]), &params);
        assert_eq!(outward.status, RayStatus::Escaped);
        let d = outward.escape_dir.unwrap();
        assert_relative_eq!(d[0], 1.0, epsilon = 1e-12);
    }

    #[test]
    fn camera_inside_horizon_sees_rays_leave_through_the_future_horizon() {
        // Schwarzschild, camera at r = 1 (inside). Past-directed rays move outward through the future horizon
        // (covered regularly by ingoing KS, docs/PRD.md §5.4); outward-pointing ones then escape.
        let k = Kerr::<f64>::schwarzschild();
        let pos = [0.0, 1.0, 0.0, 0.0];
        let g = k.g_lo(&pos);
        // Inside the horizon ∂_t is spacelike; use the infalling observer u_μ = −∂_μ t + ... approximated by
        // the KS-normal observer u ∝ g^{μν}(−1, 0, 0, 0)·(−1) (∇t is timelike in KS everywhere).
        let g_up = k.g_up(&pos);
        let u = [-g_up[0][0], -g_up[1][0], -g_up[2][0], -g_up[3][0]];
        let t = Tetrad::gram_schmidt(
            &g,
            &[
                u,
                [0.0, 1.0, 0.0, 0.0],
                [0.0, 0.0, 1.0, 0.0],
                [0.0, 0.0, 0.0, 1.0],
            ],
        )
        .unwrap();
        let res = trace_ray(
            &k,
            &pos,
            &camera_ray_momentum(&g, &t, &[1.0, 0.0, 0.0]),
            &TraceParams::cpu_reference(),
        );
        assert_eq!(res.status, RayStatus::Escaped);
    }

    #[test]
    fn rk4_agrees_with_dopri5_for_a_bent_ray() {
        // A moderately bent Schwarzschild ray; RK4 with small heuristic steps vs DOPRI5 at rtol 1e-10.
        let k = Kerr::<f64>::schwarzschild();
        let pos = [0.0, 30.0, 0.0, 0.0];
        let g = k.g_lo(&pos);
        let t = Tetrad::gram_schmidt(
            &g,
            &[
                [1.0, 0.0, 0.0, 0.0],
                [0.0, -1.0, 0.0, 0.0],
                [0.0, 0.0, -1.0, 0.0],
                [0.0, 0.0, 0.0, 1.0],
            ],
        )
        .unwrap();
        let ang = 0.3_f64;
        let p = camera_ray_momentum(&g, &t, &[ang.cos(), ang.sin(), 0.0]);
        // Escape far away so that the two runs' different stopping radii do not matter (direction error there
        // ~M b³/R⁴ ≈ 1e-20).
        let mut base = TraceParams::cpu_reference();
        base.escape_min_radius = 1e7;
        let reference = trace_ray(&k, &pos, &p, &base).escape_dir.unwrap();
        let mut errors = Vec::new();
        for f in [0.02, 0.01] {
            let mut rk4 = base;
            rk4.integrator = Integrator::Rk4;
            rk4.far_step_factor = f;
            rk4.horizon_step_factor = f;
            let res = trace_ray(&k, &pos, &p, &rk4);
            assert_eq!(res.status, RayStatus::Escaped);
            let d = res.escape_dir.unwrap();
            errors.push((0..3).map(|i| (d[i] - reference[i]).powi(2)).sum::<f64>().sqrt());
        }
        // Fourth order: halving the steps divides the error by ≈ 16 (measured 16.6; DOPRI5 reference error at
        // rtol 1e-10 is ~1e-12, far below these). Allow 12–22.
        let ratio = errors[0] / errors[1];
        assert!(errors[1] < 1e-9, "RK4 error {errors:?}");
        assert!((12.0..22.0).contains(&ratio), "convergence ratio {ratio}");
    }

    #[test]
    fn max_steps_is_reported() {
        let k = Kerr::<f64>::schwarzschild();
        let mut params = TraceParams::cpu_reference();
        params.max_steps = 5;
        let p = camera_ray_momentum(&eta(), &identity_tetrad(), &[1.0, 0.0, 0.0]);
        let res = trace_ray(&k, &[0.0, 0.0, 0.0, 50.0], &p, &params);
        assert_eq!(res.status, RayStatus::MaxSteps);
        assert_eq!(res.steps + res.rejected, 5);
    }

    #[test]
    fn pack_unpack_round_trip() {
        let (x, p) = ([1.0, 2.0, 3.0, 4.0], [5.0, 6.0, 7.0, 8.0]);
        assert_eq!(unpack(&pack(&x, &p)), (x, p));
    }
}
