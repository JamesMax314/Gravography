//! Interactive flight (`docs/PRD.md` WP-11, §5.8, FR-O1–FR-O5): the observer advanced every frame, thrust from
//! the keyboard, the mouse look rotation, presets, time warp, spin changes and safe resets. Contains no GPU or
//! window code, so all state transitions are unit tested.
//!
//! - **Time:** each frame advances the proper time by `Δτ = Δt_wall × time_warp` (time warp in `M` per wall-clock
//!   second; `Δt_wall` clamped to [`MAX_FRAME_DT`] so a stalled frame does not cause a jump). While paused,
//!   [`Flight::request_step`] advances by [`FlightSettings::step_tau`] once.
//! - **Thrust:** the held keys give a unit direction in the *view* frame. It is rotated into the body frame with the
//!   look rotation (`Tetrad::rotated` convention: `v_body = R · v_view`) and scaled by
//!   [`FlightSettings::thrust`] (units of `c²/M`). Hover and ZAMO observers keep station by automatic thrust and
//!   ignore user thrust, so pressing a thrust key while station keeping switches to free flight first.
//! - **Cut thrust (Space):** releases station keeping (free fall from the current state).
//! - **Errors** from the integration (inner/outer limit, failure) and invalid spin changes reset the observer to
//!   the last applied preset (or [`safe_preset`] if that is not valid any more) and pause, with a message.

use gravography_physics::observer::acceleration_in_g;
use gravography_physics::{Kerr, Motion, Observer, ObserverParams, Region, Tetrad, Vec4, linalg::inner};
use gravography_render::scene::{LookSpec, MAX_SPIN, ObserverSpec, PresetKind};

use super::input::ThrustKeys;

/// Longest wall-clock frame time fed to the simulation, in seconds.
pub const MAX_FRAME_DT: f64 = 0.1;
/// Pitch limit of the mouse look, degrees (avoids flipping over the zenith).
pub const PITCH_LIMIT_DEG: f64 = 89.0;
/// Range of the time warp, `M` of proper time per wall-clock second.
pub const WARP_RANGE: (f64, f64) = (1e-3, 1e3);
/// Range of the thrust magnitude, units of `c²/M`.
pub const THRUST_RANGE: (f64, f64) = (1e-5, 1.0);
/// Largest camera radius in the viewer, `M`: keeps the f32 GPU tracer in its validated range (`docs/PRD.md` §5.9).
pub const VIEWER_R_MAX: f64 = 200.0;

/// User-adjustable flight settings.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FlightSettings {
    /// Proper time per wall-clock second, `M/s`.
    pub time_warp: f64,
    /// Simulation paused.
    pub paused: bool,
    /// Thrust magnitude while a thrust key is held, `c²/M`.
    pub thrust: f64,
    /// Black-hole mass in solar masses (only for displaying accelerations in g).
    pub mass_solar: f64,
    /// Proper time of one single step while paused, `M`.
    pub step_tau: f64,
}

impl Default for FlightSettings {
    /// 1 M/s, running, thrust 0.01 c²/M (about ten times the hover thrust at r = 30), M = 10 M☉, step 0.1 M.
    fn default() -> Self {
        Self {
            time_warp: 1.0,
            paused: false,
            thrust: 0.01,
            mass_solar: 10.0,
            step_tau: 0.1,
        }
    }
}

impl FlightSettings {
    /// Clamps every value into its supported range (UI and keyboard input go through here).
    pub fn clamped(mut self) -> Self {
        let clamp = |v: f64, (lo, hi): (f64, f64)| if v.is_finite() { v.clamp(lo, hi) } else { lo };
        self.time_warp = clamp(self.time_warp, WARP_RANGE);
        self.thrust = clamp(self.thrust, THRUST_RANGE);
        self.mass_solar = clamp(self.mass_solar, (1e-3, 1e12));
        self.step_tau = clamp(self.step_tau, (1e-4, 100.0));
        self
    }
}

/// What the renderer needs from the flight state: the spacetime and the camera event and tetrad (look rotation
/// applied).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Camera {
    /// The black hole.
    pub kerr: Kerr<f64>,
    /// KS event of the camera.
    pub position: Vec4<f64>,
    /// Camera tetrad (forward, left, up) including the look rotation.
    pub tetrad: Tetrad<f64>,
}

/// Relative tolerance below which camera changes are ignored (they would not move the image by a pixel: one pixel
/// is ~1e-3 rad), so a station-keeping observer whose state only jitters at round-off still converges.
pub const CAMERA_TOLERANCE: f64 = 1e-9;

