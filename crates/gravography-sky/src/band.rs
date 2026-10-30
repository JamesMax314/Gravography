//! Observing bands and the blackbody colour lookup table (`docs/PRD.md` §6.1).
//!
//! A [`Band`] is a three-channel spectral response on a uniform wavelength grid plus a display matrix from the
//! three channels to linear sRGB. Human vision uses the CIE 1931 colour-matching functions (channels `X, Y, Z`) and
//! the XYZ → sRGB matrix. The false-colour bands (WP-13) are composites: three adjacent sub-bands (top-hat
//! responses), short → blue, medium → green, long → red, each normalised so that a blackbody at the band's reference
//! temperature (its emission peaks in the band) shows as neutral grey of luminance 1. A redder or bluer hue then
//! means a cooler or hotter (or red-/blueshifted) source, as in astronomical false-colour images. [`BlackbodyLut`] tabulates the band response to a blackbody, `T ↦ ∫ B_λ(T) r(λ) dλ`, on a log
//! grid for the GPU.

use crate::SkyError;
use crate::cmf::Cmf;
use crate::colour::{Mat3, chromaticity, xyz_from_chromaticity_sum, xyz_to_linear_srgb_matrix};
use crate::planck::planck_lambda;

/// A three-channel spectral response sampled on a uniform wavelength grid.
#[derive(Clone, Debug, PartialEq)]
pub struct Band {
    /// Human-readable name (shown in the UI).
    pub name: &'static str,
    /// Wavelength of the first sample (nm).
    pub lambda_min_nm: f64,
    /// Wavelength step (nm).
    pub step_nm: f64,
    /// Response of the three channels per sample (dimensionless).
    pub response: Vec<[f64; 3]>,
    /// Display mapping: linear sRGB = `display · channels`.
    pub display: Mat3,
    /// Reference temperature (K): a blackbody at this temperature filling the pixel is the manual-exposure
    /// reference. For the false-colour bands it also defines the neutral (grey) colour.
    pub reference_temperature: f64,
    /// Luminance as a function of the channels: `Y` itself `(0, 1, 0)` for human vision (as since WP-06), the
    /// Rec. 709 luminance of the displayed colour (`LUMA · display`) for the false-colour bands.
    pub luminance_weights: [f64; 3],
}

/// Wien's displacement constant `b = λ_max T` (m K; CODATA 2018).
pub const WIEN_B: f64 = 2.897_771_955e-3;

/// Rec. 709 / sRGB luminance weights of linear sRGB.
const LUMA: [f64; 3] = [0.2126, 0.7152, 0.0722];

impl Band {
    /// Human vision: CIE 1931 2° colour-matching functions; the channels are tristimulus `X, Y, Z`.
    pub fn human_vision() -> Self {
        let cmf = Cmf::cie1931_2deg();
        Self {
            name: "Human vision (CIE 1931 2°)",
            lambda_min_nm: cmf.lambda_min_nm,
            step_nm: cmf.step_nm,
            response: cmf.xyz.clone(),
            display: xyz_to_linear_srgb_matrix(),
            reference_temperature: 5772.0,
            luminance_weights: [0.0, 1.0, 0.0],
        }
    }

    /// False-colour composite of three wavelength windows `(λ_lo, λ_hi)` in nm, given from short to long and shown
    /// as blue, green and red. Each channel is scaled so that a blackbody at `reference_temperature` gives
    /// `rgb = (1, 1, 1)`. Sampled with `samples` points over the whole range.
    pub fn false_colour(
        name: &'static str,
        windows: [(f64, f64); 3],
        reference_temperature: f64,
        samples: usize,
    ) -> Self {
        let (lo, hi) = (windows[0].0, windows[2].1);
        let n = samples.max(3);
        let step = (hi - lo) / (n - 1) as f64;
        let response = (0..n)
            .map(|i| {
                let l = lo + step * i as f64;
                windows.map(|(a, b)| if l >= a && l < b { 1.0 } else { 0.0 })
            })
            .collect();
        let mut band = Self {
            name,
            lambda_min_nm: lo,
            step_nm: step,
            response,
            display: [[0.0; 3]; 3],
            reference_temperature,
            luminance_weights: [0.0; 3],
        };
        let r = band.blackbody_response(reference_temperature);
        // Channels (short, medium, long) → (B, G, R); rows of the display matrix are R, G, B.
        band.display = [
            [0.0, 0.0, 1.0 / r[2]],
            [0.0, 1.0 / r[1], 0.0],
            [1.0 / r[0], 0.0, 0.0],
        ];
        band.luminance_weights = core::array::from_fn(|c| (0..3).map(|k| LUMA[k] * band.display[k][c]).sum());
        band
    }

