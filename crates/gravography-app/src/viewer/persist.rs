//! Bookmarks (`docs/PRD.md` FR-O6) and camera paths (FR-O7), stored as RON files.
//!
//! A bookmark holds the complete state needed to reproduce a view exactly: the observer's event, tetrad, proper
//! time and motion mode, the look rotation, the flight and view settings, the spin, the last preset and the sky
//! description. RON writes `f64` in shortest round-trip form, so loading restores every number bit for bit.
//!
//! A camera path is a sequence of observer states (with look rotation) recorded at fixed proper-time intervals
//! (the integration is split exactly at each mark, [`Flight::update_marked`]). Playback steps through them at a
//! chosen rate of proper time per wall-clock second. This is the foundation for offline video (v2).

use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use gravography_physics::{Kerr, Motion, Observer, Tetrad};
use gravography_render::scene::{LookSpec, MAX_SPIN, ObserverSpec, SkySpec};
use serde::{Deserialize, Serialize};

use super::flight::{Flight, FlightSettings, FrameUpdate};
use super::input::ThrustKeys;
use super::state::ViewSettings;

/// Version written into every file; files with another version are refused.
pub const FORMAT_VERSION: u32 = 1;

/// Serialisable mirror of [`Motion`].
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum MotionState {
    /// Geodesic motion plus thrust.
    Free,
    /// Static observer on the orbit of cylindrical radius `rho` at height `z`.
    Hover {
        /// Cylindrical radius.
        rho: f64,
        /// Height.
        z: f64,
    },
    /// ZAMO on the orbit of cylindrical radius `rho` at height `z`.
    Zamo {
        /// Cylindrical radius.
        rho: f64,
        /// Height.
        z: f64,
    },
}

impl From<Motion<f64>> for MotionState {
    fn from(m: Motion<f64>) -> Self {
        match m {
            Motion::Free => Self::Free,
            Motion::Hover { rho, z } => Self::Hover { rho, z },
            Motion::Zamo { rho, z } => Self::Zamo { rho, z },
        }
    }
}

impl From<MotionState> for Motion<f64> {
    fn from(m: MotionState) -> Self {
        match m {
            MotionState::Free => Self::Free,
            MotionState::Hover { rho, z } => Self::Hover { rho, z },
            MotionState::Zamo { rho, z } => Self::Zamo { rho, z },
        }
    }
}

/// The observer's dynamical state (KS event, tetrad `e_up[a][μ]`, proper time `τ`, motion mode).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct ObserverState {
    /// KS event `[t, x, y, z]`.
    pub pos: [f64; 4],
    /// Body tetrad legs (u, forward, left, up), contravariant KS components.
    pub tetrad: [[f64; 4]; 4],
    /// Proper time `τ`, `M`.
    pub tau: f64,
    /// Motion mode.
    pub motion: MotionState,
}

impl ObserverState {
    /// The state of `obs`.
    pub fn of(obs: &Observer<f64>) -> Self {
        Self {
            pos: obs.pos,
            tetrad: obs.tetrad.e_up,
            tau: obs.tau,
            motion: obs.motion.into(),
        }
    }

    /// Rebuilds the observer in `kerr` (validated by [`Observer::from_state`]).
    ///
    /// # Errors
    /// The state is not valid in this spacetime.
    pub fn restore(&self, kerr: &Kerr<f64>, flight: &Flight) -> Result<Observer<f64>> {
        Ok(Observer::from_state(
            kerr,
            &self.pos,
            &Tetrad { e_up: self.tetrad },
            self.tau,
            self.motion.into(),
            flight.params(),
        )?)
    }
}

fn kerr_for(spin: f64) -> Result<Kerr<f64>> {
    if !(0.0..=MAX_SPIN).contains(&spin) {
        bail!("spin {spin} is outside [0, {MAX_SPIN}]");
    }
    Ok(Kerr::new(spin)?)
}

/// A saved view (FR-O6).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Bookmark {
    /// [`FORMAT_VERSION`].
    pub version: u32,
    /// Spin `a/M`.
    pub spin: f64,
    /// Observer state.
    pub observer: ObserverState,
    /// Look rotation.
    pub look: LookSpec,
    /// Last applied preset (target of resets).
    pub preset: ObserverSpec,
    /// Flight settings (time warp, pause, thrust, mass).
    pub flight: FlightSettings,
    /// Render settings (projection, effects, exposure, …).
    pub view: ViewSettings,
    /// Sky description, to warn when the bookmark is loaded under a different sky.
    pub sky: SkySpec,
}

