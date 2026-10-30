//! Exposure and tone mapping (`docs/PRD.md` §6.6; `docs/physics.md` § "Exposure and tone mapping").
//!
//! The HDR image holds linear sRGB (Rec. 709 primaries, D65) radiance in the units of the blackbody LUT. Display
//! values are `encode(tonemap(scale · rgb))`, where the exposure scale comes from [`Exposure`]. Only this display
//! path clamps; the HDR buffer is never clamped.

use gravography_sky::colour::srgb_encode;
use serde::{Deserialize, Serialize};

/// Luminance `Y` of linear sRGB (second row of the sRGB → XYZ matrix, IEC 61966-2-1).
pub const LUMA: [f64; 3] = [0.2126, 0.7152, 0.0722];

/// Mid-grey key for auto exposure (Reinhard et al. 2002, "Photographic tone reproduction").
pub const AUTO_KEY: f64 = 0.18;

/// Exposure settings.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Exposure {
    /// Auto exposure: scale so that the log-average luminance of the lit pixels maps to [`AUTO_KEY`].
    pub auto: bool,
    /// Exposure value in stops: compensation on top of auto exposure, or the absolute exposure when `auto` is
    /// off (`scale = 2^ev / y_ref`, see [`Exposure::scale`]).
    pub ev: f64,
}

impl Default for Exposure {
    /// Auto exposure, no compensation.
    fn default() -> Self {
        Self { auto: true, ev: 0.0 }
    }
}

impl Exposure {
    /// Exposure scale. `log_average` is the log-average luminance of the image ([`log_average_luminance`]),
    /// `y_ref` the luminance that manual exposure at `ev = 0` maps to 1 (the renderer uses a solar-temperature
    /// blackbody filling the whole pixel, `w = 1`, `T = 5772 K`, `g = 1`).
    pub fn scale(&self, log_average: Option<f64>, y_ref: f64) -> f64 {
        let stops = self.ev.exp2();
        match (self.auto, log_average) {
            (true, Some(avg)) if avg > 0.0 => stops * AUTO_KEY / avg,
            _ => stops / y_ref,
        }
    }
}

/// Log-average luminance `exp(mean ln Y)` over pixels with `Y > 0` (black pixels — the shadow, captured rays —
/// are excluded so the exposure does not depend on the shadow's size). Summed sequentially for determinism.
/// Returns `None` if no pixel is lit.
pub fn log_average_luminance(pixels: &[[f32; 3]]) -> Option<f64> {
    let (mut sum, mut n) = (0.0_f64, 0_u64);
    for p in pixels {
        let y = luminance(&p.map(f64::from));
        if y > 0.0 && y.is_finite() {
            sum += y.ln();
            n += 1;
        }
    }
    (n > 0).then(|| (sum / n as f64).exp())
}

/// Luminance of linear sRGB.
pub fn luminance(rgb: &[f64; 3]) -> f64 {
    LUMA[0] * rgb[0] + LUMA[1] * rgb[1] + LUMA[2] * rgb[2]
}

/// Tone-mapping operator.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToneMap {
    /// Stephen Hill's fit of the ACES RRT + sRGB ODT (default): filmic contrast, highlights desaturate.
    #[default]
    Aces,
    /// Extended Reinhard on luminance with white point [`REINHARD_WHITE`] (neutral: preserves hue and saturation).
    Reinhard,
}

/// Luminance mapped to 1 by [`ToneMap::Reinhard`].
pub const REINHARD_WHITE: f64 = 16.0;

impl ToneMap {
    /// Maps exposed linear sRGB to display-linear sRGB in `[0, 1]`.
    pub fn apply(self, rgb: &[f64; 3]) -> [f64; 3] {
        let rgb = rgb.map(|c| if c.is_nan() { 0.0 } else { c.max(0.0) });
        match self {
            Self::Aces => aces_fitted(&rgb),
            Self::Reinhard => reinhard_extended(&rgb, REINHARD_WHITE),
        }
    }
}

/// ACES fitted curve (S. Hill, "BakingLab", MIT licence): `M_out · RRT_ODT(M_in · rgb)`, with
/// `RRT_ODT(v) = (v(v + 0.0245786) − 0.000090537) / (v(0.983729 v + 0.4329510) + 0.238081)`, clamped to `[0, 1]`.
/// `M_in` maps sRGB to the ACES AP1 working space (with the RRT saturation adjustment), `M_out` back.
pub fn aces_fitted(rgb: &[f64; 3]) -> [f64; 3] {
    const M_IN: [[f64; 3]; 3] = [
        [0.59719, 0.35458, 0.04823],
        [0.07600, 0.90834, 0.01566],
        [0.02840, 0.13383, 0.83777],
    ];
    const M_OUT: [[f64; 3]; 3] = [
        [1.60475, -0.53108, -0.07367],
        [-0.10208, 1.10813, -0.00605],
        [-0.00327, -0.07276, 1.07602],
    ];
    let mul = |m: &[[f64; 3]; 3], v: &[f64; 3]| -> [f64; 3] {
        core::array::from_fn(|i| m[i][0] * v[0] + m[i][1] * v[1] + m[i][2] * v[2])
    };
    let v = mul(&M_IN, rgb).map(|v| {
        let a = v * (v + 0.024_578_6) - 0.000_090_537;
        let b = v * (0.983_729 * v + 0.432_951) + 0.238_081;
        a / b
    });
    mul(&M_OUT, &v).map(|c| c.clamp(0.0, 1.0))
}