    /// Near-infrared composite: J (1.1–1.4 µm), H (1.5–1.8 µm), K (2.0–2.4 µm) as blue, green, red (gaps between
    /// the atmospheric windows left out). Reference: the blackbody peaking at 1.65 µm (≈ 1756 K).
    pub fn near_infrared() -> Self {
        Self::false_colour(
            "Near infrared (J, H, K → B, G, R)",
            [(1100.0, 1400.0), (1500.0, 1800.0), (2000.0, 2400.0)],
            WIEN_B / 1650e-9,
            1301,
        )
    }

    /// Near-ultraviolet composite: 200–260, 260–320, 320–380 nm as blue, green, red. Reference: the blackbody
    /// peaking at 290 nm (≈ 9990 K).
    pub fn near_ultraviolet() -> Self {
        Self::false_colour(
            "Near ultraviolet (200–380 nm → B, G, R)",
            [(200.0, 260.0), (260.0, 320.0), (320.0, 380.0)],
            WIEN_B / 290e-9,
            721,
        )
    }

    /// Soft X-ray composite in the usual Chandra split: 2–8 keV (0.155–0.62 nm) blue, 1–2 keV (0.62–1.24 nm) green,
    /// 0.3–1 keV (1.24–4.13 nm) red. Reference: the blackbody peaking at 0.8 nm (≈ 3.6 MK). Ordinary stars
    /// (≤ 5·10⁴ K) emit essentially nothing here; only strongly blueshifted light (g ≳ 50) appears.
    pub fn soft_xray() -> Self {
        // λ[nm] = 1.239 842 / E[keV].
        let nm = |kev: f64| 1.239_841_98 / kev;
        Self::false_colour(
            "Soft X-ray (2–8, 1–2, 0.3–1 keV → B, G, R)",
            [(nm(8.0), nm(2.0)), (nm(2.0), nm(1.0)), (nm(1.0), nm(0.3))],
            WIEN_B / 0.8e-9,
            4001,
        )
    }

    /// Displayed linear sRGB of a blackbody at `t` filling the pixel.
    pub fn display_rgb(&self, t: f64) -> [f64; 3] {
        crate::colour::mat3_vec(&self.display, &self.blackbody_response(t))
    }

    /// Rec. 709 luminance of the displayed blackbody at `t`.
    pub fn display_luminance(&self, t: f64) -> f64 {
        let rgb = self.display_rgb(t);
        LUMA[0] * rgb[0] + LUMA[1] * rgb[1] + LUMA[2] * rgb[2]
    }

    /// Band response to a blackbody at temperature `t` (K): `Σᵢ B_λ(λᵢ, T) r(λᵢ) Δλ` (the CIE-recommended
    /// summation at the table spacing), in W m⁻² sr⁻¹ (unweighted by any luminous efficacy). For human vision this
    /// is the tristimulus `XYZ` of the blackbody.
    pub fn blackbody_response(&self, t: f64) -> [f64; 3] {
        let dl = self.step_nm * 1e-9;
        let mut acc = [0.0; 3];
        for (i, r) in self.response.iter().enumerate() {
            let b = planck_lambda((self.lambda_min_nm + self.step_nm * i as f64) * 1e-9, t);
            for c in 0..3 {
                acc[c] += b * r[c] * dl;
            }
        }
        acc
    }
}