impl Bookmark {
    /// Captures the current state.
    pub fn capture(flight: &Flight, view: &ViewSettings, sky: &SkySpec) -> Self {
        Self {
            version: FORMAT_VERSION,
            spin: flight.kerr.spin(),
            observer: ObserverState::of(&flight.observer),
            look: flight.look,
            preset: *flight.preset(),
            flight: flight.settings,
            view: *view,
            sky: sky.clone(),
        }
    }

    /// Restores the state into `flight` and `view`. Returns a warning if the sky differs from `current_sky`
    /// (the view is then reproduced with a different star field).
    ///
    /// # Errors
    /// Wrong version, invalid spin or an observer state that is not valid in that spacetime (nothing changes).
    pub fn apply(
        &self,
        flight: &mut Flight,
        view: &mut ViewSettings,
        current_sky: &SkySpec,
    ) -> Result<Option<String>> {
        if self.version != FORMAT_VERSION {
            bail!(
                "bookmark format version {} (expected {FORMAT_VERSION})",
                self.version
            );
        }
        let kerr = kerr_for(self.spin)?;
        let obs = self.observer.restore(&kerr, flight)?;
        flight.restore(kerr, obs, self.look, self.preset, self.flight);
        *view = self.view;
        Ok((self.sky != *current_sky).then(|| {
            "the bookmark was saved with a different sky (star field); open its scene to reproduce it exactly".into()
        }))
    }
}

/// One recorded camera state.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct PathSample {
    /// Observer state at the mark.
    pub observer: ObserverState,
    /// Look rotation at the mark.
    pub look: LookSpec,
}

/// A recorded camera path (FR-O7): observer states every `interval` of proper time.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CameraPath {
    /// [`FORMAT_VERSION`].
    pub version: u32,
    /// Spin `a/M` (constant along the path; changing the spin ends a recording).
    pub spin: f64,
    /// Proper time between samples, `M`.
    pub interval: f64,
    /// Samples at `τ₀, τ₀ + interval, …`.
    pub samples: Vec<PathSample>,
    /// Render settings when recording started.
    pub view: ViewSettings,
    /// Sky description.
    pub sky: SkySpec,
}

/// Records a [`CameraPath`] while flying.
#[derive(Clone, Debug)]
pub struct Recorder {
    path: CameraPath,
    /// Proper time of the next sample.
    pub next_mark: f64,
}

impl Recorder {
    /// Starts recording at the current state (sample 0) with `interval` (> 0) between samples.
    pub fn start(flight: &Flight, interval: f64, view: &ViewSettings, sky: &SkySpec) -> Self {
        let interval = if interval > 0.0 && interval.is_finite() {
            interval
        } else {
            1.0
        };
        let mut r = Self {
            path: CameraPath {
                version: FORMAT_VERSION,
                spin: flight.kerr.spin(),
                interval,
                samples: Vec::new(),
                view: *view,
                sky: sky.clone(),
            },
            next_mark: flight.observer.tau + interval,
        };
        r.record(flight);
        r
    }

    /// Appends the current state (called at each mark).
    pub fn record(&mut self, flight: &Flight) {
        self.path.samples.push(PathSample {
            observer: ObserverState::of(&flight.observer),
            look: flight.look,
        });
    }

    /// One frame of flight ([`Flight::update_marked`]) that records a sample at every mark it crosses.
    pub fn update(&mut self, flight: &mut Flight, dt_wall: f64, keys: &ThrustKeys) -> FrameUpdate {
        let mut marks = Vec::new();
        let interval = self.path.interval;
        let up = flight.update_marked(dt_wall, keys, &mut self.next_mark, interval, |g| {
            marks.push(PathSample {
                observer: ObserverState::of(&g.observer),
                look: g.look,
            });
        });
        self.path.samples.extend(marks);
        up
    }

    /// Proper time between samples.
    pub fn interval(&self) -> f64 {
        self.path.interval
    }

    /// Samples so far.
    pub fn len(&self) -> usize {
        self.path.samples.len()
    }

    /// Whether the recording can continue in `flight` (same spin).
    pub fn compatible(&self, flight: &Flight) -> bool {
        self.path.spin.to_bits() == flight.kerr.spin().to_bits()
    }

    /// The recorded path.
    pub fn finish(self) -> CameraPath {
        self.path
    }
}

/// Plays a [`CameraPath`] back: sample `k` is shown while `k · interval ≤ elapsed proper time < (k+1) · interval`.
#[derive(Clone, Debug)]
pub struct Playback {
    path: CameraPath,
    kerr: Kerr<f64>,
    /// Proper time per wall-clock second, `M/s`.
    pub speed: f64,
    /// Restart at the end instead of stopping.
    pub looping: bool,
    elapsed: f64,
}

