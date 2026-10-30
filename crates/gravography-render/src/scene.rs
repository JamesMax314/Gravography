//! Scene files (`scenes/*.toml`, `docs/PRD.md` FR-X2/FR-X3): spacetime, observer, camera, sky and render settings.
//! The schema is documented in `README.md` § "Scene files". Every section and field except `[observer]` has a
//! default; unknown fields are rejected so typos fail loudly.

use std::path::Path;

use gravography_physics::{Kerr, Observer, ObserverParams, TraceParams};
use gravography_sky::{Band, BlackbodyLut, DiffuseBandParams, SkyParams, StarParams};
use serde::{Deserialize, Serialize};

use crate::RenderError;
use crate::camera::Projection;
use crate::cpu::CpuSettings;
use crate::frame::{Effects, FrameParams};
use crate::tonemap::{Exposure, ToneMap};

/// Largest spin accepted in scenes (`docs/PRD.md` §2: `a ∈ [0, 0.999]`).
pub const MAX_SPIN: f64 = 0.999;

/// A complete scene.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Scene {
    /// The black hole.
    #[serde(default)]
    pub spacetime: SpacetimeSpec,
    /// The camera's worldline state and orientation.
    pub observer: ObserverSpec,
    /// Image size, projection and sampling.
    #[serde(default)]
    pub camera: CameraSpec,
    /// Procedural sky.
    #[serde(default)]
    pub sky: SkySpec,
    /// Shading, exposure and integrator settings.
    #[serde(default)]
    pub render: RenderSpec,
}

/// `[spacetime]`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SpacetimeSpec {
    /// Spin `a/M` in `[0, 0.999]` (0 = Schwarzschild).
    pub spin: f64,
}

/// Observer preset (`docs/PRD.md` §5.8).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PresetKind {
    /// Static observer held by thrust (outside the ergosphere only).
    #[default]
    Hover,
    /// Zero-angular-momentum observer held at fixed `(r, θ)` (outside the horizon only).
    Zamo,
    /// Circular equatorial geodesic orbit (above the photon orbit; `theta_deg` is ignored).
    CircularOrbit,
    /// Free fall from rest at infinity, passing through `(r, θ, φ)` (valid inside the horizon).
    RadialInfall,
}

/// `[observer]`. Fields a preset does not use are ignored.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct ObserverSpec {
    /// Preset.
    pub preset: PresetKind,
    /// Boyer–Lindquist (spheroidal) radius `r` in units of `M`.
    pub r: f64,
    /// Polar angle from the spin axis, degrees.
    pub theta_deg: f64,
    /// Kerr–Schild azimuth `φ̃`, degrees (`φ̃ = 0` is the `+x` side for `a = 0`).
    pub phi_deg: f64,
    /// Orbit direction for `circular_orbit`.
    pub prograde: bool,
    /// Extra velocity `(forward, left, up)` in units of `c`, `|β| < 1`, relative to the preset observer and in
    /// its frame (before the look rotation): a Lorentz boost of the camera ([`gravography_physics::Tetrad::boosted`]).
    pub boost: [f64; 3],
    /// Look rotation relative to the default view (forward toward the hole, up toward `+z`).
    pub look: LookSpec,
}

impl Default for ObserverSpec {
    /// Hovering at `r = 30` in the equatorial plane.
    fn default() -> Self {
        Self {
            preset: PresetKind::Hover,
            r: 30.0,
            theta_deg: 90.0,
            phi_deg: 0.0,
            prograde: true,
            boost: [0.0; 3],
            look: LookSpec::default(),
        }
    }
}