/// Extended Reinhard on luminance: `L_d = L (1 + L/L_w²) / (1 + L)`, colour scaled by `L_d / L` and clamped to
/// `[0, 1]` (Reinhard et al. 2002, eq. 4).
pub fn reinhard_extended(rgb: &[f64; 3], white: f64) -> [f64; 3] {
    let l = luminance(rgb);
    if l <= 0.0 {
        return [0.0; 3];
    }
    let ld = l * (1.0 + l / (white * white)) / (1.0 + l);
    rgb.map(|c| (c * ld / l).clamp(0.0, 1.0))
}

/// Exposes, tone maps and sRGB-encodes linear pixels to 8-bit RGB (rounded to nearest).
pub fn to_srgb8(pixels: &[[f32; 3]], scale: f64, op: ToneMap) -> Vec<u8> {
    pixels
        .iter()
        .flat_map(|p| {
            let mapped = op.apply(&p.map(|c| f64::from(c) * scale));
            mapped.map(|c| (srgb_encode(c) * 255.0).round() as u8)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_relative_eq;

    #[test]
    fn aces_endpoints_and_monotonicity() {
        // Black stays black (the curve is slightly negative at 0 and clamped); large values saturate to white
        // (RRT_ODT(v) → 1/0.983729 > 1, clamped); grey stays neutral to the fit's matrix precision (row sums of
        // M_in, M_out are 1 to ±1e-5).
        assert_eq!(aces_fitted(&[0.0; 3]), [0.0; 3]);
        assert_eq!(aces_fitted(&[1e6; 3]), [1.0; 3]);
        let mut prev = 0.0;
        for i in 1..200 {
            let x = 0.01 * f64::from(i) * f64::from(i) / 10.0;
            let y = aces_fitted(&[x; 3]);
            assert!(y[1] >= prev, "not monotonic at {x}");
            assert_relative_eq!(y[0], y[1], epsilon = 1e-4);
            assert_relative_eq!(y[2], y[1], epsilon = 1e-4);
            prev = y[1];
        }
        // Mid grey: the matrix rows sum to 1, so grey 0.18 maps to RRT_ODT(0.18) = 0.036734/0.347885 = 0.105591
        // (evaluated by hand from the formula above; 1e-5 covers the 6-digit hand arithmetic).
        assert_relative_eq!(aces_fitted(&[0.18; 3])[1], 0.105_591, epsilon = 1e-5);
    }

    #[test]
    fn reinhard_properties() {
        // L_d(L_w) = 1 (definition of the white point); small L: L_d ≈ L; hue preserved (channel ratios kept).
        let w = REINHARD_WHITE;
        assert_relative_eq!(luminance(&reinhard_extended(&[w; 3], w)), 1.0, epsilon = 1e-12);
        assert_relative_eq!(reinhard_extended(&[1e-6; 3], w)[0], 1e-6, max_relative = 1e-5);
        let c = reinhard_extended(&[0.4, 0.2, 0.1], w);
        assert_relative_eq!(c[0] / c[1], 2.0, epsilon = 1e-12);
        assert_relative_eq!(c[1] / c[2], 2.0, epsilon = 1e-12);
        assert_eq!(reinhard_extended(&[0.0; 3], w), [0.0; 3]);
        assert_eq!(ToneMap::Reinhard.apply(&[f64::NAN, -1.0, 0.0]), [0.0; 3]);
    }

    #[test]
    fn auto_exposure_maps_log_average_to_key() {
        // Two lit pixels with Y = 0.5 and 2 (log-average 1) plus a black pixel (ignored).
        let px = [[0.5_f32; 3], [2.0; 3], [0.0; 3]];
        let avg = log_average_luminance(&px).unwrap();
        assert_relative_eq!(avg, 1.0, epsilon = 1e-6);
        let e = Exposure { auto: true, ev: 1.0 };
        assert_relative_eq!(e.scale(Some(avg), 123.0), 2.0 * AUTO_KEY / avg, epsilon = 1e-12);
        let manual = Exposure {
            auto: false,
            ev: -2.0,
        };
        assert_relative_eq!(manual.scale(Some(avg), 8.0), 0.25 / 8.0, epsilon = 1e-15);
        // An all-black image falls back to the manual reference.
        assert!(log_average_luminance(&[[0.0; 3]]).is_none());
        assert_relative_eq!(Exposure::default().scale(None, 4.0), 0.25, epsilon = 1e-15);
    }

    #[test]
    fn srgb8_encoding() {
        // Reinhard at tiny input is ≈ linear; sRGB(1) = 255, sRGB(0) = 0, and 0.5 display-linear → 188
        // (IEC 61966-2-1: 1.055 · 0.5^(1/2.4) − 0.055 = 0.7354 → 187.5 → 188).
        let out = to_srgb8(&[[0.0; 3], [1e9; 3]], 1.0, ToneMap::Aces);
        assert_eq!(out, vec![0, 0, 0, 255, 255, 255]);
        let half = to_srgb8(&[[0.5; 3]], 1.0, ToneMap::Reinhard);
        let ld: f64 = 0.5 * (1.0 + 0.5 / (REINHARD_WHITE * REINHARD_WHITE)) / 1.5;
        assert_eq!(half[0], (srgb_encode(ld) * 255.0).round() as u8);
        assert_eq!((srgb_encode(0.5) * 255.0).round() as u8, 188);
    }
}