impl Camera {
    /// Whether the rendered image would change from `other` to `self`: a different spin, or a spatial position or
    /// tetrad component differing by more than [`CAMERA_TOLERANCE`] (relative, `|Δ| > tol · (1 + |v|)`). The KS
    /// time is ignored: the spacetime is stationary.
    pub fn differs(&self, other: &Self) -> bool {
        let far =
            |a: f64, b: f64| (a - b).abs() > CAMERA_TOLERANCE * (1.0 + b.abs()) || a.is_nan() != b.is_nan();
        self.kerr != other.kerr
            || (1..4).any(|i| far(self.position[i], other.position[i]))
            || (0..4).any(|a| (0..4).any(|mu| far(self.tetrad.e_up[a][mu], other.tetrad.e_up[a][mu])))
    }
}

/// Outcome of checking a preset before applying it (for the panel: errors disable the button, warnings are shown).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PresetCheck {
    /// Why the preset cannot be applied.
    pub error: Option<String>,
    /// Applicable, but worth knowing (e.g. an unstable orbit).
    pub warning: Option<String>,
}

/// Checks whether `spec` can be placed in `kerr` within `params` (`docs/PRD.md` §5.8): hovering needs `r` outside
/// the ergosurface, a ZAMO needs `r > r_+`, a circular orbit needs `r` above the corresponding photon orbit (and is
/// unstable below the ISCO), and every preset needs `r_min ≤ r ≤ r_max`.
pub fn check_preset(kerr: &Kerr<f64>, spec: &ObserverSpec, params: &ObserverParams<f64>) -> PresetCheck {
    let err = |m: String| PresetCheck {
        error: Some(m),
        warning: None,
    };
    let r = spec.r;
    if !(r.is_finite() && spec.theta_deg.is_finite() && spec.phi_deg.is_finite()) {
        return err("r, θ and φ must be finite".into());
    }
    let r_min = params.r_min(kerr);
    if r < r_min {
        return err(format!(
            "r must be at least r_min = {r_min:.3} M (the inner limit near the singularity / inner horizon)"
        ));
    }
    if r > params.r_max {
        return err(format!("r must be at most {:.0} M", params.r_max));
    }
    let theta = spec.theta_deg.to_radians();
    match spec.preset {
        PresetKind::Hover => {
            let r_e = kerr.ergosurface(theta);
            if r <= r_e {
                return err(format!(
                    "no static observers at r ≤ {r_e:.3} M (the ergosurface at θ = {:.0}°, or the horizon): \
                     everything there is dragged or falls in, and no thrust can hold a fixed position",
                    spec.theta_deg
                ));
            }
        }
        PresetKind::Zamo => {
            let rp = kerr.outer_horizon();
            if r <= rp {
                return err(format!(
                    "ZAMOs exist only outside the event horizon, r > r₊ = {rp:.3} M"
                ));
            }
        }
        PresetKind::CircularOrbit => {
            let r_ph = kerr.photon_orbit(spec.prograde);
            if r <= r_ph {
                return err(format!(
                    "circular orbits exist only above the photon orbit, r > {r_ph:.3} M"
                ));
            }
            let isco = kerr.isco(spec.prograde);
            if r < isco {
                return PresetCheck {
                    error: None,
                    warning: Some(format!(
                        "below the ISCO (r = {isco:.3} M): this orbit is unstable and will decay or escape"
                    )),
                };
            }
        }
        PresetKind::RadialInfall => {}
    }
    PresetCheck::default()
}

/// The fallback preset: hovering at `r = 30 M` in the equatorial plane, valid for every allowed spin (the
/// ergosurface is at `r ≤ 2 M`).
pub fn safe_preset() -> ObserverSpec {
    ObserverSpec {
        preset: PresetKind::Hover,
        r: 30.0,
        theta_deg: 90.0,
        phi_deg: 0.0,
        prograde: true,
        boost: [0.0; 3],
        look: LookSpec::default(),
    }
}

/// Short name of a preset for messages.
pub fn preset_name(kind: PresetKind) -> &'static str {
    match kind {
        PresetKind::Hover => "hover",
        PresetKind::Zamo => "ZAMO",
        PresetKind::CircularOrbit => "circular orbit",
        PresetKind::RadialInfall => "radial infall",
    }
}

/// Observer integration settings of the viewer: the CPU defaults with `r_max` = [`VIEWER_R_MAX`].
pub fn viewer_params() -> ObserverParams<f64> {
    ObserverParams {
        r_max: VIEWER_R_MAX,
        ..ObserverParams::default()
    }
}

/// Result of one [`Flight::update`].
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FrameUpdate {
    /// The observer was integrated this frame (the camera may have moved).
    pub advanced: bool,
    /// Something the user should be told (mode switch, reset).
    pub message: Option<String>,
}