impl Playback {
    /// Starts playback of `path`.
    ///
    /// # Errors
    /// Wrong version, invalid spin, or an empty path.
    pub fn new(path: CameraPath, speed: f64) -> Result<Self> {
        if path.version != FORMAT_VERSION {
            bail!(
                "camera path format version {} (expected {FORMAT_VERSION})",
                path.version
            );
        }
        if path.samples.is_empty() {
            bail!("the camera path has no samples");
        }
        Ok(Self {
            kerr: kerr_for(path.spin)?,
            path,
            speed,
            looping: false,
            elapsed: 0.0,
        })
    }

    /// The path's spacetime.
    pub fn kerr(&self) -> Kerr<f64> {
        self.kerr
    }

    /// The path.
    pub fn path(&self) -> &CameraPath {
        &self.path
    }

    /// Index of the sample to show now.
    pub fn index(&self) -> usize {
        let k = (self.elapsed / self.path.interval).floor();
        let n = self.path.samples.len();
        if k.is_nan() || k < 0.0 {
            0
        } else if self.looping {
            (k as usize) % n
        } else {
            (k as usize).min(n - 1)
        }
    }

    /// The sample to show now.
    pub fn current(&self) -> &PathSample {
        &self.path.samples[self.index()]
    }

    /// Advances by `dt_wall` seconds; returns `true` while playing (always while looping).
    pub fn advance(&mut self, dt_wall: f64) -> bool {
        if dt_wall > 0.0 && self.speed.is_finite() {
            self.elapsed += dt_wall * self.speed.max(0.0);
        }
        self.looping || self.elapsed < self.path.interval * self.path.samples.len() as f64
    }
}

/// Serialises `value` as pretty RON and writes it to `path` (creating the directory).
///
/// # Errors
/// I/O or serialisation errors.
pub fn save_ron<T: Serialize>(value: &T, path: &Path) -> Result<()> {
    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    }
    let text = ron::ser::to_string_pretty(value, ron::ser::PrettyConfig::default())?;
    std::fs::write(path, text).with_context(|| format!("writing {}", path.display()))
}

/// Reads a RON file.
///
/// # Errors
/// I/O or parse errors.
pub fn load_ron<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T> {
    let text = std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
    ron::from_str(&text).with_context(|| format!("parsing {}", path.display()))
}

/// `*.ron` files in `dir`, sorted by name (empty if the directory does not exist).
pub fn list_ron(dir: &Path) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = std::fs::read_dir(dir)
        .map(|it| {
            it.filter_map(|e| e.ok().map(|e| e.path()))
                .filter(|p| p.extension().is_some_and(|x| x == "ron"))
                .collect()
        })
        .unwrap_or_default();
    out.sort();
    out
}

/// A file name for user text: keeps letters, digits, `-` and `_` (others become `_`); empty → `fallback`.
pub fn file_stem(name: &str, fallback: &str) -> String {
    let s: String = name
        .trim()
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if s.is_empty() { fallback.to_string() } else { s }
}

#[cfg(test)]
mod tests {
    use super::super::flight::safe_preset;
    use super::super::input::{ThrustKey, ThrustKeys};
    use super::*;
    use gravography_render::scene::PresetKind;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("gravography-test-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        dir
    }

    fn flying() -> Flight {
        // Kerr, free flight after some thrust and a look rotation: a state no preset reproduces.
        let mut f = Flight::new(Kerr::new(0.6).unwrap(), &safe_preset()).unwrap();
        f.look_by(23.5, -7.25);
        let mut k = ThrustKeys::default();
        k.set(ThrustKey::Forward, true);
        k.set(ThrustKey::Up, true);
        f.settings.time_warp = 20.0;
        for _ in 0..30 {
            f.update(1.0 / 60.0, &k);
        }
        f
    }

