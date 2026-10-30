//! Texture-sky mode (`docs/PRD.md` §6.5, WP-13): loading equirectangular radiance images and rendering with them.

use gravography_render::output::load_equirect;
use gravography_render::tonemap::{Exposure, luminance};
use gravography_render::{CpuRenderer, CpuSettings, HdrImage, Scene, Sky, ToneMap, write_image};
use gravography_sky::colour::xyz_to_linear_srgb;
use gravography_sky::{Band, BlackbodyLut, cube_from_equirect};

fn scratch(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("gravography-texture-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temporary directory is writable");
    dir
}

/// A `w × h` equirectangular image filled with the sRGB colour of a `t` kelvin blackbody scaled by `k`.
fn blackbody_image(w: u32, h: u32, t: f64, k: f64) -> HdrImage {
    let rgb = xyz_to_linear_srgb(&Band::human_vision().blackbody_response(t)).map(|c| (c * k) as f32);
    HdrImage {
        width: w,
        height: h,
        pixels: vec![rgb; (w * h) as usize],
    }
}

#[test]
fn exr_round_trip_and_refusals() {
    let dir = scratch("io");
    let mut img = blackbody_image(64, 32, 4000.0, 1e-6);
    img.pixels[5 * 64 + 7] = [3.0, 0.5, 0.25];
    let path = dir.join("sky.exr");
    // write_image stores scale · rgb in EXR (scale 1: the radiance itself).
    write_image(&path, &img, 1.0, ToneMap::Aces).unwrap();
    let back = load_equirect(&path).unwrap();
    assert_eq!((back.width, back.height), (64, 32));
    assert_eq!(back.pixels[5 * 64 + 7], [3.0, 0.5, 0.25]);
    assert_eq!(back.pixels[0], img.pixels[0]);
    // 8-bit, display-encoded formats are not radiance.
    assert!(load_equirect(&dir.join("sky.png")).is_err());
    assert!(load_equirect(&dir.join("missing.exr")).is_err());
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn uniform_texture_renders_as_blueshifted_blackbody() {
    // A uniform 4000 K texture seen by a static observer at r = 10 (Schwarzschild): every escaped pixel shows
    // w · B(g T) with g = (1 − 2/r)^{−1/2} ≈ 1.118 (PRD §9 test 10), i.e. the texture's luminance times
    // Y(gT)/Y(T). Tolerance: the locus fit (≤ 1.5 % in T; Y(T) ∝ T^≈3 near 4000 K → ≤ 5 % in Y) dominates; 6 %.
    let dir = scratch("render");
    let k = 1e-6;
    let path = dir.join("uniform.exr");
    write_image(&path, &blackbody_image(256, 128, 4000.0, k), 1.0, ToneMap::Aces).unwrap();
    let mut scene = Scene::from_toml(&format!(
        "[observer]\nr = 10.0\n[camera]\nwidth = 48\nheight = 27\n[sky]\nface_size = 64\ntexture = {:?}\n",
        path.to_str().unwrap()
    ))
    .unwrap();
    scene.render.exposure = Exposure { auto: false, ev: 0.0 };
    let lut = BlackbodyLut::human_vision();
    let img = load_equirect(&path).unwrap();
    let sky = Sky::new(
        cube_from_equirect(&img, scene.sky.face_size, &lut),
        scene.sky.band.lut(),
    );
    let frame = scene.frame().unwrap();
    let (out, stats) = CpuRenderer::new(&frame, &sky, CpuSettings::default())
        .unwrap()
        .render();
    assert!(stats.status[0] > 0);
    let band = Band::human_vision();
    let g = (1.0 - 2.0 / 10.0_f64).powf(-0.5);
    let y_tex = band.blackbody_response(4000.0)[1] * k;
    let expect = y_tex * band.blackbody_response(4000.0 * g)[1] / band.blackbody_response(4000.0)[1];
    let mut lit = 0;
    for p in &out.pixels {
        let y = luminance(&p.map(f64::from));
        if y > 0.0 {
            lit += 1;
            assert!((y / expect - 1.0).abs() < 0.06, "Y {y} vs expected {expect}");
        }
    }
    // At r = 10 the shadow (angular radius ≈ 28°) covers about a third of the 60° view of 48 × 27 pixels.
    assert!(lit > 600, "{lit} lit pixels");
    // The round trip through TOML keeps the texture path.
    let again = Scene::from_toml(&scene.to_toml().unwrap()).unwrap();
    assert_eq!(again.sky.texture, scene.sky.texture);
    let _ = std::fs::remove_dir_all(dir);
}