impl ObserverSpec {
    /// Places the preset observer in `kerr` (no boost or look rotation).
    ///
    /// # Errors
    /// [`RenderError::InvalidScene`] for non-finite coordinates; [`RenderError::Observer`] if the preset cannot be
    /// realised (e.g. hovering inside the ergosphere) or lies outside the radii allowed by `params`.
    pub fn place(
        &self,
        kerr: &Kerr<f64>,
        params: &ObserverParams<f64>,
    ) -> Result<Observer<f64>, RenderError> {
        let (th, ph) = (self.theta_deg.to_radians(), self.phi_deg.to_radians());
        if !(self.r.is_finite() && th.is_finite() && ph.is_finite()) {
            return Err(RenderError::InvalidScene(
                "observer r, theta_deg, phi_deg must be finite".into(),
            ));
        }
        let obs = match self.preset {
            PresetKind::Hover => Observer::hover(kerr, self.r, th, ph, params),
            PresetKind::Zamo => Observer::zamo(kerr, self.r, th, ph, params),
            PresetKind::CircularOrbit => Observer::circular_orbit(kerr, self.r, self.prograde, ph, params),
            PresetKind::RadialInfall => Observer::radial_infall(kerr, self.r, th, ph, params),
        }?;
        Ok(obs)
    }

    /// Places the preset observer and applies [`ObserverSpec::boost`] to its tetrad. A boosted observer no longer
    /// follows the station-keeping orbit, so its motion becomes [`gravography_physics::Motion::Free`].
    ///
    /// # Errors
    /// As [`ObserverSpec::place`]; [`RenderError::InvalidScene`] unless `|β| < 1`.
    pub fn place_boosted(
        &self,
        kerr: &Kerr<f64>,
        params: &ObserverParams<f64>,
    ) -> Result<Observer<f64>, RenderError> {
        let mut obs = self.place(kerr, params)?;
        let boost = self.boost;
        obs.tetrad = obs
            .tetrad
            .boosted(&boost)
            .ok_or_else(|| RenderError::InvalidScene(format!("boost {boost:?} must have |β| < 1")))?;
        if boost.iter().any(|b| b.abs() > 0.0) {
            obs.release();
        }
        Ok(obs)
    }
}

/// Look rotation in degrees, applied as yaw (about up, positive turns left), then pitch (about the new left,
/// positive looks up), then roll (about the new forward, positive turns the view clockwise).
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct LookSpec {
    /// Yaw in degrees.
    pub yaw_deg: f64,
    /// Pitch in degrees.
    pub pitch_deg: f64,
    /// Roll in degrees.
    pub roll_deg: f64,
}

impl LookSpec {
    /// Rotation for [`gravography_physics::Tetrad::rotated`] (column `i` = new leg `i` in the old frame),
    /// `R = R_up(yaw) · R_left(−pitch) · R_forward(roll)` in the `(forward, left, up)` basis.
    pub fn matrix(&self) -> [[f64; 3]; 3] {
        let (sy, cy) = self.yaw_deg.to_radians().sin_cos();
        let (sp, cp) = self.pitch_deg.to_radians().sin_cos();
        let (sr, cr) = self.roll_deg.to_radians().sin_cos();
        let yaw = [[cy, -sy, 0.0], [sy, cy, 0.0], [0.0, 0.0, 1.0]];
        let pitch = [[cp, 0.0, -sp], [0.0, 1.0, 0.0], [sp, 0.0, cp]];
        let roll = [[1.0, 0.0, 0.0], [0.0, cr, sr], [0.0, -sr, cr]];
        mat_mul(&mat_mul(&yaw, &pitch), &roll)
    }
}

fn mat_mul(a: &[[f64; 3]; 3], b: &[[f64; 3]; 3]) -> [[f64; 3]; 3] {
    core::array::from_fn(|i| core::array::from_fn(|j| (0..3).map(|k| a[i][k] * b[k][j]).sum()))
}

/// `[camera]`.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct CameraSpec {
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
    /// Projection, e.g. `{ kind = "rectilinear", fov_deg = 60.0 }`.
    pub projection: Projection,
    /// Samples per pixel.
    pub samples: u32,
}

impl Default for CameraSpec {
    /// 960 × 540, 60° vertical field of view, 1 sample per pixel.
    fn default() -> Self {
        Self {
            width: 960,
            height: 540,
            projection: Projection::default(),
            samples: 1,
        }
    }
}

/// Observing band (`docs/PRD.md` §6.1): human vision or a false-colour composite.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BandKind {
    /// CIE 1931 human vision.
    #[default]
    HumanVision,
    /// J, H, K (1.1–2.4 µm) as blue, green, red.
    NearInfrared,
    /// 200–380 nm in three windows as blue, green, red.
    NearUltraviolet,
    /// 2–8, 1–2, 0.3–1 keV as blue, green, red.
    SoftXray,
}

