//! Baking the sky (catalogue + diffuse band) into a mip-mapped cube map (`docs/PRD.md` §6.3; `docs/physics.md`
//! § "Sky cube map").
//!
//! Each star is splatted with a bilinear (tent) kernel over the 2×2 texels around its position, with weights `kᵢ`
//! summing to 1: `wᵢ += kᵢ Ω_* / Ω_texel,ᵢ` and `(wT)ᵢ += kᵢ Ω_* T / Ω_texel,ᵢ`. So `Σ w Ω` gains exactly `Ω_*`
//! (flux conservation), and the kernel is ≈ 1–2 texels wide (anti-aliasing). Kernel taps are clamped at face
//! edges rather than wrapped onto the neighbouring face, which shifts stars by < 1 texel there but keeps the flux.

use rayon::prelude::*;

use crate::catalogue::{Star, StarParams, generate};
use crate::cubemap::{CubeLevel, CubeMap, FACE_COUNT, dir_to_face_uv, face_uv_to_dir, texel_solid_angle};
use crate::diffuse::DiffuseBandParams;
use crate::{BlackbodyLut, SkyError};

/// Everything that determines the baked sky.
#[derive(Clone, Debug, PartialEq)]
pub struct SkyParams {
    /// Face size of mip level 0 (power of two). PRD §6.3 default: 2048 (≈ 270 MB with mips as RG32F).
    pub face_size: usize,
    /// Pole of the galactic plane (KS Cartesian axes; need not be normalised).
    pub galactic_normal: [f64; 3],
    /// Star catalogue.
    pub stars: StarParams,
    /// Diffuse band (`None` disables it).
    pub diffuse: Option<DiffuseBandParams>,
}

impl Default for SkyParams {
    /// 2048² faces; the galactic pole is tilted 60° from the spin axis so the band crosses the sky obliquely.
    fn default() -> Self {
        Self {
            face_size: 2048,
            galactic_normal: [0.0, 0.866_025_403_784_438_6, 0.5],
            stars: StarParams::default(),
            diffuse: Some(DiffuseBandParams::default()),
        }
    }
}

/// Splats one star into `level` (see module docs).
pub fn splat_star(level: &mut CubeLevel, star: &Star) {
    let n = level.size;
    let (face, u, v) = dir_to_face_uv(&star.dir);
    let max = (n - 1) as f64;
    let s = (u * n as f64 - 0.5).clamp(0.0, max);
    let t = (v * n as f64 - 0.5).clamp(0.0, max);
    let (c0, r0) = (s.floor() as usize, t.floor() as usize);
    let (c1, r1) = ((c0 + 1).min(n - 1), (r0 + 1).min(n - 1));
    let (fs, ft) = (s - c0 as f64, t - r0 as f64);
    for (c, r, k) in [
        (c0, r0, (1.0 - fs) * (1.0 - ft)),
        (c1, r0, fs * (1.0 - ft)),
        (c0, r1, (1.0 - fs) * ft),
        (c1, r1, fs * ft),
    ] {
        if k <= 0.0 {
            continue;
        }
        let w = k * star.solid_angle / texel_solid_angle(n, c, r);
        let i = level.index(face, c, r);
        level.data[i][0] += w as f32;
        level.data[i][1] += (w * star.temperature) as f32;
    }
}

/// Bakes `stars` and the optional diffuse band into a cube map with mips.
///
/// # Errors
/// [`SkyError::InvalidParameters`] if `params.face_size` is not a power of two.
pub fn bake(params: &SkyParams, stars: &[Star]) -> Result<CubeMap, SkyError> {
    let n = params.face_size;
    if !n.is_power_of_two() {
        return Err(SkyError::InvalidParameters(
            "cube-map face size must be a power of two",
        ));
    }
    let mut level = CubeLevel::zeros(n);
    if let Some(band) = &params.diffuse {
        // Point-sampled at texel centres (smooth on the texel scale); rows in parallel, deterministic.
        level.data.par_chunks_mut(n).enumerate().for_each(|(fr, row)| {
            let (face, r) = (fr / n, fr % n);
            for (c, texel) in row.iter_mut().enumerate() {
                let d = face_uv_to_dir(face, (c as f64 + 0.5) / n as f64, (r as f64 + 0.5) / n as f64);
                let w = band.filling_factor(&d, &params.galactic_normal);
                *texel = [w as f32, (w * band.temperature) as f32];
            }
        });
    }
    debug_assert_eq!(level.data.len(), FACE_COUNT * n * n);
    // Sequential splatting keeps the f32 accumulation order (and so the result) deterministic.
    for star in stars {
        splat_star(&mut level, star);
    }
    Ok(CubeMap::with_mips(level))
}