/// The interactive observer and its controls.
#[derive(Clone, Debug)]
pub struct Flight {
    /// The black hole.
    pub kerr: Kerr<f64>,
    /// The observer (body frame, without the look rotation).
    pub observer: Observer<f64>,
    /// Look rotation relative to the body frame.
    pub look: LookSpec,
    /// Time warp, pause, thrust, mass.
    pub settings: FlightSettings,
    /// Last applied preset (boost and look cleared): the target of resets.
    preset: ObserverSpec,
    params: ObserverParams<f64>,
    step_pending: bool,
}

impl Flight {
    /// Starts at the scene's observer (preset, boost and look) in `kerr`.
    ///
    /// # Errors
    /// The message of the placement error (invalid preset, `|β| ≥ 1`, out of range).
    pub fn new(kerr: Kerr<f64>, spec: &ObserverSpec) -> Result<Self, String> {
        let params = viewer_params();
        let observer = spec.place_boosted(&kerr, &params).map_err(|e| e.to_string())?;
        Ok(Self {
            kerr,
            observer,
            look: spec.look,
            settings: FlightSettings::default(),
            preset: ObserverSpec {
                boost: [0.0; 3],
                look: LookSpec::default(),
                ..*spec
            },
            params,
            step_pending: false,
        })
    }

    /// Integration settings in use.
    pub fn params(&self) -> &ObserverParams<f64> {
        &self.params
    }

    /// Last applied preset.
    pub fn preset(&self) -> &ObserverSpec {
        &self.preset
    }

    /// The camera to render.
    pub fn camera(&self) -> Camera {
        Camera {
            kerr: self.kerr,
            position: self.observer.pos,
            tetrad: self.observer.tetrad.rotated(&self.look.matrix()),
        }
    }

    /// Turns the view by `(Δyaw, Δpitch)` degrees (yaw wrapped to `(−180°, 180°]`, pitch clamped to
    /// ±[`PITCH_LIMIT_DEG`]).
    pub fn look_by(&mut self, d_yaw: f64, d_pitch: f64) {
        if !(d_yaw.is_finite() && d_pitch.is_finite()) {
            return;
        }
        // 180° − ((180° − y) mod 360°) lies in (−180°, 180°].
        self.look.yaw_deg = 180.0 - (180.0 - (self.look.yaw_deg + d_yaw)).rem_euclid(360.0);
        self.look.pitch_deg = (self.look.pitch_deg + d_pitch).clamp(-PITCH_LIMIT_DEG, PITCH_LIMIT_DEG);
    }

    /// Resets the look rotation (forward = the body frame's forward leg).
    pub fn reset_look(&mut self) {
        self.look = LookSpec::default();
    }

    /// Requests one step of [`FlightSettings::step_tau`] (only acts while paused).
    pub fn request_step(&mut self) {
        self.step_pending = true;
    }

    /// Body-frame thrust (forward, left, up; `c²/M`) for the held keys: the view-frame direction rotated by the look
    /// rotation and scaled by the thrust magnitude.
    pub fn thrust_body(&self, keys: &ThrustKeys) -> [f64; 3] {
        let d = keys.direction();
        let rot = self.look.matrix();
        core::array::from_fn(|i| self.settings.thrust * (0..3).map(|j| rot[i][j] * d[j]).sum::<f64>())
    }

    /// Space: cuts all thrust. Station keeping stops (free fall from the current state). Returns a message when the
    /// motion mode changed.
    pub fn cut_thrust(&mut self) -> Option<String> {
        if self.observer.motion == Motion::Free {
            return None;
        }
        self.observer.release();
        Some("thrust cut: station keeping off, now in free fall".into())
    }

    /// Advances the simulation by one frame of `dt_wall` seconds with the held thrust keys.
    pub fn update(&mut self, dt_wall: f64, keys: &ThrustKeys) -> FrameUpdate {
        let dt = if dt_wall > 0.0 {
            dt_wall.min(MAX_FRAME_DT)
        } else {
            0.0
        };
        let step = std::mem::take(&mut self.step_pending);
        let dtau = if self.settings.paused {
            if step { self.settings.step_tau } else { 0.0 }
        } else {
            dt * self.settings.time_warp
        };
        let mut out = FrameUpdate::default();
        if dtau.is_nan() || dtau <= 0.0 {
            return out;
        }
        if keys.any() && self.observer.motion != Motion::Free {
            self.observer.release();
            out.message = Some("station keeping off: free flight under thrust".into());
        }
        let thrust = self.thrust_body(keys);
        out.advanced = true;
        if let Err(e) = self.observer.advance(&self.kerr, dtau, &thrust, &self.params) {
            out.message = Some(self.reset(&e.to_string()));
        }
        out
    }