impl BandKind {
    /// Every band, in UI order.
    pub const ALL: [Self; 4] = [
        Self::HumanVision,
        Self::NearInfrared,
        Self::NearUltraviolet,
        Self::SoftXray,
    ];

    /// The band definition.
    pub fn band(self) -> Band {
        match self {
            Self::HumanVision => Band::human_vision(),
            Self::NearInfrared => Band::near_infrared(),
            Self::NearUltraviolet => Band::near_ultraviolet(),
            Self::SoftXray => Band::soft_xray(),
        }
    }

    /// Its blackbody table (default size and range).
    pub fn lut(self) -> BlackbodyLut {
        BlackbodyLut::for_band(&self.band())
    }

    /// Short UI name.
    pub fn label(self) -> &'static str {
        match self {
            Self::HumanVision => "human vision",
            Self::NearInfrared => "near infrared (false colour)",
            Self::NearUltraviolet => "near ultraviolet (false colour)",
            Self::SoftXray => "soft X-ray (false colour)",
        }
    }
}

/// `[sky]`.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct SkySpec {
    /// Star catalogue seed.
    pub seed: u64,
    /// Number of stars.
    pub star_count: usize,
    /// Cube-map face size (power of two).
    pub face_size: usize,
    /// Diffuse galactic band on/off.
    pub diffuse_band: bool,
    /// Galactic pole (KS Cartesian axes; need not be normalised).
    pub galactic_normal: [f64; 3],
    /// Observing band: `"human_vision"` (default), `"near_infrared"`, `"near_ultraviolet"` or `"soft_xray"`.
    pub band: BandKind,
}

impl Default for SkySpec {
    /// The defaults of [`SkyParams`].
    fn default() -> Self {
        let p = SkyParams::default();
        Self {
            seed: p.stars.seed,
            star_count: p.stars.count,
            face_size: p.face_size,
            diffuse_band: p.diffuse.is_some(),
            galactic_normal: p.galactic_normal,
            band: BandKind::HumanVision,
        }
    }
}

/// `[render]`.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct RenderSpec {
    /// Exposure, e.g. `{ auto = true, ev = 0.0 }`.
    pub exposure: Exposure,
    /// Tone-mapping operator: `"aces"` or `"reinhard"`.
    pub tone_map: ToneMap,
    /// Effect toggles, e.g. `{ lensing = true, frequency_shift = true, beaming = true }`.
    pub effects: Effects,
    /// Relative tolerance of the CPU ray integrator (DOPRI5).
    pub rtol: f64,
}

impl Default for RenderSpec {
    fn default() -> Self {
        Self {
            exposure: Exposure::default(),
            tone_map: ToneMap::default(),
            effects: Effects::default(),
            rtol: TraceParams::<f64>::cpu_reference().rtol,
        }
    }
}

impl Scene {
    /// Parses a scene from TOML text.
    ///
    /// # Errors
    /// [`RenderError::SceneParse`] for invalid TOML, unknown fields or wrong types.
    pub fn from_toml(text: &str) -> Result<Self, RenderError> {
        Ok(toml::from_str(text)?)
    }

    /// Serialises the scene to TOML (all fields written out).
    ///
    /// # Errors
    /// [`RenderError::SceneWrite`] (should not happen for finite values).
    pub fn to_toml(&self) -> Result<String, RenderError> {
        Ok(toml::to_string(self)?)
    }

    /// Reads a scene file.
    ///
    /// # Errors
    /// I/O or parse errors.
    pub fn load(path: &Path) -> Result<Self, RenderError> {
        Self::from_toml(&std::fs::read_to_string(path)?)
    }

    /// The black hole.
    ///
    /// # Errors
    /// [`RenderError::InvalidScene`] if the spin is outside `[0, 0.999]`.
    pub fn kerr(&self) -> Result<Kerr<f64>, RenderError> {
        let a = self.spacetime.spin;
        if !(0.0..=MAX_SPIN).contains(&a) {
            return Err(RenderError::InvalidScene(format!(
                "spin must be in [0, {MAX_SPIN}], got {a}"
            )));
        }
        Ok(Kerr::new(a)?)
    }