/// Default LUT size (entries over `[T_min, T_max]`). PRD §6.1 asks for ≥ 1024. 4096 keeps the linear-interpolation
/// error in `ln S` below ~3e-4 even at 100 K, where `ln S` is most curved in `ln T`.
pub const DEFAULT_LUT_SIZE: usize = 4096;
/// Default LUT temperature range (K), `docs/PRD.md` §6.1.
pub const DEFAULT_LUT_RANGE: (f64, f64) = (100.0, 1e7);

/// Blackbody response table on a uniform grid in `ln T`, laid out for a GPU texture.
///
/// Entry `i` (for `T_i = T_min (T_max/T_min)^{i/(n−1)}`) is `[ln S, x, y, 0]` with `S = c₀ + c₁ + c₂` and
/// chromaticity `(x, y) = (c₀, c₁)/S`. Storing `ln S` keeps the huge dynamic range (≈ e^{−360} at 100 K) in f32, and
/// all three quantities vary smoothly with `ln T`, so linear interpolation is accurate.
#[derive(Clone, Debug, PartialEq)]
pub struct BlackbodyLut {
    /// Lowest tabulated temperature (K).
    pub t_min: f64,
    /// Highest tabulated temperature (K).
    pub t_max: f64,
    /// `[ln S, x, y, 0]` per entry.
    pub entries: Vec<[f32; 4]>,
    /// Display mapping of the band (channels → linear sRGB).
    pub display: Mat3,
    /// Channel weights giving luminance ([`Band::luminance_weights`]).
    pub luminance_weights: [f64; 3],
    /// Manual-exposure reference temperature (K).
    pub reference_temperature: f64,
    /// Band name (UI).
    pub name: &'static str,
}

/// Floor for `ln S` in the table. Bands far from a temperature's emission (X-rays at 100 K: `S ≈ e^{−10⁵}`)
/// underflow to `S = 0`; storing `ln 0 = −∞` would make interpolation produce NaN, so it is clamped to a value
/// whose exponential is 0 in f32 and f64 alike.
pub const LN_S_FLOOR: f64 = -1000.0;

/// Result of a LUT lookup.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LutSample {
    /// Band response (for human vision: `XYZ`), W m⁻² sr⁻¹.
    pub response: [f64; 3],
    /// `true` if `T` was outside `[T_min, T_max]` and was clamped (the UI should indicate it, PRD §6.1).
    pub clamped: bool,
}

impl BlackbodyLut {
    /// Builds a table of `n ≥ 2` entries over `[t_min, t_max]` for `band`.
    ///
    /// # Errors
    /// [`SkyError::InvalidParameters`] if `n < 2` or the range is empty or non-positive.
    pub fn build(band: &Band, t_min: f64, t_max: f64, n: usize) -> Result<Self, SkyError> {
        let valid = t_min > 0.0 && t_max > t_min && t_max.is_finite();
        if n < 2 || !valid {
            return Err(SkyError::InvalidParameters(
                "LUT needs n ≥ 2 and 0 < T_min < T_max < ∞",
            ));
        }
        let ratio = (t_max / t_min).ln();
        let entries = (0..n)
            .map(|i| {
                let t = t_min * (ratio * i as f64 / (n - 1) as f64).exp();
                let r = band.blackbody_response(t);
                let s: f64 = r.iter().sum();
                let xy = chromaticity(&r).unwrap_or([1.0 / 3.0; 2]);
                [s.ln().max(LN_S_FLOOR) as f32, xy[0] as f32, xy[1] as f32, 0.0]
            })
            .collect();
        Ok(Self {
            t_min,
            t_max,
            entries,
            display: band.display,
            luminance_weights: band.luminance_weights,
            reference_temperature: band.reference_temperature,
            name: band.name,
        })
    }

    /// Table for `band` with the default size and range.
    pub fn for_band(band: &Band) -> Self {
        Self::build(band, DEFAULT_LUT_RANGE.0, DEFAULT_LUT_RANGE.1, DEFAULT_LUT_SIZE)
            .expect("default LUT parameters are valid")
    }