    /// Applies a preset from the panel (boost and look of `spec` are ignored; the look rotation is reset). Returns
    /// a warning to show, if any.
    ///
    /// # Errors
    /// Why the preset cannot be applied (the observer is unchanged).
    pub fn apply_preset(&mut self, spec: &ObserverSpec) -> Result<Option<String>, String> {
        let check = check_preset(&self.kerr, spec, &self.params);
        if let Some(e) = check.error {
            return Err(e);
        }
        let spec = ObserverSpec {
            boost: [0.0; 3],
            look: LookSpec::default(),
            ..*spec
        };
        self.observer = spec.place(&self.kerr, &self.params).map_err(|e| e.to_string())?;
        self.look = LookSpec::default();
        self.preset = spec;
        Ok(check.warning)
    }

    /// Changes the spin while flying (`docs/PRD.md` §5.8, FR-O5): keeps the position and spatial velocity
    /// ([`Observer::change_spin`]). If the state is not valid for the new spin, resets (and pauses) with a message.
    /// The spin is clamped to `[0, 0.999]`.
    pub fn set_spin(&mut self, spin: f64) -> Option<String> {
        let a = if spin.is_finite() {
            spin.clamp(0.0, MAX_SPIN)
        } else {
            0.0
        };
        if a.to_bits() == self.kerr.spin().to_bits() {
            return None;
        }
        let new = Kerr::new(a).expect("a ∈ [0, 0.999] is a valid spin");
        let mut obs = self.observer;
        match obs.change_spin(&new, &self.params) {
            Ok(()) => {
                self.kerr = new;
                self.observer = obs;
                None
            }
            Err(e) => {
                self.kerr = new;
                Some(self.reset(&format!("spin changed to a = {a:.3}: {e}")))
            }
        }
    }

    /// Resets to the last preset (or [`safe_preset`] if that is not valid for the current spin) and pauses.
    /// Returns the message for the user.
    pub fn reset(&mut self, reason: &str) -> String {
        let last = self.preset;
        let target = match last.place(&self.kerr, &self.params) {
            Ok(obs) => {
                self.observer = obs;
                last
            }
            Err(_) => {
                let safe = safe_preset();
                self.observer = safe
                    .place(&self.kerr, &self.params)
                    .expect("the safe preset (hover at r = 30 M) is valid for every allowed spin");
                self.preset = safe;
                safe
            }
        };
        self.look = LookSpec::default();
        self.settings.paused = true;
        format!(
            "{reason}. Reset to the {} preset at r = {} M; paused (Enter resumes).",
            preset_name(target.preset),
            target.r
        )
    }
}

/// Head-up display values (`docs/PRD.md` FR-O4; FPS, render scale and ray statistics come from the renderer).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hud {
    /// Proper time `τ`, `M`.
    pub tau: f64,
    /// KS coordinate time `t`, `M` (regular across the horizon, unlike BL `t`).
    pub t_ks: f64,
    /// BL radius `r`, `M`.
    pub r: f64,
    /// BL polar angle `θ`, degrees.
    pub theta_deg: f64,
    /// BL azimuth `φ`, degrees (`None` where it is not finite: on the horizons).
    pub phi_deg: Option<f64>,
    /// Speed relative to the local ZAMO `(β, γ)` (`None` inside the horizon, where there are no ZAMOs).
    pub zamo_speed: Option<(f64, f64)>,
    /// Felt (proper) acceleration from thrust or station keeping, `c²/M`.
    pub acceleration: f64,
    /// The same in multiples of `g₀` for [`FlightSettings::mass_solar`].
    pub acceleration_g: f64,
    /// Region of spacetime.
    pub region: Region,
    /// Spin `a/M`.
    pub spin: f64,
    /// Motion mode.
    pub motion: Motion<f64>,
}

