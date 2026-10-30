//! Renders the baked sky (no black hole) to an equirectangular image for visual inspection.
//!
//! `cargo run --release -p gravography-sky --example sky_preview -- [out.ppm] [face_size] [exposure]`
//! writes a binary PPM (convert with e.g. `sips -s format png out.ppm --out out.png` on macOS). Each pixel samples
//! the cube map with a level of detail matching its solid angle, shades `w · XYZ(T)`, converts to sRGB and applies
//! a simple exposure + Reinhard tone curve.

use std::io::Write;

use gravography_sky::colour::{srgb_encode, xyz_to_linear_srgb};
use gravography_sky::{BlackbodyLut, SkyParams, bake_sky};

fn main() -> std::io::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let out = args
        .get(1)
        .cloned()
        .unwrap_or_else(|| "target/screenshots/sky_preview.ppm".into());
    let face: usize = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(1024);
    let exposure: f64 = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(1.0);
    let lut = BlackbodyLut::human_vision();
    let params = SkyParams {
        face_size: face,
        ..SkyParams::default()
    };
    let (_, cube) = bake_sky(&params, &lut).map_err(std::io::Error::other)?;
    let (w, h) = (2048_usize, 1024_usize);
    // Pixel solid angle near the equator ≈ (2π/w)²; texel solid angle ≈ 4π / (6 n²) ⇒ lod = ½ log2(Ω_px / Ω_texel).
    let lod = 0.5
        * ((2.0 * std::f64::consts::PI / w as f64).powi(2)
            / (4.0 * std::f64::consts::PI / (6.0 * (face * face) as f64)))
            .log2();
    // Normalise brightness by the mean Y of the sky.
    let mut px = vec![[0.0_f64; 3]; w * h];
    let mut mean_y = 0.0;
    for j in 0..h {
        let theta = std::f64::consts::PI * (j as f64 + 0.5) / h as f64;
        for i in 0..w {
            let phi = 2.0 * std::f64::consts::PI * (i as f64 + 0.5) / w as f64;
            let d = [theta.sin() * phi.cos(), theta.sin() * phi.sin(), theta.cos()];
            let [wf, wt] = cube.sample_trilinear(&d, lod.max(0.0));
            let xyz = if wf > 0.0 {
                lut.lookup(wt / wf).response.map(|c| c * wf)
            } else {
                [0.0; 3]
            };
            mean_y += xyz[1] / (w * h) as f64;
            px[j * w + i] = xyz;
        }
    }
    if let Some(parent) = std::path::Path::new(&out).parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut f = std::io::BufWriter::new(std::fs::File::create(&out)?);
    write!(f, "P6\n{w} {h}\n255\n")?;
    for xyz in px {
        let rgb = xyz_to_linear_srgb(&xyz.map(|c| c * exposure * 0.18 / mean_y));
        let bytes = rgb.map(|c| {
            let c = c.max(0.0);
            (srgb_encode(c / (1.0 + c)) * 255.0).round() as u8
        });
        f.write_all(&bytes)?;
    }
    println!("wrote {out} (lod {lod:.2}, mean Y {mean_y:.3e})");
    Ok(())
}
