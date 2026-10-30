//! Observer (camera) dynamics: worldline with optional thrust, Fermi–Walker transported tetrad, presets,
//! regions and limits (`docs/PRD.md` §5.8; derivations in `docs/physics.md` § "Observer dynamics").
//!
//! The state is the KS event `x^μ`, the orthonormal tetrad `e_(a)^μ` (with `e_(0) = u`) and the proper time `τ`.
//! It is integrated in proper time with DOPRI5 on the 20-component state `(x, e_(0), e_(1), e_(2), e_(3))`:
//!
//! - `dx^μ/dτ = u^μ`,
//! - `de_(a)^μ/dτ = −Γ^μ_{αβ} u^α e_(a)^β + u^μ g(A, e_(a)) − A^μ g(u, e_(a))` (Fermi–Walker transport),
//!
//! where `A` is the 4-acceleration (`A · u = 0`). For `a = 0` this gives `Du/dτ = A`, the equation of motion with
//! thrust. After every accepted substep the tetrad is re-orthonormalised by Gram–Schmidt, keeping `e_(0) ∥ u`.
//!
//! [`Motion`] selects how `A` is chosen:
//! - [`Motion::Free`]: `A = Aⁱ e_(i)` from the user's thrust, given in the transported (body) frame;
//! - [`Motion::Hover`] and [`Motion::Zamo`]: station keeping on the orbit of the Killing field `∂_t + Ω ∂_φ`
//!   (`Ω = 0` static, `Ω = ω` ZAMO); `A` is the acceleration of that worldline and the user's thrust is ignored.
//!
//! The user look rotation is not part of the state: apply [`Tetrad::rotated`] for rendering, and rotate thrust
//! input from the view frame into the body frame before calling [`Observer::advance`].

use crate::error::ObserverError;
use crate::linalg::{Mat4, Vec4, inner, raise};
use crate::ode::{StepController, dopri5_step, error_norm};
use crate::tetrad::Tetrad;
use crate::{Christoffel, Kerr, Real};

/// Region of spacetime the observer is in (`docs/PRD.md` FR-O4).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Region {
    /// `r > r_E(θ)`: static observers exist.
    OutsideErgosphere,
    /// `r_+ < r ≤ r_E(θ)`: everything is dragged around the hole; no static observers.
    Ergosphere,
    /// `r ≤ r_+`: inside the event horizon.
    InsideHorizon,
}

/// How the observer's 4-acceleration is chosen.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Motion<T> {
    /// Geodesic motion plus the user's thrust.
    Free,
    /// Static observer (`u ∝ ∂_t`) held at a fixed position by automatic thrust. Only possible outside the
    /// ergosphere.
    Hover {
        /// Cylindrical radius `sqrt(x² + y²)` of the held orbit.
        rho: T,
        /// Height `z` of the held orbit.
        z: T,
    },
    /// Zero-angular-momentum observer (`u ∝ ∂_t + ω ∂_φ`) held at fixed `(r, θ)` by automatic thrust. Only
    /// possible outside the horizon.
    Zamo {
        /// Cylindrical radius `sqrt(x² + y²)` of the held orbit.
        rho: T,
        /// Height `z` of the held orbit.
        z: T,
    },
}

/// Integration settings and limits for [`Observer::advance`]. Lengths and proper times in units of `M`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ObserverParams<T> {
    /// DOPRI5 relative tolerance.
    pub rtol: T,
    /// DOPRI5 absolute tolerance.
    pub atol: T,
    /// Step cap: coordinate displacement per substep `≤ step_factor · r`.
    pub step_factor: T,
    /// Smallest substep in `τ` (accepted regardless of the error estimate, to avoid stalling).
    pub h_min: T,
    /// Largest substep in `τ`.
    pub h_max: T,
    /// Maximum number of substeps (accepted + rejected) per call to [`Observer::advance`].
    pub max_substeps: u32,
    /// The simulation stops below `r_min = r_− + inner_margin` (`docs/PRD.md` §5.8: near the singularity for
    /// `a = 0`, before the inner horizon for Kerr).
    pub inner_margin: T,
    /// The simulation stops beyond this radius (keeps the GPU f32 path in range, `docs/PRD.md` §5.9).
    pub r_max: T,
}

impl<T: Real> Default for ObserverParams<T> {
    /// CPU defaults: `rtol = 1e-10`, `atol = 1e-12`, `r_min = r_− + 0.05`, `r_max = 1000`.
    fn default() -> Self {
        Self {
            rtol: T::lit(1e-10),
            atol: T::lit(1e-12),
            step_factor: T::lit(0.1),
            h_min: T::lit(1e-12),
            h_max: T::lit(10.0),
            max_substeps: 100_000,
            inner_margin: T::lit(0.05),
            r_max: T::lit(1000.0),
        }
    }
}

impl<T: Real> ObserverParams<T> {
    /// Inner limit `r_min = r_− + inner_margin` for the given hole.
    pub fn r_min(&self, kerr: &Kerr<T>) -> T {
        kerr.inner_horizon() + self.inner_margin
    }
}

/// Diagnostics of one call to [`Observer::advance`].
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AdvanceReport<T> {
    /// Accepted substeps.
    pub substeps: u32,
    /// Rejected substeps.
    pub rejected: u32,
    /// Largest `|g(e_a, e_b) − η_ab|` found *before* re-orthonormalisation (integration drift).
    pub max_constraint_error: T,
}

/// An observer: event, Fermi–Walker transported tetrad, proper time and motion mode.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Observer<T> {
    /// KS event `[t, x, y, z]`.
    pub pos: Vec4<T>,
    /// Body tetrad (`e_(0) = u`; forward, left, up), Fermi–Walker transported.
    pub tetrad: Tetrad<T>,
    /// Proper time `τ` (units of `M`).
    pub tau: T,
    /// Motion mode.
    pub motion: Motion<T>,
    /// Step-size memory between calls to [`Observer::advance`].
    h_next: T,
}

/// Number of components of the integrated state `(x, e_(0), e_(1), e_(2), e_(3))`.
const STATE_LEN: usize = 20;
type State<T> = [T; STATE_LEN];

/// KS event at spheroidal coordinates `(r, θ, φ̃)` with `t = 0`: `x + i y = (r + i a) sinθ e^{iφ̃}`, `z = r cosθ`
/// (`docs/physics.md` § "Kerr–Schild metric"). `φ̃` is the KS azimuth (equal to BL `φ` up to a function of `r`).
pub fn ks_position<T: Real>(a: T, r: T, theta: T, phi: T) -> Vec4<T> {
    let (st, ct) = theta.sin_cos();
    let (sp, cp) = phi.sin_cos();
    [T::zero(), st * (r * cp - a * sp), st * (r * sp + a * cp), r * ct]
}

/// Default viewing orientation at `pos`: forward toward the hole (`−x⃗/|x⃗|`), up toward `+z` (toward `+x` when
/// looking along the spin axis). Returns `(forward, up)` as Euclidean coordinate directions.
pub fn look_at_hole<T: Real>(pos: &Vec4<T>) -> ([T; 3], [T; 3]) {
    let n = (pos[1] * pos[1] + pos[2] * pos[2] + pos[3] * pos[3]).sqrt();
    let fwd = [-pos[1] / n, -pos[2] / n, -pos[3] / n];
    let (o, z) = (T::one(), T::zero());
    let up = if fwd[2].abs() > T::lit(0.999) {
        [o, z, z]
    } else {
        [z, z, o]
    };
    (fwd, up)
}