impl Hud {
    /// HUD values for the current state with the held keys.
    pub fn new(flight: &Flight, keys: &ThrustKeys) -> Self {
        let (kerr, obs) = (&flight.kerr, &flight.observer);
        let bl = kerr.ks_to_bl_position(&obs.pos);
        let acc = obs.acceleration(kerr, &flight.thrust_body(keys));
        let a2 = inner(&kerr.g_lo(&obs.pos), &acc, &acc);
        let acceleration = if a2.is_finite() {
            a2.max(0.0).sqrt()
        } else {
            f64::NAN
        };
        let zamo_speed = obs
            .zamo_lorentz_factor(kerr)
            .filter(|g| g.is_finite())
            .map(|g| (gravography_physics::observer::speed_from_lorentz_factor(g), g));
        Self {
            tau: obs.tau,
            t_ks: obs.pos[0],
            r: kerr.r(&obs.pos),
            theta_deg: bl[2].to_degrees(),
            phi_deg: bl[3].is_finite().then(|| bl[3].to_degrees()),
            zamo_speed,
            acceleration,
            acceleration_g: acceleration_in_g(acceleration, flight.settings.mass_solar),
            region: obs.region(kerr),
            spin: kerr.spin(),
            motion: obs.motion,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::input::ThrustKey;
    use super::*;
    use approx::assert_relative_eq;

    fn spec(preset: PresetKind, r: f64, theta_deg: f64) -> ObserverSpec {
        ObserverSpec {
            preset,
            r,
            theta_deg,
            ..safe_preset()
        }
    }

    fn flight(a: f64, s: &ObserverSpec) -> Flight {
        Flight::new(Kerr::new(a).unwrap(), s).unwrap()
    }

    fn keys(held: &[ThrustKey]) -> ThrustKeys {
        let mut k = ThrustKeys::default();
        for &h in held {
            k.set(h, true);
        }
        k
    }

    /// Runs `n` frames of 1/60 s.
    fn run(f: &mut Flight, n: usize, k: &ThrustKeys) -> Vec<FrameUpdate> {
        (0..n).map(|_| f.update(1.0 / 60.0, k)).collect()
    }

    #[test]
    fn hovering_camera_stays_put_and_converges() {
        // A static observer in Schwarzschild is at rest and, with no thrust input, its Fermi–Walker frame does not
        // rotate (docs/physics.md § "Validation"): the camera must not change beyond round-off, otherwise the viewer
        // would restart accumulation every frame and never converge.
        let mut f = flight(0.0, &safe_preset());
        let c0 = f.camera();
        let ups = run(&mut f, 600, &ThrustKeys::default());
        assert!(ups.iter().all(|u| u.advanced && u.message.is_none()));
        assert!(!f.camera().differs(&c0));
        assert_relative_eq!(f.observer.tau, 10.0, max_relative = 1e-12); // 600 × (1/60 s) × 1 M/s
        assert_eq!(
            f.observer.motion,
            Motion::Hover {
                rho: 30.0,
                z: f.observer.pos[3]
            }
        );
    }

    #[test]
    fn time_warp_pause_and_single_step() {
        let mut f = flight(0.0, &safe_preset());
        f.settings.time_warp = 4.0;
        run(&mut f, 30, &ThrustKeys::default());
        assert_relative_eq!(f.observer.tau, 2.0, max_relative = 1e-12);
        // A stalled frame is clamped to MAX_FRAME_DT; nonsense frame times do nothing.
        f.update(5.0, &ThrustKeys::default());
        assert_relative_eq!(f.observer.tau, 2.0 + 4.0 * MAX_FRAME_DT, max_relative = 1e-12);
        for bad in [0.0, -1.0, f64::NAN] {
            assert!(!f.update(bad, &ThrustKeys::default()).advanced);
        }
        f.settings.paused = true;
        let tau = f.observer.tau;
        assert!(!run(&mut f, 10, &ThrustKeys::default()).iter().any(|u| u.advanced));
        assert_eq!(f.observer.tau, tau);
        f.request_step();
        assert!(f.update(1.0 / 60.0, &ThrustKeys::default()).advanced);
        assert_relative_eq!(f.observer.tau, tau + f.settings.step_tau, max_relative = 1e-12);
        assert!(
            !f.update(1.0 / 60.0, &ThrustKeys::default()).advanced,
            "one step per request"
        );
    }

    #[test]
    fn thrust_key_releases_station_keeping_and_accelerates_forward() {
        // Hover at r = 30 looking at the hole; W pushes toward the hole.
        let mut f = flight(0.0, &safe_preset());
        let w = keys(&[ThrustKey::Forward]);
        let ups = run(&mut f, 1, &w);
        assert!(ups[0].message.as_deref().unwrap().contains("free flight"));
        assert_eq!(f.observer.motion, Motion::Free);
        // Felt acceleration equals the thrust magnitude (thrust is a body-frame 4-acceleration, A·u = 0).
        let hud = Hud::new(&f, &w);
        assert_relative_eq!(hud.acceleration, f.settings.thrust, max_relative = 1e-12);
        // Thrust 0.01 c²/M toward the hole (plus gravity, ≈ 1.1e-3) for 5 M: r drops by about ½ a τ² ≈ 0.14 M.
        run(&mut f, 299, &w);
        let r = f.kerr.r(&f.observer.pos);
        assert!(r < 30.0 - 0.1 && r > 30.0 - 0.2, "r = {r}");
    }

    #[test]
    fn thrust_follows_the_look_rotation() {
        let mut f = flight(0.0, &safe_preset());
        f.look_by(90.0, 0.0); // looking along the old left leg
        let t = f.thrust_body(&keys(&[ThrustKey::Forward]));
        assert_relative_eq!(t[0], 0.0, epsilon = 1e-17);
        assert_relative_eq!(t[1], f.settings.thrust, max_relative = 1e-15);
        // Thrust "up" with the view pitched up 30° tilts forward-up in the body frame.
        f.reset_look();
        f.look_by(0.0, 30.0);
        let t = f.thrust_body(&keys(&[ThrustKey::Up]));
        let th = f.settings.thrust;
        assert_relative_eq!(t[0], -th * 0.5, max_relative = 1e-14);
        assert_relative_eq!(t[2], th * 0.75_f64.sqrt(), max_relative = 1e-14);
        // The camera's forward leg is the thrust direction for W.
        let cam = f.camera();
        let fwd = f.observer.tetrad.to_coordinate(&{
            let t = f.thrust_body(&keys(&[ThrustKey::Forward]));
            [0.0, t[0] / th, t[1] / th, t[2] / th]
        });
        for mu in 0..4 {
            assert_relative_eq!(fwd[mu], cam.tetrad.e_up[1][mu], epsilon = 1e-14);
        }
    }

    #[test]
    fn look_wraps_yaw_and_clamps_pitch() {
        let mut f = flight(0.0, &safe_preset());
        f.look_by(270.0, 120.0);
        assert_relative_eq!(f.look.yaw_deg, -90.0, epsilon = 1e-12);
        assert_eq!(f.look.pitch_deg, PITCH_LIMIT_DEG);
        f.look_by(-90.0, f64::NAN);
        assert_relative_eq!(f.look.yaw_deg, -90.0, epsilon = 1e-12); // NaN input ignored
        f.look_by(-90.0, -500.0);
        assert_relative_eq!(f.look.yaw_deg, 180.0, epsilon = 1e-12);
        assert_eq!(f.look.pitch_deg, -PITCH_LIMIT_DEG);
        assert!(f.camera().differs(&flight(0.0, &safe_preset()).camera()));
    }

    #[test]
    fn cut_thrust_switches_to_free_fall() {
        let mut f = flight(0.0, &safe_preset());
        assert!(f.cut_thrust().is_some());
        assert_eq!(f.observer.motion, Motion::Free);
        assert!(f.cut_thrust().is_none());
        // Released from rest at r = 30, it falls inward.
        run(&mut f, 120, &ThrustKeys::default());
        assert!(f.kerr.r(&f.observer.pos) < 30.0);
    }

    #[test]
    fn preset_checks_follow_the_physics() {
        // PRD §5.8: hover only outside the ergosphere, ZAMO only outside the horizon, circular orbits above the
        // photon orbit (warning below the ISCO), all within [r_min, r_max].
        let k = Kerr::new(0.9).unwrap();
        let p = viewer_params();
        let ok = |s: ObserverSpec| check_preset(&k, &s, &p);
        let r_e = k.ergosurface(core::f64::consts::FRAC_PI_2); // 2 at the equator
        assert!(ok(spec(PresetKind::Hover, r_e - 0.01, 90.0)).error.is_some());
        assert_eq!(
            ok(spec(PresetKind::Hover, r_e + 0.01, 90.0)),
            PresetCheck::default()
        );
        // On the axis the ergosurface touches the horizon.
        assert_eq!(ok(spec(PresetKind::Hover, 1.6, 0.0)), PresetCheck::default());
        assert!(
            ok(spec(PresetKind::Zamo, k.outer_horizon(), 90.0))
                .error
                .is_some()
        );
        assert_eq!(ok(spec(PresetKind::Zamo, 1.8, 90.0)), PresetCheck::default());
        let below_ph = ObserverSpec {
            prograde: false,
            ..spec(PresetKind::CircularOrbit, k.photon_orbit(false) - 0.01, 90.0)
        };
        assert!(ok(below_ph).error.is_some());
        let unstable = spec(
            PresetKind::CircularOrbit,
            0.5 * (k.photon_orbit(true) + k.isco(true)),
            90.0,
        );
        let c = ok(unstable);
        assert!(c.error.is_none() && c.warning.as_deref().unwrap().contains("ISCO"));
        assert_eq!(
            ok(spec(PresetKind::CircularOrbit, 10.0, 90.0)),
            PresetCheck::default()
        );
        assert_eq!(
            ok(spec(PresetKind::RadialInfall, 1.2, 30.0)),
            PresetCheck::default()
        );
        assert!(
            ok(spec(PresetKind::RadialInfall, p.r_min(&k) - 0.01, 30.0))
                .error
                .is_some()
        );
        assert!(
            ok(spec(PresetKind::RadialInfall, VIEWER_R_MAX + 1.0, 30.0))
                .error
                .is_some()
        );
        assert!(ok(spec(PresetKind::Hover, f64::NAN, 90.0)).error.is_some());
        // Every preset that passes the check can actually be placed.
        for kind in [
            PresetKind::Hover,
            PresetKind::Zamo,
            PresetKind::CircularOrbit,
            PresetKind::RadialInfall,
        ] {
            for r in [1.0, 1.5, 1.9, 2.1, 3.0, 5.0, 50.0] {
                for th in [0.0, 30.0, 90.0] {
                    let s = spec(kind, r, th);
                    if ok(s).error.is_none() {
                        assert!(s.place(&k, &p).is_ok(), "{s:?}");
                    }
                }
            }
        }
    }

    #[test]
    fn apply_preset_replaces_the_observer_or_refuses() {
        let mut f = flight(0.9, &safe_preset());
        f.look_by(10.0, 5.0);
        let before = f.observer;
        let err = f.apply_preset(&spec(PresetKind::Hover, 1.9, 90.0)).unwrap_err();
        assert!(err.contains("ergosurface"), "{err}");
        assert_eq!(f.observer, before);
        let warn = f
            .apply_preset(&spec(PresetKind::CircularOrbit, 2.0, 90.0))
            .unwrap();
        assert!(warn.is_some());
        assert_relative_eq!(f.kerr.r(&f.observer.pos), 2.0, max_relative = 1e-12);
        assert_eq!(f.look, LookSpec::default());
        assert_eq!(f.preset().preset, PresetKind::CircularOrbit);
    }

    #[test]
    fn circular_orbit_keeps_its_radius_in_flight() {
        // Per-frame integration (many short advance calls) must keep a stable geodesic orbit on its circle:
        // r = 10, a = 0.5 prograde, 10 M/s for 60 s ≈ 600 M ≈ 3 orbits (period 2π(r^{3/2} + a) ≈ 202 M).
        let s = ObserverSpec {
            preset: PresetKind::CircularOrbit,
            r: 10.0,
            ..safe_preset()
        };
        let mut f = flight(0.5, &s);
        f.settings.time_warp = 10.0;
        let ups = run(&mut f, 3600, &ThrustKeys::default());
        assert!(ups.iter().all(|u| u.message.is_none()));
        // DOPRI5 rtol = 1e-10 over ~1e4 substeps: radius drift well below 1e-6.
        assert_relative_eq!(f.kerr.r(&f.observer.pos), 10.0, max_relative = 1e-6);
    }

    #[test]
    fn falling_through_the_horizon_then_resetting_at_the_inner_limit() {
        // Radial infall from r = 6 (Schwarzschild): crosses the horizon, the HUD reports it, then the inner limit
        // r_min = 0.05 stops the simulation and resets to the preset, paused. Total τ(6 → 0) = (2/3) sqrt(6³/2)
        // ≈ 6.93 M (PRD Appendix A), so 10 M/s for 1 s is enough.
        let s = spec(PresetKind::RadialInfall, 6.0, 90.0);
        let mut f = flight(0.0, &s);
        f.settings.time_warp = 10.0;
        let mut inside = false;
        let mut reset_msg = None;
        for _ in 0..120 {
            let u = f.update(1.0 / 60.0, &ThrustKeys::default());
            if Hud::new(&f, &ThrustKeys::default()).region == Region::InsideHorizon {
                inside = true;
                assert!(Hud::new(&f, &ThrustKeys::default()).zamo_speed.is_none());
            }
            if let Some(m) = u.message {
                reset_msg = Some(m);
                break;
            }
        }
        assert!(inside, "never saw the inside of the horizon");
        let m = reset_msg.expect("the inner limit was not reached");
        assert!(
            m.contains("r_min") && m.contains("radial infall") && m.contains("paused"),
            "{m}"
        );
        assert!(f.settings.paused);
        assert_relative_eq!(f.kerr.r(&f.observer.pos), 6.0, max_relative = 1e-12);
        assert_eq!(f.observer.tau, 0.0);
    }

    #[test]
    fn outer_limit_resets() {
        let mut f = flight(0.0, &spec(PresetKind::Hover, 190.0, 90.0));
        f.settings.time_warp = 1000.0;
        f.settings.thrust = 1.0;
        let back = keys(&[ThrustKey::Back]);
        let msg = run(&mut f, 60, &back)
            .into_iter()
            .find_map(|u| u.message.filter(|m| m.contains("r_max")));
        assert!(msg.is_some());
        assert!(f.settings.paused);
    }

    #[test]
    fn spin_change_keeps_or_resets() {
        // Hover at r = 30: valid for every spin, the observer stays (now static in Kerr).
        let mut f = flight(0.0, &safe_preset());
        assert!(f.set_spin(0.9).is_none());
        assert_eq!(f.kerr.spin(), 0.9);
        assert_eq!(
            Hud::new(&f, &ThrustKeys::default()).region,
            Region::OutsideErgosphere
        );
        // Clamped to 0.999; same value is a no-op.
        assert!(f.set_spin(1.5).is_none());
        assert_eq!(f.kerr.spin(), MAX_SPIN);
        assert!(f.set_spin(MAX_SPIN).is_none());
        // A ZAMO at r = 1.6 (a = 0.9, outside r₊ ≈ 1.436) keeps its KS position, which for a = 0 is at
        // r = sqrt(1.6² + 0.9²) ≈ 1.84 < 2: inside the Schwarzschild horizon, where no ZAMO exists.
        let mut f = flight(0.9, &spec(PresetKind::Zamo, 1.6, 90.0));
        let msg = f.set_spin(0.0).expect("the ZAMO is inside the a = 0 horizon");
        assert!(msg.contains("spin changed") && msg.contains("paused"), "{msg}");
        // The last preset is invalid for a = 0 too, so the safe preset is used.
        assert_eq!(*f.preset(), safe_preset());
        assert_relative_eq!(f.kerr.r(&f.observer.pos), 30.0, max_relative = 1e-12);
        assert_eq!(f.kerr.spin(), 0.0);
    }

    #[test]
    fn hud_reports_station_keeping_and_motion() {
        // Static observer at r = 30, a = 0: felt acceleration M / (r² sqrt(1 − 2M/r)) (the Schwarzschild hover
        // acceleration), zero speed relative to the ZAMO (the ZAMO is static for a = 0).
        let f = flight(0.0, &safe_preset());
        let h = Hud::new(&f, &ThrustKeys::default());
        assert_relative_eq!(
            h.acceleration,
            1.0 / (900.0 * (1.0 - 2.0 / 30.0_f64).sqrt()),
            max_relative = 1e-12
        );
        let (beta, gamma) = h.zamo_speed.unwrap();
        assert_relative_eq!(gamma, 1.0, epsilon = 1e-12);
        assert!(beta < 1e-5);
        assert_relative_eq!(h.r, 30.0, max_relative = 1e-12);
        assert_relative_eq!(h.theta_deg, 90.0, epsilon = 1e-12);
        assert!(h.phi_deg.is_some());
        assert_eq!(h.region, Region::OutsideErgosphere);
        assert_relative_eq!(
            h.acceleration_g,
            acceleration_in_g(h.acceleration, f.settings.mass_solar),
            max_relative = 1e-15
        );
        // Circular orbit at r = 10 (a = 0): free, no acceleration, speed vs the static ZAMO
        // β = sqrt(M / (r − 2M)) = sqrt(1/8) (Schwarzschild orbital speed measured by static observers).
        let f = flight(0.0, &spec(PresetKind::CircularOrbit, 10.0, 90.0));
        let h = Hud::new(&f, &ThrustKeys::default());
        assert_eq!(h.motion, Motion::Free);
        assert!(h.acceleration < 1e-12);
        assert_relative_eq!(h.zamo_speed.unwrap().0, 0.125_f64.sqrt(), max_relative = 1e-12);
    }

    #[test]
    fn scene_boost_starts_in_free_flight() {
        let s = ObserverSpec {
            boost: [0.0, 0.9, 0.0],
            ..spec(PresetKind::Hover, 10.0, 90.0)
        };
        let f = flight(0.0, &s);
        assert_eq!(f.observer.motion, Motion::Free);
        let (_, gamma) = Hud::new(&f, &ThrustKeys::default()).zamo_speed.unwrap();
        assert_relative_eq!(gamma, 1.0 / (1.0 - 0.81_f64).sqrt(), max_relative = 1e-12);
        // Resets go to the unboosted preset.
        assert_eq!(f.preset().boost, [0.0; 3]);
        assert!(
            Flight::new(
                Kerr::new(0.0).unwrap(),
                &ObserverSpec {
                    boost: [1.0, 0.0, 0.0],
                    ..s
                }
            )
            .is_err()
        );
    }

    #[test]
    fn settings_are_clamped() {
        let s = FlightSettings {
            time_warp: 1e9,
            thrust: f64::NAN,
            mass_solar: -1.0,
            step_tau: 0.0,
            paused: false,
        }
        .clamped();
        assert_eq!(s.time_warp, WARP_RANGE.1);
        assert_eq!(s.thrust, THRUST_RANGE.0);
        assert!(s.mass_solar > 0.0 && s.step_tau > 0.0);
        assert_eq!(FlightSettings::default().clamped(), FlightSettings::default());
    }
}