    /// Places the observer (preset only: no boost or look rotation).
    ///
    /// # Errors
    /// Invalid spin, or the preset cannot be realised (e.g. hovering inside the ergosphere).
    pub fn observer(&self) -> Result<Observer<f64>, RenderError> {
        self.observer.place(&self.kerr()?, &ObserverParams::default())
    }

    /// Frame parameters: observer tetrad with boost and look rotation applied, camera and render settings.
    ///
    /// # Errors
    /// Any invalid value (spin, observer, boost `|β| ≥ 1`, image size, samples, projection).
    pub fn frame(&self) -> Result<FrameParams, RenderError> {
        let kerr = self.kerr()?;
        let obs = self.observer.place_boosted(&kerr, &ObserverParams::default())?;
        let tetrad = obs.tetrad.rotated(&self.observer.look.matrix());
        let frame = FrameParams {
            kerr,
            position: obs.pos,
            tetrad,
            width: self.camera.width,
            height: self.camera.height,
            projection: self.camera.projection,
            samples: self.camera.samples,
            effects: self.render.effects,
            exposure: self.render.exposure,
            tone_map: self.render.tone_map,
        };
        frame.validate()?;
        Ok(frame)
    }

    /// Sky bake parameters (star-catalogue and diffuse-band shapes keep their library defaults).
    ///
    /// # Errors
    /// [`RenderError::InvalidScene`] for a face size that is not a power of two, or no stars and no band.
    pub fn sky_params(&self) -> Result<SkyParams, RenderError> {
        let s = &self.sky;
        if !s.face_size.is_power_of_two() || s.face_size > 8192 {
            return Err(RenderError::InvalidScene(format!(
                "sky face_size must be a power of two ≤ 8192, got {}",
                s.face_size
            )));
        }
        Ok(SkyParams {
            face_size: s.face_size,
            galactic_normal: s.galactic_normal,
            stars: StarParams {
                seed: s.seed,
                count: s.star_count,
                ..StarParams::default()
            },
            diffuse: s.diffuse_band.then(DiffuseBandParams::default),
        })
    }