fn cross<T: Real>(a: &[T; 3], b: &[T; 3]) -> [T; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

/// Orthonormal tetrad for 4-velocity `u` with spatial legs seeded by the coordinate directions `forward` and
/// `up` (Euclidean `(x, y, z)` components; `up` need not be orthogonal to `forward`). The seeds
/// `(u, forward, up × forward, forward × (up × forward))` are positively oriented, and Gram–Schmidt keeps the
/// orientation, so the result is right-handed. Returns `None` if `u` is not timelike or `forward ∥ up`.
pub fn oriented_tetrad<T: Real>(
    g_lo: &Mat4<T>,
    u: &Vec4<T>,
    forward: &[T; 3],
    up: &[T; 3],
) -> Option<Tetrad<T>> {
    let left = cross(up, forward);
    let up_perp = cross(forward, &left);
    let z = T::zero();
    let seeds = [
        *u,
        [z, forward[0], forward[1], forward[2]],
        [z, left[0], left[1], left[2]],
        [z, up_perp[0], up_perp[1], up_perp[2]],
    ];
    if u[0] <= T::zero() {
        return None;
    }
    Tetrad::gram_schmidt(g_lo, &seeds)
}

/// Lorentz factor `γ = −g(u, v)` between two unit timelike vectors.
pub fn lorentz_factor<T: Real>(g_lo: &Mat4<T>, u: &Vec4<T>, v: &Vec4<T>) -> T {
    -inner(g_lo, u, v)
}

/// Relative speed `β = sqrt(1 − 1/γ²)` (units of `c`) for a Lorentz factor `γ ≥ 1`.
pub fn speed_from_lorentz_factor<T: Real>(gamma: T) -> T {
    (T::one() - T::one() / (gamma * gamma)).max(T::zero()).sqrt()
}

/// `GM_☉/c²` in metres (IAU 2015 nominal solar mass parameter `1.3271244e20 m³ s⁻²`).
pub const SOLAR_MASS_LENGTH_M: f64 = 1.327_124_4e20 / (SPEED_OF_LIGHT_M_S * SPEED_OF_LIGHT_M_S);
/// Speed of light in m/s.
pub const SPEED_OF_LIGHT_M_S: f64 = 299_792_458.0;
/// Standard gravity `g₀` in m/s².
pub const STANDARD_GRAVITY_M_S2: f64 = 9.806_65;

/// Converts a proper acceleration in units of `c²/M` (the code units) to multiples of `g₀` for a hole of
/// `mass_solar` solar masses: `a_SI = a c² / (G M / c²)` (`docs/PRD.md` §5.8).
pub fn acceleration_in_g(accel: f64, mass_solar: f64) -> f64 {
    accel * SPEED_OF_LIGHT_M_S * SPEED_OF_LIGHT_M_S
        / (mass_solar * SOLAR_MASS_LENGTH_M)
        / STANDARD_GRAVITY_M_S2
}

impl<T: Real> Kerr<T> {
    /// Region of the event `pos` (uses `θ = arccos(z/r)` for the ergosurface).
    pub fn region(&self, pos: &Vec4<T>) -> Region {
        let r = self.r(pos);
        if r <= self.outer_horizon() {
            return Region::InsideHorizon;
        }
        let theta = (pos[3] / r).max(-T::one()).min(T::one()).acos();
        if r <= self.ergosurface(theta) {
            Region::Ergosphere
        } else {
            Region::OutsideErgosphere
        }
    }

    /// Frame-dragging angular velocity `ω = 2Mar / ((r² + a²)² − Δ a² sin²θ)` at `pos` (BL formula, regular on
    /// the axis; `docs/physics.md` § "Boyer–Lindquist ↔ Kerr–Schild"). Positive for `a > 0`.
    pub fn zamo_angular_velocity(&self, pos: &Vec4<T>) -> T {
        let r = self.r(pos);
        let a = self.spin();
        let q = r * r + a * a;
        let cos2 = if r > T::zero() {
            pos[3] * pos[3] / (r * r)
        } else {
            T::zero()
        };
        let sin2 = T::one() - cos2;
        T::lit(2.0) * self.mass() * a * r / (q * q - self.delta(r) * a * a * sin2)
    }

    /// Angular velocity `dφ/dt = ±M^{1/2} / (r^{3/2} ± a M^{1/2})` of an equatorial circular geodesic
    /// (`docs/PRD.md` Appendix A; Bardeen, Press & Teukolsky 1972), upper sign prograde.
    pub fn keplerian_angular_velocity(&self, r: T, prograde: bool) -> T {
        let sm = self.mass().sqrt();
        if prograde {
            sm / (r * r.sqrt() + self.spin() * sm)
        } else {
            -sm / (r * r.sqrt() - self.spin() * sm)
        }
    }

    /// Unit 4-velocity `u = K / sqrt(−g(K, K))` along the Killing field `K = ∂_t + Ω ∂_φ = (1, −Ω y, Ω x, 0)` at
    /// `pos`. Returns `None` where `K` is not timelike (e.g. `Ω = 0` inside the ergosphere).
    pub fn stationary_velocity(&self, pos: &Vec4<T>, omega: T) -> Option<Vec4<T>> {
        let k = [T::one(), -omega * pos[2], omega * pos[1], T::zero()];
        let kk = inner(&self.g_lo(pos), &k, &k);
        if kk.is_nan() || kk >= T::zero() {
            return None;
        }
        let n = T::one() / (-kk).sqrt();
        Some(k.map(|c| c * n))
    }

    /// 4-acceleration `a^μ = du^μ/dτ + Γ^μ_{αβ} u^α u^β` of the stationary observer of
    /// [`Kerr::stationary_velocity`]. Along the orbit `N = (−K·K)^{−1/2}` is constant and
    /// `du^μ/dτ = N² K^ν ∂_ν K^μ = N² (0, −Ω² x, −Ω² y, 0)` (`docs/physics.md` § "Stationary observers").
    pub fn stationary_acceleration(&self, pos: &Vec4<T>, omega: T) -> Option<Vec4<T>> {
        let u = self.stationary_velocity(pos, omega)?;
        Some(stationary_acceleration_with(
            &self.christoffel(pos),
            pos,
            omega,
            &u,
        ))
    }
}

fn stationary_acceleration_with<T: Real>(
    gamma: &Christoffel<T>,
    pos: &Vec4<T>,
    omega: T,
    u: &Vec4<T>,
) -> Vec4<T> {
    // u = N K with K^t = 1, so N = u^t.
    let n2w2 = u[0] * u[0] * omega * omega;
    let du = [T::zero(), -n2w2 * pos[1], -n2w2 * pos[2], T::zero()];
    core::array::from_fn(|mu| {
        let mut s = du[mu];
        for al in 0..4 {
            for be in 0..4 {
                s = s + gamma[mu][al][be] * u[al] * u[be];
            }
        }
        s
    })
}

/// What drives the acceleration during one call to [`Observer::advance`].
#[derive(Clone, Copy)]
enum Drive<T> {
    /// Thrust in the body frame (forward, left, up), units of `c²/M`.
    Thrust([T; 3]),
    /// Station keeping with `Ω = 0` (`zamo = false`) or `Ω = ω(x)` (`zamo = true`).
    Station { zamo: bool },
}

fn pack<T: Real>(pos: &Vec4<T>, tetrad: &Tetrad<T>) -> State<T> {
    let mut y = [T::zero(); STATE_LEN];
    y[..4].copy_from_slice(pos);
    for a in 0..4 {
        y[4 + 4 * a..8 + 4 * a].copy_from_slice(&tetrad.e_up[a]);
    }
    y
}

fn unpack<T: Real>(y: &State<T>) -> (Vec4<T>, Tetrad<T>) {
    let v = |i: usize| [y[i], y[i + 1], y[i + 2], y[i + 3]];
    (
        v(0),
        Tetrad {
            e_up: [v(4), v(8), v(12), v(16)],
        },
    )
}

/// 4-acceleration for the given drive at event `x` with legs `e`.
fn drive_acceleration<T: Real>(
    kerr: &Kerr<T>,
    gamma: &Christoffel<T>,
    drive: Drive<T>,
    x: &Vec4<T>,
    e: &Tetrad<T>,
) -> Vec4<T> {
    match drive {
        Drive::Thrust(th) => e.to_coordinate(&[T::zero(), th[0], th[1], th[2]]),
        Drive::Station { zamo } => {
            let omega = if zamo {
                kerr.zamo_angular_velocity(x)
            } else {
                T::zero()
            };
            match kerr.stationary_velocity(x, omega) {
                Some(u) => stationary_acceleration_with(gamma, x, omega, &u),
                None => [T::nan(); 4],
            }
        }
    }
}

/// Right-hand side of the worldline + Fermi–Walker system (module docs; `docs/physics.md`
/// § "Fermi–Walker transport").
fn observer_rhs<T: Real>(kerr: &Kerr<T>, drive: Drive<T>, y: &State<T>) -> State<T> {
    let (x, e) = unpack(y);
    let gamma = kerr.christoffel(&x);
    let g = kerr.g_lo(&x);
    let u = e.e_up[0];
    let acc = drive_acceleration(kerr, &gamma, drive, &x, &e);
    let mut out = [T::zero(); STATE_LEN];
    out[..4].copy_from_slice(&u);
    for a in 0..4 {
        let ea = &e.e_up[a];
        let a_dot_e = inner(&g, &acc, ea);
        let u_dot_e = inner(&g, &u, ea);
        for mu in 0..4 {
            let mut s = T::zero();
            for al in 0..4 {
                for be in 0..4 {
                    s = s + gamma[mu][al][be] * u[al] * ea[be];
                }
            }
            out[4 + 4 * a + mu] = -s + u[mu] * a_dot_e - acc[mu] * u_dot_e;
        }
    }
    out
}

impl<T: Real> Observer<T> {
    /// Free observer at `pos` with 4-velocity `u` (normalised here; must be future-directed timelike) and
    /// orientation seeded by the coordinate directions `forward` and `up` (see [`oriented_tetrad`]).
    ///
    /// # Errors
    /// [`ObserverError::InvalidState`] if `u` is not future timelike or the orientation is degenerate;
    /// [`ObserverError::InnerLimit`] / [`ObserverError::OuterLimit`] if `pos` is outside the allowed range.
    pub fn new(
        kerr: &Kerr<T>,
        pos: &Vec4<T>,
        u: &Vec4<T>,
        forward: &[T; 3],
        up: &[T; 3],
        params: &ObserverParams<T>,
    ) -> Result<Self, ObserverError> {
        check_radius(kerr, pos, params)?;
        let tetrad = oriented_tetrad(&kerr.g_lo(pos), u, forward, up).ok_or(ObserverError::InvalidState(
            "the velocity must be future-directed and timelike, and forward must not be parallel to up",
        ))?;
        Ok(Self {
            pos: *pos,
            tetrad,
            tau: T::zero(),
            motion: Motion::Free,
            h_next: T::lit(1e-2),
        })
    }

    /// Rebuilds an observer from a saved state (bookmarks, camera paths): event, tetrad, proper time and motion.
    /// The integrator's step-size memory starts fresh, which does not affect the state, only how the next
    /// [`Observer::advance`] subdivides its interval.
    ///
    /// # Errors
    /// [`ObserverError::InvalidState`] if a component is not finite, the tetrad is not orthonormal to `1e-9`, or
    /// `e_(0)` is not future-directed; a limit error if the event is outside the allowed radii.
    pub fn from_state(
        kerr: &Kerr<T>,
        pos: &Vec4<T>,
        tetrad: &Tetrad<T>,
        tau: T,
        motion: Motion<T>,
        params: &ObserverParams<T>,
    ) -> Result<Self, ObserverError> {
        let finite = pos
            .iter()
            .chain(tetrad.e_up.iter().flatten())
            .all(|c| c.is_finite())
            && tau.is_finite();
        if !finite {
            return Err(ObserverError::InvalidState(
                "the saved state contains non-finite numbers",
            ));
        }
        check_radius(kerr, pos, params)?;
        if tetrad.orthonormality_error(&kerr.g_lo(pos)) > T::lit(1e-9) || tetrad.e_up[0][0] <= T::zero() {
            return Err(ObserverError::InvalidState(
                "the saved camera frame is not an orthonormal, future-directed tetrad for this spacetime",
            ));
        }
        Ok(Self {
            pos: *pos,
            tetrad: *tetrad,
            tau,
            motion,
            h_next: T::lit(1e-2),
        })
    }

    fn at(
        kerr: &Kerr<T>,
        pos: Vec4<T>,
        u: Vec4<T>,
        motion: Motion<T>,
        params: &ObserverParams<T>,
    ) -> Result<Self, ObserverError> {
        let (fwd, up) = look_at_hole(&pos);
        let mut obs = Self::new(kerr, &pos, &u, &fwd, &up, params)?;
        obs.motion = motion;
        Ok(obs)
    }

    /// Hover preset: static observer at spheroidal `(r, θ, φ̃)` ([`ks_position`]), held by automatic thrust,
    /// looking at the hole.
    ///
    /// # Errors
    /// [`ObserverError::InvalidState`] inside the ergosphere, where static observers do not exist.
    pub fn hover(
        kerr: &Kerr<T>,
        r: T,
        theta: T,
        phi: T,
        params: &ObserverParams<T>,
    ) -> Result<Self, ObserverError> {
        let pos = ks_position(kerr.spin(), r, theta, phi);
        check_radius(kerr, &pos, params)?;
        let u = kerr
            .stationary_velocity(&pos, T::zero())
            .ok_or(ObserverError::InvalidState(
                "hovering is impossible inside the ergosphere: no thrust can hold a static position there",
            ))?;
        let motion = Motion::Hover {
            rho: rho(&pos),
            z: pos[3],
        };
        Self::at(kerr, pos, u, motion, params)
    }

    /// ZAMO preset: zero-angular-momentum observer at spheroidal `(r, θ, φ̃)`, co-rotating with `ω(r, θ)` and held
    /// at fixed `(r, θ)` by automatic thrust, looking at the hole.
    ///
    /// # Errors
    /// [`ObserverError::InvalidState`] at or inside the outer horizon.
    pub fn zamo(
        kerr: &Kerr<T>,
        r: T,
        theta: T,
        phi: T,
        params: &ObserverParams<T>,
    ) -> Result<Self, ObserverError> {
        let pos = ks_position(kerr.spin(), r, theta, phi);
        check_radius(kerr, &pos, params)?;
        let err = ObserverError::InvalidState("a ZAMO exists only outside the event horizon");
        if r <= kerr.outer_horizon() {
            return Err(err);
        }
        let u = kerr
            .stationary_velocity(&pos, kerr.zamo_angular_velocity(&pos))
            .ok_or(err)?;
        let motion = Motion::Zamo {
            rho: rho(&pos),
            z: pos[3],
        };
        Self::at(kerr, pos, u, motion, params)
    }

    /// Circular equatorial geodesic orbit preset at radius `r` and KS azimuth `φ̃`, prograde or retrograde
    /// (free motion, no thrust), looking at the hole. Orbits below the ISCO exist but are unstable; check
    /// [`Kerr::isco`] to warn the user.
    ///
    /// # Errors
    /// [`ObserverError::InvalidState`] at or below the corresponding photon orbit.
    pub fn circular_orbit(
        kerr: &Kerr<T>,
        r: T,
        prograde: bool,
        phi: T,
        params: &ObserverParams<T>,
    ) -> Result<Self, ObserverError> {
        let err = ObserverError::InvalidState("circular orbits exist only outside the photon orbit");
        if r <= kerr.photon_orbit(prograde) {
            return Err(err);
        }
        let pos = ks_position(kerr.spin(), r, T::lit(core::f64::consts::FRAC_PI_2), phi);
        check_radius(kerr, &pos, params)?;
        let u = kerr
            .stationary_velocity(&pos, kerr.keplerian_angular_velocity(r, prograde))
            .ok_or(err)?;
        Self::at(kerr, pos, u, Motion::Free, params)
    }

    /// Radial-infall preset: free fall from rest at infinity (`E = 1`, `L_z = 0`, `Q = 0`) starting at
    /// spheroidal `(r, θ, φ̃)`, looking at the hole. For `a = 0` this is radial infall in any direction. For Kerr
    /// the observer keeps `θ` constant and is dragged in `φ` (Doran's free-fall observers); on the spin axis it is
    /// purely radial. Valid inside the horizon too. `u_μ = −dt + c(r) dr` with
    /// `c = −sqrt(2Mr) / (sqrt(2Mr) + sqrt(r² + a²))` (`docs/physics.md` § "Presets").
    ///
    /// # Errors
    /// [`ObserverError::InnerLimit`] / [`ObserverError::OuterLimit`] outside the allowed radii.
    pub fn radial_infall(
        kerr: &Kerr<T>,
        r: T,
        theta: T,
        phi: T,
        params: &ObserverParams<T>,
    ) -> Result<Self, ObserverError> {
        let pos = ks_position(kerr.spin(), r, theta, phi);
        check_radius(kerr, &pos, params)?;
        let u = infall_velocity(kerr, &pos);
        Self::at(kerr, pos, u, Motion::Free, params)
    }

    /// 4-velocity `u^μ = e_(0)^μ`.
    pub fn u(&self) -> Vec4<T> {
        self.tetrad.u()
    }

    /// Region of the current event.
    pub fn region(&self, kerr: &Kerr<T>) -> Region {
        kerr.region(&self.pos)
    }

    /// 4-acceleration `A^μ` that [`Observer::advance`] applies for body-frame thrust `thrust` (ignored when
    /// station keeping). Its magnitude `sqrt(g(A, A))` is the felt acceleration in units of `c²/M`.
    pub fn acceleration(&self, kerr: &Kerr<T>, thrust: &[T; 3]) -> Vec4<T> {
        drive_acceleration(
            kerr,
            &kerr.christoffel(&self.pos),
            self.drive(thrust),
            &self.pos,
            &self.tetrad,
        )
    }

    /// Lorentz factor relative to the local ZAMO, or `None` inside the horizon (no ZAMO there).
    pub fn zamo_lorentz_factor(&self, kerr: &Kerr<T>) -> Option<T> {
        if kerr.r(&self.pos) <= kerr.outer_horizon() {
            return None;
        }
        let zamo = kerr.stationary_velocity(&self.pos, kerr.zamo_angular_velocity(&self.pos))?;
        Some(lorentz_factor(&kerr.g_lo(&self.pos), &self.u(), &zamo))
    }

    /// Rebuilds the spatial legs from new coordinate directions `forward`, `up` (keeps `u`, position and motion).
    ///
    /// # Errors
    /// [`ObserverError::InvalidState`] if `forward ∥ up`.
    pub fn reorient(&mut self, kerr: &Kerr<T>, forward: &[T; 3], up: &[T; 3]) -> Result<(), ObserverError> {
        self.tetrad = oriented_tetrad(&kerr.g_lo(&self.pos), &self.u(), forward, up)
            .ok_or(ObserverError::InvalidState("forward must not be parallel to up"))?;
        Ok(())
    }

    /// Switches to free motion (keeps the current state; station-keeping thrust stops).
    pub fn release(&mut self) {
        self.motion = Motion::Free;
    }

    fn drive(&self, thrust: &[T; 3]) -> Drive<T> {
        match self.motion {
            Motion::Free => Drive::Thrust(*thrust),
            Motion::Hover { .. } => Drive::Station { zamo: false },
            Motion::Zamo { .. } => Drive::Station { zamo: true },
        }
    }

    /// Advances the observer by proper time `dtau` with body-frame thrust `thrust` (forward, left, up; units of
    /// `c²/M`; ignored when station keeping). Adaptive DOPRI5 substeps, re-orthonormalisation after each one.
    ///
    /// # Errors
    /// Stops at the first substep that would leave the allowed radii ([`ObserverError::InnerLimit`],
    /// [`ObserverError::OuterLimit`]) or fails ([`ObserverError::IntegrationFailure`]). The observer is then left
    /// at the last valid substep (`tau` tells how far it got).
    pub fn advance(
        &mut self,
        kerr: &Kerr<T>,
        dtau: T,
        thrust: &[T; 3],
        params: &ObserverParams<T>,
    ) -> Result<AdvanceReport<T>, ObserverError> {
        let mut report = AdvanceReport {
            substeps: 0,
            rejected: 0,
            max_constraint_error: T::zero(),
        };
        if dtau.is_nan() || dtau <= T::zero() {
            return Ok(report);
        }
        let drive = self.drive(thrust);
        let rhs = |y: &State<T>| observer_rhs(kerr, drive, y);
        let ctl = StepController::<T>::dopri5();
        let mut done = T::zero();
        let mut h = self.h_next.max(params.h_min);
        while done < dtau {
            if report.substeps + report.rejected >= params.max_substeps {
                return Err(ObserverError::IntegrationFailure("substep budget exhausted"));
            }
            let y = pack(&self.pos, &self.tetrad);
            let k1 = rhs(&y);
            let remaining = dtau - done;
            let speed = (k1[1] * k1[1] + k1[2] * k1[2] + k1[3] * k1[3]).sqrt();
            let cap = if speed > T::zero() {
                params.step_factor * kerr.r(&self.pos) / speed
            } else {
                params.h_max
            };
            let h_free = h.min(cap).min(params.h_max);
            let last = h_free >= remaining;
            let h_try = if last { remaining } else { h_free };
            let s = dopri5_step(rhs, &y, &k1, h_try);
            let err = error_norm(&y, &s.y, &s.err, params.rtol, params.atol);
            if !err.is_finite() {
                if h_try <= params.h_min {
                    return Err(ObserverError::IntegrationFailure("non-finite state"));
                }
                report.rejected += 1;
                h = (h_try * T::lit(0.2)).max(params.h_min);
                continue;
            }
            let accept = err <= T::one() || h_try <= params.h_min;
            let h_new = ctl.next_h(h_try, err).max(params.h_min);
            if !accept {
                report.rejected += 1;
                h = h_new;
                continue;
            }
            let (pos, legs) = unpack(&s.y);
            let drift = legs.orthonormality_error(&kerr.g_lo(&pos));
            report.max_constraint_error = report.max_constraint_error.max(drift);
            let (pos, tetrad) = self.correct(kerr, pos, legs, params)?;
            self.pos = pos;
            self.tetrad = tetrad;
            self.tau = self.tau + h_try;
            report.substeps += 1;
            done = if last { dtau } else { done + h_try };
            // A step shortened only to land on dtau must not shrink the remembered step size.
            if !last || h_new < h {
                h = h_new;
            }
        }
        self.h_next = h;
        Ok(report)
    }

    /// Constraint correction after a substep: snap station-keeping orbits back onto their circle, check the
    /// limits, and re-orthonormalise (Gram–Schmidt, `e_(0) ∥ u`).
    fn correct(
        &self,
        kerr: &Kerr<T>,
        mut pos: Vec4<T>,
        legs: Tetrad<T>,
        params: &ObserverParams<T>,
    ) -> Result<(Vec4<T>, Tetrad<T>), ObserverError> {
        if pos.iter().any(|c| !c.is_finite()) {
            return Err(ObserverError::IntegrationFailure("non-finite position"));
        }
        let u_seed = match self.motion {
            Motion::Free => legs.e_up[0],
            Motion::Hover { rho, z } | Motion::Zamo { rho, z } => {
                let phi = pos[2].atan2(pos[1]);
                let (sp, cp) = phi.sin_cos();
                pos = [pos[0], rho * cp, rho * sp, z];
                let omega = match self.motion {
                    Motion::Zamo { .. } => kerr.zamo_angular_velocity(&pos),
                    _ => T::zero(),
                };
                kerr.stationary_velocity(&pos, omega)
                    .ok_or(ObserverError::InvalidState(
                        "station keeping is impossible at this position",
                    ))?
            }
        };
        check_radius(kerr, &pos, params)?;
        let seeds = [u_seed, legs.e_up[1], legs.e_up[2], legs.e_up[3]];
        let tetrad = Tetrad::gram_schmidt(&kerr.g_lo(&pos), &seeds)
            .ok_or(ObserverError::IntegrationFailure("tetrad became degenerate"))?;
        Ok((pos, tetrad))
    }

    /// Changes the spin of the hole while flying (`docs/PRD.md` §5.8): keeps the KS position and the spatial
    /// velocity components `uⁱ`, solves `g(u, u) = −1` for `u^t` in the new metric (the future-directed root
    /// closest to the old `u^t`), and rebuilds the tetrad from the old legs. Station-keeping observers are rebuilt
    /// for the new metric (ZAMOs pick up the new `ω`). On error the observer is unchanged and the caller should
    /// reset to a safe preset.
    ///
    /// # Errors
    /// [`ObserverError::InvalidState`] if no valid state exists in the new spacetime (e.g. a hover position is
    /// now inside the ergosphere, or no timelike `u` has these `uⁱ`), or a limit error for the new `r_min`.
    pub fn change_spin(&mut self, new: &Kerr<T>, params: &ObserverParams<T>) -> Result<(), ObserverError> {
        check_radius(new, &self.pos, params)?;
        let g = new.g_lo(&self.pos);
        let u = match self.motion {
            Motion::Free => renormalise_time_component(&g, &self.u()).ok_or(ObserverError::InvalidState(
                "the current velocity has no timelike counterpart for the new spin",
            ))?,
            Motion::Hover { .. } => {
                new.stationary_velocity(&self.pos, T::zero())
                    .ok_or(ObserverError::InvalidState(
                        "the hover position is inside the ergosphere for the new spin",
                    ))?
            }
            Motion::Zamo { .. } => {
                if new.r(&self.pos) <= new.outer_horizon() {
                    return Err(ObserverError::InvalidState(
                        "the ZAMO position is inside the horizon for the new spin",
                    ));
                }
                new.stationary_velocity(&self.pos, new.zamo_angular_velocity(&self.pos))
                    .ok_or(ObserverError::InvalidState(
                        "no ZAMO at this position for the new spin",
                    ))?
            }
        };
        let seeds = [u, self.tetrad.e_up[1], self.tetrad.e_up[2], self.tetrad.e_up[3]];
        self.tetrad = Tetrad::gram_schmidt(&g, &seeds).ok_or(ObserverError::InvalidState(
            "the camera frame is degenerate for the new spin",
        ))?;
        Ok(())
    }
}

fn rho<T: Real>(pos: &Vec4<T>) -> T {
    (pos[1] * pos[1] + pos[2] * pos[2]).sqrt()
}

/// Checks `r_min ≤ r ≤ r_max` (`docs/PRD.md` §5.8).
fn check_radius<T: Real>(
    kerr: &Kerr<T>,
    pos: &Vec4<T>,
    params: &ObserverParams<T>,
) -> Result<(), ObserverError> {
    let r = kerr.r(pos);
    let r_min = params.r_min(kerr);
    if !r.is_finite() {
        return Err(ObserverError::IntegrationFailure("non-finite radius"));
    }
    if r < r_min {
        return Err(ObserverError::InnerLimit {
            r: r.to_f64_lossy(),
            r_min: r_min.to_f64_lossy(),
        });
    }
    if r > params.r_max {
        return Err(ObserverError::OuterLimit {
            r: r.to_f64_lossy(),
            r_max: params.r_max.to_f64_lossy(),
        });
    }
    Ok(())
}

/// Solves `g_tt (u^t)² + 2 g_ti uⁱ u^t + g_ij uⁱ uʲ = −1` for `u^t > 0`, keeping `uⁱ`. Of the future-directed roots
/// (`u^t > 0`, which in KS is future-directed: `g(u, n) = −u^t` for the KS-normal observer `n`), returns the one
/// closest to the old `u^t`.
fn renormalise_time_component<T: Real>(g: &Mat4<T>, u_old: &Vec4<T>) -> Option<Vec4<T>> {
    let a = g[0][0];
    let b = (1..4).fold(T::zero(), |s, i| s + g[0][i] * u_old[i]);
    let c = (1..4).fold(T::one(), |s, i| {
        s + (1..4).fold(T::zero(), |t, j| t + g[i][j] * u_old[i] * u_old[j])
    });
    let mut roots = [T::nan(); 2];
    if a.abs() > T::lit(1e-14) {
        let disc = b * b - a * c;
        if disc < T::zero() {
            return None;
        }
        let sq = disc.sqrt();
        roots = [(-b - sq) / a, (-b + sq) / a];
    } else if b.abs() > T::zero() {
        roots[0] = -c / (T::lit(2.0) * b);
    }
    let ut = roots.into_iter().filter(|t| *t > T::zero()).min_by(|p, q| {
        let dp = (*p - u_old[0]).abs();
        let dq = (*q - u_old[0]).abs();
        dp.partial_cmp(&dq).unwrap_or(core::cmp::Ordering::Equal)
    })?;
    Some([ut, u_old[1], u_old[2], u_old[3]])
}

/// 4-velocity of the zero-angular-momentum observer falling from rest at infinity (`E = 1`, `L_z = Q = 0`):
/// `u_μ = (−1, c ∂_x r, c ∂_y r, c ∂_z r)`, `c = −sqrt(2Mr) / (sqrt(2Mr) + sqrt(r² + a²))`, raised with `g^{μν}`
/// (`docs/physics.md` § "Presets").
fn infall_velocity<T: Real>(kerr: &Kerr<T>, pos: &Vec4<T>) -> Vec4<T> {
    let (_, d) = crate::kerr_schild::ks_field_derivatives(kerr.mass(), kerr.spin(), pos);
    let r = kerr.r(pos);
    let a = kerr.spin();
    let s = (T::lit(2.0) * kerr.mass() * r).sqrt();
    let c = -s / (s + (r * r + a * a).sqrt());
    let u_lo = [-T::one(), c * d.dr[1], c * d.dr[2], c * d.dr[3]];
    raise(&kerr.g_up(pos), &u_lo)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kerr_schild::ks_dg_lo;
    use crate::linalg::{determinant, lower};
    use approx::assert_relative_eq;
    use proptest::prelude::*;
    use std::f64::consts::{FRAC_PI_2, PI};

    fn params() -> ObserverParams<f64> {
        ObserverParams::default()
    }

    #[test]
    fn regions() {
        // a = 0.9: r_+ = 1.4359, r_E(π/2) = 2, r_E(0) = r_+.
        let k = Kerr::new(0.9_f64).unwrap();
        let eq = |r: f64| ks_position(0.9, r, FRAC_PI_2, 0.3);
        assert_eq!(k.region(&eq(2.5)), Region::OutsideErgosphere);
        assert_eq!(k.region(&eq(1.8)), Region::Ergosphere);
        assert_eq!(k.region(&eq(1.2)), Region::InsideHorizon);
        // On the axis the ergosurface touches the horizon: no ergosphere.
        assert_eq!(k.region(&[0.0, 0.0, 0.0, 1.5]), Region::OutsideErgosphere);
        let s = Kerr::<f64>::schwarzschild();
        assert_eq!(s.region(&[0.0, 2.1, 0.0, 0.0]), Region::OutsideErgosphere);
        assert_eq!(s.region(&[0.0, 1.9, 0.0, 0.0]), Region::InsideHorizon);
    }

    #[test]
    fn static_acceleration_in_schwarzschild() {
        // |a| = M / (r² sqrt(1 − 2M/r)) (MTW; same as connection::tests), now via the stationary formula.
        let k = Kerr::<f64>::schwarzschild();
        for r in [2.5, 4.0, 30.0] {
            let pos = ks_position(0.0, r, 1.1, 0.4);
            let acc = k.stationary_acceleration(&pos, 0.0).unwrap();
            let g = k.g_lo(&pos);
            let mag = inner(&g, &acc, &acc).sqrt();
            assert_relative_eq!(mag, 1.0 / (r * r * (1.0 - 2.0 / r).sqrt()), max_relative = 1e-12);
        }
    }

    #[test]
    fn keplerian_orbits_need_no_thrust() {
        // Ω = ±1/(r^{3/2} ± a) is geodesic (Appendix A): the stationary acceleration vanishes. Terms are
        // Γ u u ~ M/r² ~ 1e-2; f64 rounding over 16 terms gives ≲ 1e-15.
        for a in [0.0, 0.5, 0.9, 0.999] {
            let k = Kerr::new(a).unwrap();
            for pro in [true, false] {
                for r in [k.photon_orbit(pro) + 0.5, 8.0, 40.0] {
                    let pos = ks_position(a, r, FRAC_PI_2, 1.0);
                    let acc = k
                        .stationary_acceleration(&pos, k.keplerian_angular_velocity(r, pro))
                        .unwrap();
                    for c in acc {
                        assert!(c.abs() < 1e-13, "a = {a}, r = {r}, pro = {pro}: {acc:?}");
                    }
                }
            }
        }
    }

    proptest! {
        #[test]
        fn zamo_has_zero_angular_momentum(a in 0.0..0.999_f64, r_off in 0.05..30.0_f64, theta in 0.0..PI, phi in -PI..PI) {
            // ZAMO: u_φ = L_z = 0, and its acceleration is orthogonal to u.
            let k = Kerr::new(a).unwrap();
            let r = k.outer_horizon() + r_off;
            let pos = ks_position(a, r, theta, phi);
            let w = k.zamo_angular_velocity(&pos);
            prop_assert!(w >= 0.0);
            let u = k.stationary_velocity(&pos, w).unwrap();
            let g = k.g_lo(&pos);
            let u_lo = lower(&g, &u);
            prop_assert!(k.angular_momentum_z(&pos, &u_lo).abs() < 1e-12 * (1.0 + r * r));
            let acc = k.stationary_acceleration(&pos, w).unwrap();
            let scale = acc.iter().fold(1.0_f64, |m, c| m.max(c.abs())) * u[0];
            prop_assert!(inner(&g, &acc, &u).abs() < 1e-12 * scale);
        }

        #[test]
        fn fermi_walker_preserves_inner_products(
            (a, pos) in crate::test_util::event(),
            v in proptest::array::uniform3(-0.4..0.4_f64),
            th in proptest::array::uniform3(-2.0..2.0_f64),
        ) {
            // d/dτ g(e_a, e_b) = ∂_γ g_{μν} u^γ e_a^μ e_b^ν + g(ė_a, e_b) + g(e_a, ė_b) = 0 for the transport law
            // (docs/physics.md § "Fermi–Walker transport"), with thrust. Terms are ~ (1 + f)³ |Γ| |e|²; check
            // to 1e-10 relative.
            let k = Kerr::new(a).unwrap();
            let g = k.g_lo(&pos);
            let g_up = k.g_up(&pos);
            let n = [-g_up[0][0], -g_up[1][0], -g_up[2][0], -g_up[3][0]];
            // u = n + w (0, v), w halved until safely timelike (deterministic, no rejects; as in tetrad::tests).
            let mut w = 1.0;
            let mut u = n;
            for _ in 0..60 {
                u = [n[0], n[1] + w * v[0], n[2] + w * v[1], n[3] + w * v[2]];
                if inner(&g, &u, &u) < 0.5 * inner(&g, &n, &n) {
                    break;
                }
                w *= 0.5;
            }
            let seeds = [u, [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]];
            let t = Tetrad::gram_schmidt(&g, &seeds).unwrap();
            let y = pack(&pos, &t);
            let dy = observer_rhs(&k, Drive::Thrust(th), &y);
            let (_, de) = unpack(&dy);
            let dg = ks_dg_lo(1.0, a, &pos);
            let uu = t.u();
            let mut scale = 1.0_f64;
            for aa in 0..4 {
                for bb in 0..4 {
                    let (ea, eb) = (&t.e_up[aa], &t.e_up[bb]);
                    let mut d = inner(&g, &de.e_up[aa], eb) + inner(&g, ea, &de.e_up[bb]);
                    for ga in 0..4 {
                        for mu in 0..4 {
                            for nu in 0..4 {
                                let term = dg[ga][mu][nu] * uu[ga] * ea[mu] * eb[nu];
                                scale = scale.max(term.abs());
                                d += term;
                            }
                        }
                    }
                    prop_assert!(d.abs() < 1e-10 * scale, "({aa},{bb}): {d}");
                }
            }
            // e_(0) obeys the equation of motion: du/dτ = −Γuu + A.
            let acc = t.to_coordinate(&[0.0, th[0], th[1], th[2]]);
            let geo = k.geodesic_acceleration(&pos, &uu);
            for mu in 0..4 {
                prop_assert!((de.e_up[0][mu] - geo[mu] - acc[mu]).abs() < 1e-10 * scale);
            }
        }

        #[test]
        fn infall_velocity_is_rest_at_infinity(a in 0.0..0.999_f64, r_off in 0.05..50.0_f64, theta in 0.0..PI, phi in -PI..PI) {
            // u·u = −1, E = −u_t = 1, L_z = 0, Q = 0, future-directed, and dr/dτ = −sqrt(2Mr(r² + a²))/Σ (Kerr radial
            // equation with E = μ = 1, L = Q = 0: R = (r² + a²) 2Mr). Valid across the horizon.
            let k = Kerr::new(a).unwrap();
            let r = k.inner_horizon() + r_off;
            let pos = ks_position(a, r, theta, phi);
            let u = infall_velocity(&k, &pos);
            let g = k.g_lo(&pos);
            let u_lo = lower(&g, &u);
            let scale = u.iter().fold(1.0_f64, |m, c| m.max(c.abs())).powi(2);
            prop_assert!((inner(&g, &u, &u) + 1.0).abs() < 1e-12 * scale);
            prop_assert!(u[0] > 0.0);
            prop_assert!((k.energy(&u_lo) - 1.0).abs() < 1e-12 * scale);
            prop_assert!(k.angular_momentum_z(&pos, &u_lo).abs() < 1e-12 * scale * (1.0 + r));
            prop_assert!(k.carter_q(&pos, &u_lo).abs() < 1e-10 * scale * (1.0 + r * r));
            let (_, d) = crate::kerr_schild::ks_field_derivatives(1.0, a, &pos);
            let rdot: f64 = (1..4).map(|i| d.dr[i] * u[i]).sum();
            let sigma = r * r + a * a * (theta.cos()).powi(2);
            let expect = -(2.0 * r * (r * r + a * a)).sqrt() / sigma;
            prop_assert!((rdot - expect).abs() < 1e-11 * scale);
        }
    }

    #[test]
    fn presets_are_valid_and_right_handed() {
        let p = params();
        let k = Kerr::new(0.7_f64).unwrap();
        let obs = [
            Observer::hover(&k, 6.0, 1.0, 0.2, &p).unwrap(),
            Observer::hover(&k, 6.0, 0.0, 0.0, &p).unwrap(), // on the axis
            Observer::zamo(&k, 1.9, FRAC_PI_2, 0.0, &p).unwrap(), // in the ergosphere
            Observer::circular_orbit(&k, 8.0, true, 0.0, &p).unwrap(),
            Observer::circular_orbit(&k, 8.0, false, 0.0, &p).unwrap(),
            Observer::radial_infall(&k, 10.0, 0.0, 0.0, &p).unwrap(),
            Observer::radial_infall(&k, 1.2, 0.8, 0.0, &p).unwrap(), // inside the horizon
        ];
        for o in obs {
            let g = k.g_lo(&o.pos);
            assert!(o.tetrad.orthonormality_error(&g) < 1e-12);
            assert!(o.u()[0] > 0.0);
            assert!(determinant(&o.tetrad.e_up) > 0.0, "left-handed tetrad");
            // Forward leg points toward the hole: negative Euclidean projection on the position vector.
            let f = o.tetrad.e_up[1];
            assert!(f[1] * o.pos[1] + f[2] * o.pos[2] + f[3] * o.pos[3] < 0.0);
        }
    }

    #[test]
    fn invalid_presets_are_rejected() {
        let p = params();
        let k = Kerr::new(0.9_f64).unwrap();
        assert!(matches!(
            Observer::hover(&k, 1.8, FRAC_PI_2, 0.0, &p),
            Err(ObserverError::InvalidState(_))
        ));
        assert!(Observer::zamo(&k, 1.3, FRAC_PI_2, 0.0, &p).is_err());
        assert!(Observer::circular_orbit(&k, k.photon_orbit(false) - 0.01, false, 0.0, &p).is_err());
        assert!(Observer::circular_orbit(&k, k.photon_orbit(true) + 0.01, true, 0.0, &p).is_ok());
        assert!(matches!(
            Observer::radial_infall(&k, k.inner_horizon() + 0.01, 0.0, 0.0, &p),
            Err(ObserverError::InnerLimit { .. })
        ));
        assert!(matches!(
            Observer::hover(&k, 2000.0, 1.0, 0.0, &p),
            Err(ObserverError::OuterLimit { .. })
        ));
    }

    #[test]
    fn hover_stays_put_with_a_constant_frame_in_schwarzschild() {
        // Static frames in Schwarzschild are non-rotating, so the FW-transported legs keep their KS components.
        let k = Kerr::<f64>::schwarzschild();
        let mut o = Observer::hover(&k, 5.0, 1.2, 0.7, &params()).unwrap();
        let start = o;
        let rep = o.advance(&k, 50.0, &[0.3, 0.0, 0.0], &params()).unwrap();
        assert_relative_eq!(o.tau, 50.0, max_relative = 1e-14);
        for i in 1..4 {
            assert_relative_eq!(o.pos[i], start.pos[i], epsilon = 1e-12);
        }
        // t advances at u^t = (1 − 2/r)^{-1/2}.
        assert_relative_eq!(o.pos[0], 50.0 / (1.0 - 0.4_f64).sqrt(), max_relative = 1e-10);
        for a in 0..4 {
            for mu in 0..4 {
                assert!((o.tetrad.e_up[a][mu] - start.tetrad.e_up[a][mu]).abs() < 1e-9);
            }
        }
        assert!(rep.max_constraint_error < 1e-9);
        // The felt acceleration equals the static one.
        let acc = o.acceleration(&k, &[0.0; 3]);
        let mag = inner(&k.g_lo(&o.pos), &acc, &acc).sqrt();
        assert_relative_eq!(mag, 1.0 / (25.0 * 0.6_f64.sqrt()), max_relative = 1e-12);
    }

    #[test]
    fn zamo_corotates_with_frame_dragging() {
        // Frame-dragging sign check through the observer API: a ZAMO at a = 0.9 advances in +φ at dφ/dt = ω > 0,
        // at fixed (r, θ).
        let k = Kerr::new(0.9_f64).unwrap();
        let mut o = Observer::zamo(&k, 3.0, 1.0, 0.0, &params()).unwrap();
        let w = k.zamo_angular_velocity(&o.pos);
        assert!(w > 0.0);
        let (r0, z0) = (k.r(&o.pos), o.pos[3]);
        let phi0 = o.pos[2].atan2(o.pos[1]);
        o.advance(&k, 2.0, &[0.0; 3], &params()).unwrap();
        let dphi = o.pos[2].atan2(o.pos[1]) - phi0;
        assert_relative_eq!(dphi / o.pos[0], w, max_relative = 1e-9);
        assert_relative_eq!(k.r(&o.pos), r0, max_relative = 1e-13);
        assert_relative_eq!(o.pos[3], z0, max_relative = 1e-13);
        assert_relative_eq!(o.zamo_lorentz_factor(&k).unwrap(), 1.0, max_relative = 1e-12);
    }

    #[test]
    fn constant_thrust_gives_hyperbolic_motion_in_flat_space() {
        // M → 0: proper acceleration α along forward (+x) from rest gives x = x₀ + (cosh ατ − 1)/α,
        // t = sinh(ατ)/α (Rindler), and the FW frame does not rotate (forward stays along x). M = 1e-12 makes
        // curvature effects ~1e-13.
        let k = Kerr::with_mass(1e-12_f64, 0.0).unwrap();
        let p = params();
        let mut o = Observer::new(
            &k,
            &[0.0, 10.0, 0.0, 0.0],
            &[1.0, 0.0, 0.0, 0.0],
            &[1.0, 0.0, 0.0],
            &[0.0, 0.0, 1.0],
            &p,
        )
        .unwrap();
        let alpha = 0.5;
        let tau = 3.0;
        o.advance(&k, tau, &[alpha, 0.0, 0.0], &p).unwrap();
        assert_relative_eq!(
            o.pos[1],
            10.0 + ((alpha * tau).cosh() - 1.0) / alpha,
            max_relative = 1e-9
        );
        assert_relative_eq!(o.pos[0], (alpha * tau).sinh() / alpha, max_relative = 1e-9);
        assert!(o.pos[2].abs() < 1e-9 && o.pos[3].abs() < 1e-9);
        let gamma = (alpha * tau).cosh();
        assert_relative_eq!(o.u()[0], gamma, max_relative = 1e-9);
        // Forward leg = (sinh, cosh, 0, 0); left and up unchanged.
        assert_relative_eq!(o.tetrad.e_up[1][1], gamma, max_relative = 1e-9);
        assert!((o.tetrad.e_up[2][2] - 1.0).abs() < 1e-9 && (o.tetrad.e_up[3][3] - 1.0).abs() < 1e-9);
    }

    #[test]
    fn spin_change_keeps_spatial_velocity() {
        let p = params();
        let k0 = Kerr::<f64>::schwarzschild();
        let k1 = Kerr::new(0.9_f64).unwrap();
        let mut o = Observer::radial_infall(&k0, 8.0, 1.0, 0.5, &p).unwrap();
        let old = o.u();
        o.change_spin(&k1, &p).unwrap();
        let g = k1.g_lo(&o.pos);
        assert!(o.tetrad.orthonormality_error(&g) < 1e-12);
        for i in 1..4 {
            assert_relative_eq!(o.u()[i], old[i], max_relative = 1e-12);
        }
        assert!(determinant(&o.tetrad.e_up) > 0.0);

        // Hover at r = 1.9 on the equator: fine for a = 0, inside the ergosphere for a = 0.9.
        let mut h = Observer::hover(&k0, 2.1, FRAC_PI_2, 0.0, &p).unwrap();
        let before = h;
        assert!(matches!(
            h.change_spin(&k1, &p),
            Err(ObserverError::InvalidState(_))
        ));
        assert_eq!(
            h, before,
            "a failed spin change must leave the observer unchanged"
        );

        // ZAMO picks up the new ω.
        let mut z = Observer::zamo(&k0, 5.0, 1.0, 0.0, &p).unwrap();
        z.change_spin(&k1, &p).unwrap();
        assert_relative_eq!(z.zamo_lorentz_factor(&k1).unwrap(), 1.0, max_relative = 1e-12);
    }

    #[test]
    fn acceleration_unit_conversion() {
        // Static observer at r = 10 M around 10 M_☉: a = GM/(R² sqrt(1 − 2GM/(Rc²))) with R = 10 GM/c², in SI.
        let gm = 10.0 * 1.327_124_4e20;
        let c2 = SPEED_OF_LIGHT_M_S * SPEED_OF_LIGHT_M_S;
        let r_si = 10.0 * gm / c2;
        let a_si = gm / (r_si * r_si * (1.0 - 0.2_f64).sqrt());
        let a_code = 1.0 / (100.0 * 0.8_f64.sqrt());
        assert_relative_eq!(
            acceleration_in_g(a_code, 10.0),
            a_si / STANDARD_GRAVITY_M_S2,
            max_relative = 1e-12
        );
        // GM_☉/c² ≈ 1476.625 m.
        assert_relative_eq!(SOLAR_MASS_LENGTH_M, 1476.625, max_relative = 1e-6);
    }

    #[test]
    fn lorentz_factor_and_speed() {
        let g = crate::linalg::eta::<f64>();
        let b: f64 = 0.6;
        let gam = 1.0 / (1.0 - b * b).sqrt();
        let lf = lorentz_factor(&g, &[1.0, 0.0, 0.0, 0.0], &[gam, gam * b, 0.0, 0.0]);
        assert_relative_eq!(lf, 1.25, max_relative = 1e-15);
        assert_relative_eq!(speed_from_lorentz_factor(lf), b, max_relative = 1e-14);
    }

    #[test]
    fn renormalisation_picks_the_future_root() {
        // Flat space, uⁱ = (0.6, 0, 0) ⇒ u^t = 1.166…; inside a horizon (g_tt > 0) both roots may be positive.
        let g = crate::linalg::eta::<f64>();
        let u = renormalise_time_component(&g, &[5.0, 0.6, 0.0, 0.0]).unwrap();
        assert_relative_eq!(u[0], 1.36_f64.sqrt(), max_relative = 1e-15);
        let k = Kerr::<f64>::schwarzschild();
        let pos = [0.0, 1.0, 0.0, 0.0];
        let o = infall_velocity(&k, &pos);
        let back = renormalise_time_component(&k.g_lo(&pos), &o).unwrap();
        assert_relative_eq!(back[0], o[0], max_relative = 1e-12);
    }

    #[test]
    fn from_state_restores_exactly_and_validates() {
        let kerr = Kerr::new(0.7).unwrap();
        let params = ObserverParams::default();
        let mut obs = Observer::circular_orbit(&kerr, 9.0, true, 0.3, &params).unwrap();
        obs.advance(&kerr, 5.0, &[0.0; 3], &params).unwrap();
        let back = Observer::from_state(&kerr, &obs.pos, &obs.tetrad, obs.tau, obs.motion, &params).unwrap();
        assert_eq!(
            (back.pos, back.tetrad, back.tau, back.motion),
            (obs.pos, obs.tetrad, obs.tau, obs.motion)
        );
        // Rejected: non-finite, wrong spacetime (tetrad not orthonormal for a = 0), past-directed, out of range.
        let mut bad = obs.pos;
        bad[1] = f64::NAN;
        assert!(Observer::from_state(&kerr, &bad, &obs.tetrad, 0.0, Motion::Free, &params).is_err());
        let schw = Kerr::new(0.0).unwrap();
        assert!(Observer::from_state(&schw, &obs.pos, &obs.tetrad, 0.0, Motion::Free, &params).is_err());
        let mut flipped = obs.tetrad;
        flipped.e_up[0] = flipped.e_up[0].map(|c| -c);
        assert!(Observer::from_state(&kerr, &obs.pos, &flipped, 0.0, Motion::Free, &params).is_err());
        let far = [0.0, 2000.0, 0.0, 0.0];
        assert!(Observer::from_state(&kerr, &far, &obs.tetrad, 0.0, Motion::Free, &params).is_err());
    }
}
