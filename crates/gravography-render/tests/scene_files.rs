//! FR-X3: scene schema round trip, example scenes, README documentation of the schema.

use std::path::{Path, PathBuf};

use gravography_physics::RayStatus;
use gravography_render::cpu::status_index;
use gravography_render::scene::{LookSpec, PresetKind};
use gravography_render::tonemap::Exposure;
use gravography_render::{CpuRenderer, Effects, Projection, Scene, Sky, ToneMap};
use gravography_sky::{BlackbodyLut, bake_sky};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn scene_files() -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(repo_root().join("scenes"))
        .expect("scenes/ directory exists")
        .map(|e| e.expect("readable directory entry").path())
        .filter(|p| p.extension().is_some_and(|e| e == "toml"))
        .collect();
    files.sort();
    files
}

/// A scene with every field set to a non-default value.
fn full_scene() -> Scene {
    let mut s = Scene::default();
    s.spacetime.spin = 0.7;
    s.observer.preset = PresetKind::CircularOrbit;
    s.observer.r = 12.5;
    s.observer.theta_deg = 80.0;
    s.observer.phi_deg = -30.0;
    s.observer.prograde = false;
    s.observer.boost = [0.1, -0.2, 0.05];
    s.observer.look = LookSpec {
        yaw_deg: 10.0,
        pitch_deg: -5.5,
        roll_deg: 2.0,
    };
    s.camera.width = 321;
    s.camera.height = 123;
    s.camera.projection = Projection::Rectilinear { fov_deg: 75.25 };
    s.camera.samples = 3;
    s.sky.seed = 99;
    s.sky.star_count = 12_345;
    s.sky.face_size = 256;
    s.sky.diffuse_band = false;
    s.sky.galactic_normal = [0.1, 0.2, 0.97];
    s.render.exposure = Exposure { auto: false, ev: 1.5 };
    s.render.tone_map = ToneMap::Reinhard;
    s.render.effects = Effects {
        lensing: false,
        frequency_shift: true,
        beaming: false,
    };
    s.render.rtol = 3e-9;
    s
}

#[test]
fn full_scene_round_trips() {
    let s = full_scene();
    let text = s.to_toml().unwrap();
    let back = Scene::from_toml(&text).unwrap();
    assert_eq!(back, s, "TOML:\n{text}");
    // Every field differs from the default, so a field that silently fell back to its default would fail here.
    assert_ne!(Scene::from_toml("[observer]\n").unwrap(), s);
    assert!(s.frame().is_ok());
}

#[test]
fn example_scenes_parse_round_trip_and_build() {
    let files = scene_files();
    assert!(
        files.len() >= 4,
        "expected the four WP-06 example scenes, found {files:?}"
    );
    for path in files {
        let scene = Scene::load(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let back = Scene::from_toml(&scene.to_toml().unwrap()).unwrap();
        assert_eq!(back, scene, "{}", path.display());
        scene
            .frame()
            .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        scene.sky_params().unwrap();
        scene.cpu_settings().unwrap();
    }
}

#[test]
fn readme_documents_every_scene_field() {
    // Every key of the serialised schema (sections and fields, including inline tables) must appear in README.md
    // in backticks, so the documentation cannot drift from the schema.
    let readme = std::fs::read_to_string(repo_root().join("README.md")).unwrap();
    let value: toml::Table = toml::from_str(&full_scene().to_toml().unwrap()).unwrap();
    fn keys(t: &toml::Table, out: &mut Vec<String>) {
        for (k, v) in t {
            out.push(k.clone());
            if let toml::Value::Table(inner) = v {
                keys(inner, out);
            }
        }
    }
    let mut all = Vec::new();
    keys(&value, &mut all);
    for k in all {
        assert!(
            readme.contains(&format!("`{k}`")),
            "README.md does not document scene key `{k}`"
        );
    }
    for v in [
        "hover",
        "zamo",
        "circular_orbit",
        "radial_infall",
        "rectilinear",
        "aces",
        "reinhard",
    ] {
        assert!(
            readme.contains(&format!("\"{v}\"")),
            "README.md does not document value \"{v}\""
        );
    }
}

#[test]
fn example_scenes_render_without_errors() {
    // Every example scene at thumbnail size with a small sky: no integration errors, no exhausted step budgets,
    // finite pixels, and some sky visible.
    let lut = BlackbodyLut::human_vision();
    for path in scene_files() {
        let mut scene = Scene::load(&path).unwrap();
        scene.camera.width = 32;
        scene.camera.height = 18;
        scene.camera.samples = 1;
        scene.sky.face_size = 32;
        scene.sky.star_count = 2000;
        let frame = scene.frame().unwrap();
        let (_, cube) = bake_sky(&scene.sky_params().unwrap(), &lut).unwrap();
        let sky = Sky::new(cube, lut.clone());
        let (img, stats) = CpuRenderer::new(&frame, &sky, scene.cpu_settings().unwrap())
            .unwrap()
            .render();
        let name = path.display();
        for s in [RayStatus::Error, RayStatus::MaxSteps, RayStatus::Singular] {
            assert_eq!(stats.status[status_index(s)], 0, "{name}: {s:?}");
        }
        assert!(
            stats.status[status_index(RayStatus::Escaped)] > 0,
            "{name}: nothing escaped"
        );
        assert!(
            img.pixels.iter().flatten().all(|c| c.is_finite()),
            "{name}: non-finite pixel"
        );
    }
}