    /// CPU renderer settings (`rtol` from the scene; `atol = rtol / 100` as in the reference defaults).
    ///
    /// # Errors
    /// [`RenderError::InvalidScene`] unless `0 < rtol ≤ 1e-3`.
    pub fn cpu_settings(&self) -> Result<CpuSettings, RenderError> {
        let rtol = self.render.rtol;
        if !(rtol > 0.0 && rtol <= 1e-3) {
            return Err(RenderError::InvalidScene(format!(
                "rtol must be in (0, 1e-3], got {rtol}"
            )));
        }
        let mut settings = CpuSettings::default();
        settings.trace.rtol = rtol;
        settings.trace.atol = rtol * 1e-2;
        Ok(settings)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_relative_eq;
    use gravography_physics::linalg::determinant;

    #[test]
    fn minimal_scene_uses_defaults() {
        let s = Scene::from_toml("[observer]\nr = 50.0\n").unwrap();
        assert_eq!(s.observer.r, 50.0);
        assert_eq!(s.observer.preset, PresetKind::Hover);
        assert_eq!(s.camera, CameraSpec::default());
        assert_eq!(s.render.rtol, 1e-10);
        let f = s.frame().unwrap();
        assert_relative_eq!(f.kerr.r(&f.position), 50.0, epsilon = 1e-12);
        let p = s.sky_params().unwrap();
        assert_eq!(p, SkyParams::default());
    }

    #[test]
    fn rejects_bad_input() {
        assert!(matches!(
            Scene::from_toml("[observer]\nradius = 5\n"),
            Err(RenderError::SceneParse(_))
        ));
        assert!(matches!(
            Scene::from_toml("spin = 0.5\n[observer]\n"),
            Err(RenderError::SceneParse(_))
        ));
        let mut s = Scene::default();
        s.spacetime.spin = 1.0;
        assert!(matches!(s.frame(), Err(RenderError::InvalidScene(_))));
        s.spacetime.spin = 0.9;
        s.observer.r = 1.8; // inside the ergosphere at the equator (r_E = 2)
        assert!(matches!(s.frame(), Err(RenderError::Observer(_))));
        let mut s = Scene::default();
        s.observer.boost = [0.8, 0.7, 0.0];
        assert!(matches!(s.frame(), Err(RenderError::InvalidScene(_))));
        let mut s = Scene::default();
        s.camera.width = 0;
        assert!(s.frame().is_err());
        s = Scene::default();
        s.sky.face_size = 1000;
        assert!(s.sky_params().is_err());
        s = Scene::default();
        s.render.rtol = 0.1;
        assert!(s.cpu_settings().is_err());
    }

    #[test]
    fn look_rotation_conventions() {
        // Yaw +90° turns forward into the old left; pitch +90° turns forward into up; roll keeps forward.
        let col0 = |m: [[f64; 3]; 3]| [m[0][0], m[1][0], m[2][0]];
        let yaw = LookSpec {
            yaw_deg: 90.0,
            ..LookSpec::default()
        }
        .matrix();
        let pitch = LookSpec {
            pitch_deg: 90.0,
            ..LookSpec::default()
        }
        .matrix();
        let roll = LookSpec {
            roll_deg: 35.0,
            ..LookSpec::default()
        }
        .matrix();
        for (got, expect) in [
            (col0(yaw), [0.0, 1.0, 0.0]),
            (col0(pitch), [0.0, 0.0, 1.0]),
            (col0(roll), [1.0, 0.0, 0.0]),
        ] {
            for k in 0..3 {
                assert_relative_eq!(got[k], expect[k], epsilon = 1e-15);
            }
        }
        // Proper rotations (det = +1) keep the tetrad right-handed.
        let any = LookSpec {
            yaw_deg: 20.0,
            pitch_deg: -30.0,
            roll_deg: 75.0,
        }
        .matrix();
        let m4 = [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, any[0][0], any[0][1], any[0][2]],
            [0.0, any[1][0], any[1][1], any[1][2]],
            [0.0, any[2][0], any[2][1], any[2][2]],
        ];
        assert_relative_eq!(determinant(&m4), 1.0, epsilon = 1e-14);
    }

    #[test]
    fn boosted_placement_releases_station_keeping() {
        let kerr = Kerr::new(0.5).unwrap();
        let params = ObserverParams::default();
        let spec = ObserverSpec::default();
        let still = spec.place_boosted(&kerr, &params).unwrap();
        assert!(matches!(still.motion, gravography_physics::Motion::Hover { .. }));
        assert_eq!(still, spec.place(&kerr, &params).unwrap());
        let moving = ObserverSpec {
            boost: [0.0, 0.3, 0.0],
            ..spec
        }
        .place_boosted(&kerr, &params)
        .unwrap();
        assert_eq!(moving.motion, gravography_physics::Motion::Free);
        assert_eq!(moving.pos, still.pos);
    }

    #[test]
    fn boost_and_look_are_applied() {
        let mut s = Scene::default();
        s.observer.boost = [0.5, 0.0, 0.0];
        s.observer.look.yaw_deg = 180.0;
        let f = s.frame().unwrap();
        let g = f.kerr.g_lo(&f.position);
        assert!(f.tetrad.orthonormality_error(&g) < 1e-13);
        let hover = s.observer().unwrap();
        let gamma = -gravography_physics::linalg::inner(&g, &hover.u(), &f.tetrad.u());
        assert_relative_eq!(gamma, 1.0 / 0.75_f64.sqrt(), epsilon = 1e-13);
        // Looking backwards: the new forward leg is minus the hover-frame forward leg (boosted).
        let boosted = hover.tetrad.boosted(&[0.5, 0.0, 0.0]).unwrap();
        for mu in 0..4 {
            assert_relative_eq!(f.tetrad.e_up[1][mu], -boosted.e_up[1][mu], epsilon = 1e-14);
        }
    }
}
