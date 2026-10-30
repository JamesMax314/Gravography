//! Texture-sky mode (`docs/PRD.md` §6.5, approximate): an equirectangular HDR image becomes the same `(w, w·T)`
//! cube map as the procedural sky, by treating every pixel as a blackbody fitted to its chromaticity.
//!
//! A linear-sRGB pixel is converted to XYZ and chromaticity `(x, y)`. Its temperature `T` is the nearest point
//! of the Planckian locus (sampled on a log grid over [`FIT_RANGE`]), and its weight `w = Y / Y_bb(T)`
//! reproduces its luminance (`I = w B(T)`). The renderer then shows `w B(gT)`. The hue moves along the
//! Planckian locus and the bolometric brightness scales as `g⁴` (`∫ B(gT) ∝ g⁴ T⁴`), so the PRD's "g⁴ intensity
//! scaling" and "hue shift" both follow from the representation. The approximation: colours off the locus (greens,
//! purples) are replaced by the nearest blackbody colour, and spectral detail is lost. Pixel values are relative,
//! so only auto exposure is meaningful.
//!
//! Orientation: image centre = `+x`, longitude increases to the left (`u = 0.25` is `+y`), top row = `+z`.
//! That matches the equirectangular camera of a camera looking along `+x` with `+z` up.

use rayon::prelude::*;

use crate::band::{Band, BlackbodyLut};
use crate::colour::{D65_WHITE, Mat3, SRGB_PRIMARIES, chromaticity, mat3_vec, rgb_to_xyz_matrix};
use crate::cubemap::{CubeLevel, CubeMap, face_uv_to_dir};

/// Temperature range of the blackbody fit (K).
pub const FIT_RANGE: (f64, f64) = (1000.0, 40_000.0);
/// Number of Planckian-locus samples searched by the fit (log-spaced over [`FIT_RANGE`]; adjacent samples are
/// ≈ 1.5 % apart in temperature).
pub const LOCUS_SAMPLES: usize = 256;

/// An equirectangular image of linear sRGB radiance (row 0 at the top).
#[derive(Clone, Debug, PartialEq)]
pub struct EquirectImage {
    /// Width in pixels (longitude).
    pub width: usize,
    /// Height in pixels (latitude).
    pub height: usize,
    /// Row-major linear sRGB.
    pub pixels: Vec<[f32; 3]>,
}

impl EquirectImage {
    /// Bilinear sample at direction `d` (unit), wrapping in longitude and clamping in latitude.
    pub fn sample(&self, d: &[f64; 3]) -> [f64; 3] {
        let lon = d[1].atan2(d[0]);
        let lat = d[2].clamp(-1.0, 1.0).asin();
        let u = 0.5 - lon / (2.0 * core::f64::consts::PI);
        let v = 0.5 - lat / core::f64::consts::PI;
        let (w, h) = (self.width as f64, self.height as f64);
        let fx = u * w - 0.5;
        let fy = (v * h - 0.5).clamp(0.0, h - 1.0);
        let (x0, y0) = (fx.floor(), fy.floor());
        let (tx, ty) = (fx - x0, fy - y0);
        let px = |x: f64, y: f64| {
            let xi = (x.rem_euclid(w)) as usize % self.width;
            let yi = (y as usize).min(self.height - 1);
            self.pixels[yi * self.width + xi].map(f64::from)
        };
        let (a, b) = (px(x0, y0), px(x0 + 1.0, y0));
        let (c, e) = (px(x0, y0 + 1.0), px(x0 + 1.0, y0 + 1.0));
        core::array::from_fn(|k| {
            let top = a[k] + tx * (b[k] - a[k]);
            let bottom = c[k] + tx * (e[k] - c[k]);
            top + ty * (bottom - top)
        })
    }
}

/// Planckian locus samples `(T, x, y)` for the fit, from the human-vision band.
pub fn locus() -> Vec<(f64, f64, f64)> {
    let band = Band::human_vision();
    let ratio = (FIT_RANGE.1 / FIT_RANGE.0).ln();
    (0..LOCUS_SAMPLES)
        .map(|i| {
            let t = FIT_RANGE.0 * (ratio * i as f64 / (LOCUS_SAMPLES - 1) as f64).exp();
            let xy = chromaticity(&band.blackbody_response(t)).expect("blackbodies have positive XYZ");
            (t, xy[0], xy[1])
        })
        .collect()
}

/// Linear sRGB → XYZ matrix.
pub fn srgb_to_xyz_matrix() -> Mat3 {
    rgb_to_xyz_matrix(&SRGB_PRIMARIES, &D65_WHITE).expect("sRGB primaries are linearly independent")
}

/// Fits a blackbody to a linear-sRGB colour: `Some((w, T))` with `w · Y_lut(T) = Y`, or `None` for black,
/// negative-luminance or non-finite input.
pub fn fit_blackbody(
    rgb: &[f64; 3],
    m: &Mat3,
    locus: &[(f64, f64, f64)],
    lut: &BlackbodyLut,
) -> Option<(f64, f64)> {
    let xyz = mat3_vec(m, rgb);
    if !(xyz[1].is_finite() && xyz[1] > 0.0) {
        return None;
    }
    let [x, y] = chromaticity(&xyz)?;
    let t = locus
        .iter()
        .min_by(|a, b| {
            let da = (a.1 - x).powi(2) + (a.2 - y).powi(2);
            let db = (b.1 - x).powi(2) + (b.2 - y).powi(2);
            da.total_cmp(&db)
        })?
        .0;
    let y_bb = lut.lookup(t).response[1];
    (y_bb > 0.0).then(|| (xyz[1] / y_bb, t))
}