    #[test]
    fn bookmarks_round_trip_bit_for_bit() {
        // FR-O6: save to RON, load, and the camera (event + tetrad with look) is identical bit for bit, as are all
        // settings, so the renderer receives exactly the same frame (NF-3 then gives the same image).
        let f = flying();
        let mut view = ViewSettings::from_scene(&gravography_render::Scene::default());
        view.projection = gravography_render::Projection::Equirectangular;
        view.exposure.ev = -1.5;
        let sky = SkySpec::default();
        let b = Bookmark::capture(&f, &view, &sky);
        let path = scratch("bm").join("view.ron");
        save_ron(&b, &path).unwrap();
        let loaded: Bookmark = load_ron(&path).unwrap();
        assert_eq!(loaded, b);
        let mut g = Flight::new(Kerr::new(0.0).unwrap(), &safe_preset()).unwrap();
        let mut v2 = ViewSettings::from_scene(&gravography_render::Scene::default());
        assert_eq!(loaded.apply(&mut g, &mut v2, &sky).unwrap(), None);
        let (ca, cb) = (f.camera(), g.camera());
        assert_eq!(ca.kerr, cb.kerr);
        assert_eq!(ca.position.map(f64::to_bits), cb.position.map(f64::to_bits));
        assert_eq!(
            ca.tetrad.e_up.map(|l| l.map(f64::to_bits)),
            cb.tetrad.e_up.map(|l| l.map(f64::to_bits))
        );
        assert_eq!(
            (g.observer.tau, g.observer.motion, g.look),
            (f.observer.tau, f.observer.motion, f.look)
        );
        assert_eq!((g.settings, *g.preset(), v2), (f.settings, *f.preset(), view));
        // A different sky is reported; a corrupt or foreign file is refused without changing the state.
        let other = SkySpec {
            seed: 7,
            ..sky.clone()
        };
        assert!(loaded.apply(&mut g, &mut v2, &other).unwrap().is_some());
        let mut bad = b.clone();
        bad.version = 99;
        assert!(bad.apply(&mut g, &mut v2, &sky).is_err());
        let mut bad = b.clone();
        bad.spin = 0.0; // the tetrad is not orthonormal for a = 0
        let before = g.camera();
        assert!(bad.apply(&mut g, &mut v2, &sky).is_err());
        assert_eq!(g.camera(), before);
        std::fs::write(&path, "not ron").unwrap();
        assert!(load_ron::<Bookmark>(&path).is_err());
        assert_eq!(list_ron(path.parent().unwrap()), vec![path.clone()]);
        let _ = std::fs::remove_dir_all(path.parent().unwrap());
    }

    #[test]
    fn recorded_paths_have_fixed_intervals_and_play_back() {
        // FR-O7: radial infall recorded every 0.5 M for ~3 M of proper time; samples sit exactly at τ₀ + k·0.5;
        // the RON round trip is exact; playback at 2 M/s shows sample k at wall time k/4 s and stops at the end.
        let spec = ObserverSpec {
            preset: PresetKind::RadialInfall,
            r: 12.0,
            ..safe_preset()
        };
        let mut f = Flight::new(Kerr::new(0.3).unwrap(), &spec).unwrap();
        f.settings.time_warp = 6.0;
        let view = ViewSettings::from_scene(&gravography_render::Scene::default());
        let mut rec = Recorder::start(&f, 0.5, &view, &SkySpec::default());
        for _ in 0..30 {
            rec.update(&mut f, 1.0 / 60.0, &ThrustKeys::default());
        }
        assert!(rec.compatible(&f) && rec.len() == 7);
        let path = rec.finish();
        assert_eq!(path.samples.len(), 7, "τ from 0 to 3: marks 0, 0.5, …, 3.0");
        for (k, s) in path.samples.iter().enumerate() {
            assert_eq!(s.observer.tau, 0.5 * k as f64);
        }
        // Falling inward monotonically.
        let r = |s: &PathSample| Kerr::new(0.3).unwrap().r(&s.observer.pos);
        assert!(path.samples.windows(2).all(|w| r(&w[1]) < r(&w[0])));
        let file = scratch("path").join("fall.ron");
        save_ron(&path, &file).unwrap();
        let loaded: CameraPath = load_ron(&file).unwrap();
        assert_eq!(loaded, path);
        let mut p = Playback::new(loaded, 2.0).unwrap();
        assert_eq!(p.path().samples.len(), 7);
        assert_eq!(p.index(), 0);
        assert!(p.advance(0.26));
        assert_eq!(p.index(), 1);
        assert!(p.current().observer.restore(&p.kerr(), &f).is_ok());
        assert!(!p.advance(10.0), "stops after the last sample");
        assert_eq!(p.index(), 6);
        p.looping = true;
        assert!(p.advance(0.0));
        assert!(
            Playback::new(
                CameraPath {
                    samples: vec![],
                    ..path.clone()
                },
                1.0
            )
            .is_err()
        );
        let _ = std::fs::remove_dir_all(file.parent().unwrap());
    }

    #[test]
    fn file_stems_are_safe() {
        assert_eq!(file_stem("  my view/1 ", "x"), "my_view_1");
        assert_eq!(file_stem("", "bookmark"), "bookmark");
        assert_eq!(file_stem("../etc", "x"), "___etc");
    }
}