    /// Human-vision table with the default size and range.
    pub fn human_vision() -> Self {
        Self::for_band(&Band::human_vision())
    }

    /// Manual-exposure reference: luminance of a blackbody at the band's reference temperature filling the pixel
    /// (`w = 1`, `g = 1`), from the table. For human vision `Y` of a 5772 K blackbody.
    pub fn reference_luminance(&self) -> f64 {
        let r = self.lookup(self.reference_temperature).response;
        (0..3).map(|c| self.luminance_weights[c] * r[c]).sum()
    }

    /// Texture coordinate of `t` in `[0, 1]` (unclamped): `ln(T/T_min) / ln(T_max/T_min)`. The GPU samples entry
    /// centres at `(i + 0.5)/n`, so it must use `(coord · (n − 1) + 0.5) / n`.
    pub fn coord(&self, t: f64) -> f64 {
        (t / self.t_min).ln() / (self.t_max / self.t_min).ln()
    }

    /// Linearly interpolated band response at temperature `t` (K), clamped to the table range.
    pub fn lookup(&self, t: f64) -> LutSample {
        let n = self.entries.len();
        let u = self.coord(t);
        let clamped = !(0.0..=1.0).contains(&u);
        let s = (if u.is_nan() { 0.0 } else { u.clamp(0.0, 1.0) }) * (n - 1) as f64;
        let i = (s.floor() as usize).min(n - 2);
        let f = s - i as f64;
        let (a, b) = (self.entries[i], self.entries[i + 1]);
        let lerp = |k: usize| f64::from(a[k]) + f * (f64::from(b[k]) - f64::from(a[k]));
        LutSample {
            response: xyz_from_chromaticity_sum(&[lerp(1), lerp(2)], lerp(0).exp()),
            clamped,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::colour::chromaticity;
    use crate::planck::C2;

    /// Kim et al. cubic-spline approximation of the CIE 1931 Planckian locus, 1667–25000 K (US patent 7024034,
    /// 2006; coefficients as quoted on Wikipedia, "Planckian locus").
    fn kim_planckian_locus(t: f64) -> [f64; 2] {
        let x = if t <= 4000.0 {
            -0.266_123_9e9 / t.powi(3) - 0.234_358_9e6 / t.powi(2) + 0.877_695_6e3 / t + 0.179_910
        } else {
            -3.025_846_9e9 / t.powi(3) + 2.107_037_9e6 / t.powi(2) + 0.222_634_7e3 / t + 0.240_390
        };
        let y = if t <= 2222.0 {
            -1.106_381_4 * x.powi(3) - 1.348_110_20 * x * x + 2.185_558_32 * x - 0.202_196_83
        } else if t <= 4000.0 {
            -0.954_947_6 * x.powi(3) - 1.374_185_93 * x * x + 2.091_370_15 * x - 0.167_488_67
        } else {
            3.081_758_0 * x.powi(3) - 5.873_386_70 * x * x + 3.751_129_97 * x - 0.370_014_83
        };
        [x, y]
    }

    #[test]
    fn false_colour_bands_are_neutral_at_reference_and_shift_with_temperature() {
        // By construction the reference blackbody is (1, 1, 1); hotter sources are bluer (B/R grows) and cooler ones
        // redder (Wien: the peak moves to shorter wavelengths as T grows). The reference luminance is 1.
        for band in [Band::near_infrared(), Band::near_ultraviolet(), Band::soft_xray()] {
            let rgb = band.display_rgb(band.reference_temperature);
            for c in rgb {
                assert!((c - 1.0).abs() < 1e-12, "{}: {rgb:?}", band.name);
            }
            assert!((band.display_luminance(band.reference_temperature) - 1.0).abs() < 1e-12);
            // Via the table: interpolation error in ln S (≲ 3e-4 relative, see DEFAULT_LUT_SIZE).
            let lut = BlackbodyLut::for_band(&band);
            assert!((lut.reference_luminance() - 1.0).abs() < 1e-3, "{}", band.name);
            let hot = band.display_rgb(band.reference_temperature * 2.0);
            let cold = band.display_rgb(band.reference_temperature * 0.5);
            assert!(hot[2] / hot[0] > 1.0 && cold[2] / cold[0] < 1.0, "{}", band.name);
            // The reference peak wavelength lies inside the band's range.
            let peak_nm = WIEN_B / band.reference_temperature * 1e9;
            let top = band.lambda_min_nm + band.step_nm * (band.response.len() - 1) as f64;
            assert!(peak_nm > band.lambda_min_nm && peak_nm < top, "{}", band.name);
        }
        // Human vision: the reference luminance is exactly the table's Y at 5772 K (unchanged since WP-06). The
        // displayed Rec. 709 luminance agrees to the rounding of the Rec. 709 weights vs the exact sRGB Y row
        // (≈ 1e-4 relative; bound 5e-4).
        let hv = Band::human_vision();
        let y = hv.blackbody_response(5772.0)[1];
        let lut = BlackbodyLut::human_vision();
        assert_eq!(lut.reference_luminance(), lut.lookup(5772.0).response[1]);
        assert!((hv.display_luminance(5772.0) / y - 1.0).abs() < 5e-4);
    }

    #[test]
    fn xray_lut_is_finite_where_stars_are_dark() {
        // A 100 K blackbody emits nothing at 1 nm (S underflows to 0); the table stays finite (ln S clamped) and
        // lookups return 0, while a 5 MK source is bright.
        let lut = BlackbodyLut::for_band(&Band::soft_xray());
        assert!(lut.entries.iter().all(|e| e.iter().all(|c| c.is_finite())));
        assert_eq!(lut.lookup(100.0).response, [0.0; 3]);
        // A hot O star (30 000 K) is ~10⁻⁵⁰ of a 5 MK source here (Wien tail e^{−hc/λkT}): invisible.
        let bright = lut.lookup(5e6).response.iter().sum::<f64>();
        assert!(bright > 0.0);
        assert!(lut.lookup(30_000.0).response.iter().sum::<f64>() < 1e-40 * bright);
        // Interpolation across the floor/finite boundary stays finite.
        for t in [2e5, 3e5, 5e5, 8e5] {
            assert!(
                lut.lookup(t).response.iter().all(|c| c.is_finite() && *c >= 0.0),
                "T = {t}"
            );
        }
    }

    #[test]
    fn planckian_locus_matches_published_values() {
        // PRD §9 test 13: chromaticities of blackbodies vs published locus, tolerance 2e-3.
        let band = Band::human_vision();
        let mut worst: f64 = 0.0;
        for t in [
            1700.0, 2000.0, 2500.0, 3000.0, 4000.0, 5000.0, 6500.0, 10_000.0, 15_000.0, 25_000.0,
        ] {
            let xy = chromaticity(&band.blackbody_response(t)).unwrap();
            let k = kim_planckian_locus(t);
            let d = (xy[0] - k[0]).abs().max((xy[1] - k[1]).abs());
            worst = worst.max(d);
            assert!(d < 2e-3, "T = {t}: {xy:?} vs {k:?}");
        }
        // The spline itself is accurate to ~1e-3 or better; a much larger worst case would indicate a bug.
        assert!(worst < 1.5e-3, "worst {worst}");
    }

    #[test]
    fn illuminant_a_chromaticity() {
        // CIE standard illuminant A is a Planckian radiator with c₂ = 1.435e-2 m K at 2848 K, i.e. T = 2848 · c₂/1.435e-2
        // ≈ 2856 K with the modern c₂. Published (x, y) = (0.44757, 0.40745) (CIE 015; e.g. colour-science dataset). The
        // published values are rounded to 5 decimals and computed from the tabulated SPD on a 5 nm grid, so agreement is
        // to ~1e-5 (measured: 0.447574, 0.407439); tolerance 2e-5, far tighter than test 13's 2e-3.
        let t = 2848.0 * C2 / 1.435e-2;
        let xy = chromaticity(&Band::human_vision().blackbody_response(t)).unwrap();
        assert!(
            (xy[0] - 0.447_57).abs() < 2e-5 && (xy[1] - 0.407_45).abs() < 2e-5,
            "{xy:?}"
        );
    }

    #[test]
    fn hotter_is_bluer_and_brighter() {
        let band = Band::human_vision();
        let mut prev_x = 1.0;
        let mut prev_y = 0.0;
        for i in 0..40 {
            let t = 1000.0 * 1.2_f64.powi(i);
            let r = band.blackbody_response(t);
            let x = chromaticity(&r).unwrap()[0];
            assert!(x < prev_x, "x must decrease with T");
            assert!(r[1] > prev_y, "Y must increase with T");
            prev_x = x;
            prev_y = r[1];
        }
    }

    #[test]
    fn lut_matches_direct_integration() {
        // Linear interpolation in ln T of (ln S, x, y): measured worst errors are well below these bounds (the
        // curvature of ln S ∝ 1/T dominates at 100 K: h²/8 · c₂/(λT) with h = ln(10⁵)/4095).
        let band = Band::human_vision();
        let lut = BlackbodyLut::human_vision();
        assert_eq!(lut.entries.len(), DEFAULT_LUT_SIZE);
        let mut worst_y: f64 = 0.0;
        let mut worst_xy: f64 = 0.0;
        for i in 0..997 {
            let t = 100.0 * (1e5_f64).powf((f64::from(i) + 0.37) / 997.0);
            let exact = band.blackbody_response(t);
            let s = lut.lookup(t);
            assert!(!s.clamped);
            worst_y = worst_y.max((s.response[1] / exact[1] - 1.0).abs());
            let (a, b) = (chromaticity(&s.response).unwrap(), chromaticity(&exact).unwrap());
            worst_xy = worst_xy.max((a[0] - b[0]).abs().max((a[1] - b[1]).abs()));
        }
        assert!(worst_y < 5e-4, "Y error {worst_y}");
        assert!(worst_xy < 1e-5, "xy error {worst_xy}");
    }

    #[test]
    fn lut_clamps_and_flags() {
        let lut = BlackbodyLut::build(&Band::human_vision(), 1000.0, 1e5, 64).unwrap();
        let lo = lut.lookup(10.0);
        assert!(lo.clamped);
        let at_min = lut.lookup(1000.0);
        assert!(!at_min.clamped);
        for c in 0..3 {
            assert!((lo.response[c] / at_min.response[c] - 1.0).abs() < 1e-6);
        }
        assert!(lut.lookup(1e9).clamped);
        assert!(BlackbodyLut::build(&Band::human_vision(), 10.0, 5.0, 64).is_err());
        assert!(BlackbodyLut::build(&Band::human_vision(), 10.0, 50.0, 1).is_err());
    }

    #[test]
    fn redshifted_spectrum_integrates_to_hotter_blackbody() {
        // PRD §9 test 14 (shading identity, §5.7/§6.3): integrating the transformed spectrum g⁵ B_λ(gλ, T) over the
        // band equals the band response of a blackbody at gT. Same summation grid, so equal to rounding.
        let band = Band::human_vision();
        for (t, g) in [(3000.0_f64, 2.0_f64), (6000.0, 0.3), (20_000.0, 1.7)] {
            let dl = band.step_nm * 1e-9;
            let mut acc = [0.0; 3];
            for (i, r) in band.response.iter().enumerate() {
                let lambda = (band.lambda_min_nm + band.step_nm * i as f64) * 1e-9;
                let b = g.powi(5) * planck_lambda(g * lambda, t);
                for c in 0..3 {
                    acc[c] += b * r[c] * dl;
                }
            }
            let direct = band.blackbody_response(g * t);
            for c in 0..3 {
                assert!((acc[c] / direct[c] - 1.0).abs() < 1e-12);
            }
        }
    }
}