/// Converts an equirectangular image to a mip-mapped `(w, w·T)` cube map with `face_size` texels per face side,
/// sampling the image bilinearly at each texel centre. `lut` must be the human-vision table (it defines `Y`).
pub fn cube_from_equirect(img: &EquirectImage, face_size: usize, lut: &BlackbodyLut) -> CubeMap {
    let m = srgb_to_xyz_matrix();
    let loc = locus();
    let n = face_size.max(1);
    let mut base = CubeLevel::zeros(n);
    base.data.par_iter_mut().enumerate().for_each(|(k, texel)| {
        let face = k / (n * n);
        let (row, col) = ((k / n) % n, k % n);
        let d = face_uv_to_dir(face, (col as f64 + 0.5) / n as f64, (row as f64 + 0.5) / n as f64);
        if let Some((w, t)) = fit_blackbody(&img.sample(&d), &m, &loc, lut) {
            *texel = [w as f32, (w * t) as f32];
        }
    });
    CubeMap::with_mips(base)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::colour::xyz_to_linear_srgb;

    fn uniform(rgb: [f32; 3]) -> EquirectImage {
        EquirectImage {
            width: 64,
            height: 32,
            pixels: vec![rgb; 64 * 32],
        }
    }

    #[test]
    fn blackbody_colours_are_recovered() {
        // The sRGB colour of a blackbody at T fits back to T within the locus grid spacing (1.5 %), and the weight
        // reproduces the luminance through the same table the renderer uses (to the LUT interpolation, 1e-3).
        let lut = BlackbodyLut::human_vision();
        let (m, loc) = (srgb_to_xyz_matrix(), locus());
        let band = Band::human_vision();
        for t in [1800.0, 3000.0, 4500.0, 5772.0, 8000.0, 15_000.0, 30_000.0] {
            let xyz = band.blackbody_response(t).map(|c| c * 0.37);
            let (w, tf) = fit_blackbody(&xyz_to_linear_srgb(&xyz), &m, &loc, &lut).unwrap();
            assert!((tf / t - 1.0).abs() < 0.015, "T = {t}: fitted {tf}");
            let y = w * lut.lookup(tf).response[1];
            assert!((y / xyz[1] - 1.0).abs() < 1e-3, "T = {t}: Y {y} vs {}", xyz[1]);
        }
        assert_eq!(fit_blackbody(&[0.0; 3], &m, &loc, &lut), None);
        assert_eq!(fit_blackbody(&[-1.0, -1.0, -1.0], &m, &loc, &lut), None);
        assert_eq!(fit_blackbody(&[f64::NAN, 0.0, 0.0], &m, &loc, &lut), None);
        // Off-locus colours still get the nearest blackbody: pure green → a ~6000 K-ish neutral, pure red → cool.
        let (_, t_red) = fit_blackbody(&[1.0, 0.0, 0.0], &m, &loc, &lut).unwrap();
        let (_, t_blue) = fit_blackbody(&[0.0, 0.0, 1.0], &m, &loc, &lut).unwrap();
        assert!(t_red < 2000.0 && t_blue > 20_000.0, "red {t_red}, blue {t_blue}");
    }

    #[test]
    fn orientation_and_uniform_images() {
        // A uniform image gives a uniform cube. A single bright column at u = 0.25 (longitude +90°) lands on the
        // +y face; the top row lands on +z.
        let lut = BlackbodyLut::human_vision();
        let cube = cube_from_equirect(&uniform([0.5, 0.5, 0.5]), 8, &lut);
        let base = &cube.levels[0].data;
        assert!(
            base.iter()
                .all(|t| (t[0] - base[0][0]).abs() < 1e-6 * base[0][0] && t[0] > 0.0)
        );
        let mut img = uniform([0.0; 3]);
        for y in 12..20 {
            for x in 14..18 {
                img.pixels[y * 64 + x] = [1.0, 1.0, 1.0];
            }
        }
        assert!(img.sample(&[0.0, 1.0, 0.0])[0] > 0.5, "+y is at u = 0.25");
        assert_eq!(img.sample(&[0.0, -1.0, 0.0]), [0.0; 3]);
        let mut top = uniform([0.0; 3]);
        for x in 0..64 {
            top.pixels[x] = [2.0, 2.0, 2.0];
        }
        assert!(top.sample(&[0.0, 0.0, 1.0])[0] > 1.0 && top.sample(&[0.0, 0.0, -1.0])[0] == 0.0);
        // Wrap-around in longitude: −x sits on the seam.
        let seam = uniform([0.25; 3]);
        assert!((seam.sample(&[-1.0, 0.0, 0.0])[0] - 0.25).abs() < 1e-12);
    }
}