/// Generates the catalogue for `params` (using `lut` for flux → solid angle) and bakes it.
///
/// # Errors
/// See [`bake`].
pub fn bake_sky(params: &SkyParams, lut: &BlackbodyLut) -> Result<(Vec<Star>, CubeMap), SkyError> {
    let stars = generate(&params.stars, lut, &params.galactic_normal);
    let cube = bake(params, &stars)?;
    Ok((stars, cube))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cubemap::texel_solid_angle;

    fn small(n: usize, count: usize, diffuse: bool) -> SkyParams {
        SkyParams {
            face_size: n,
            stars: StarParams {
                count,
                ..StarParams::default()
            },
            diffuse: diffuse.then(DiffuseBandParams::default),
            ..SkyParams::default()
        }
    }

    #[test]
    fn bake_conserves_star_flux() {
        // WP-05 acceptance: Σ w Ω over the cube equals Σ Ω_* (and Σ w T Ω = Σ Ω_* T) at every mip level. f32 texels
        // accumulate ~10 stars each; relative 1e-5 covers the rounding.
        let lut = BlackbodyLut::human_vision();
        let (stars, cube) = bake_sky(&small(64, 20_000, false), &lut).unwrap();
        let omega: f64 = stars.iter().map(|s| s.solid_angle).sum();
        let omega_t: f64 = stars.iter().map(|s| s.solid_angle * s.temperature).sum();
        for l in &cube.levels {
            let [w, wt] = l.totals();
            assert!((w / omega - 1.0).abs() < 1e-5, "size {}: {w} vs {omega}", l.size);
            assert!((wt / omega_t - 1.0).abs() < 1e-5);
        }
    }

    #[test]
    fn bake_with_diffuse_band_adds_its_integral() {
        let lut = BlackbodyLut::human_vision();
        let p = small(32, 2000, true);
        let (stars, cube) = bake_sky(&p, &lut).unwrap();
        let band = p.diffuse.as_ref().unwrap();
        let n = p.face_size;
        let mut band_flux = 0.0;
        for face in 0..6 {
            for r in 0..n {
                for c in 0..n {
                    let d = face_uv_to_dir(face, (c as f64 + 0.5) / n as f64, (r as f64 + 0.5) / n as f64);
                    band_flux += band.filling_factor(&d, &p.galactic_normal) * texel_solid_angle(n, c, r);
                }
            }
        }
        let star_flux: f64 = stars.iter().map(|s| s.solid_angle).sum();
        let [w, _] = cube.levels[0].totals();
        assert!((w / (band_flux + star_flux) - 1.0).abs() < 1e-5);
        assert!(band_flux > 0.0);
    }

    #[test]
    fn single_star_keeps_its_temperature_and_position() {
        let p = small(16, 0, false);
        let star = Star {
            dir: crate::catalogue::normalise(&[0.2, -0.3, 1.0]),
            temperature: 7000.0,
            solid_angle: 1e-6,
        };
        let cube = bake(&p, &[star]).unwrap();
        for l in &cube.levels {
            for t in &l.data {
                if t[0] > 0.0 {
                    assert!((t[1] / t[0] - 7000.0).abs() < 7000.0 * 1e-6);
                }
            }
        }
        // The brightest texel of level 0 contains the star's direction.
        let l0 = &cube.levels[0];
        let (imax, _) = l0
            .data
            .iter()
            .enumerate()
            .fold((0, 0.0), |m, (i, t)| if t[0] > m.1 { (i, t[0]) } else { m });
        let (face, u, v) = dir_to_face_uv(&star.dir);
        let near = l0.index(face, (u * 16.0) as usize, (v * 16.0) as usize);
        assert_eq!(imax, near);
    }

    #[test]
    fn bake_is_deterministic_and_validates_size() {
        let lut = BlackbodyLut::human_vision();
        let a = bake_sky(&small(16, 500, true), &lut).unwrap().1;
        let b = bake_sky(&small(16, 500, true), &lut).unwrap().1;
        assert_eq!(a, b);
        assert!(bake(&small(24, 0, false), &[]).is_err());
    }
}
