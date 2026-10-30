//! Observing bands and the blackbody colour lookup table (`docs/PRD.md` §6.1).
//!
//! A [`Band`] is a three-channel spectral response on a uniform wavelength grid. v1 uses human vision (the CIE
//! 1931 colour-matching functions, channels `X, Y, Z`); WP-13 adds false-colour bands with their own display
//! mapping. [`BlackbodyLut`] tabulates the band response to a blackbody, `T ↦ ∫ B_λ(T) r(λ) dλ`, on a log
//! grid for the GPU.

use crate::SkyError;
use crate::cmf::Cmf;
use crate::colour::{chromaticity, xyz_from_chromaticity_sum};
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
}

impl Band {
    /// Human vision: CIE 1931 2° colour-matching functions; the channels are tristimulus `X, Y, Z`.
    pub fn human_vision() -> Self {
        let cmf = Cmf::cie1931_2deg();
        Self {
            name: "Human vision (CIE 1931 2°)",
            lambda_min_nm: cmf.lambda_min_nm,
            step_nm: cmf.step_nm,
            response: cmf.xyz.clone(),
        }
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
}

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
                [s.ln() as f32, xy[0] as f32, xy[1] as f32, 0.0]
            })
            .collect();
        Ok(Self {
            t_min,
            t_max,
            entries,
        })
    }

    /// Human-vision table with the default size and range.
    pub fn human_vision() -> Self {
        Self::build(
            &Band::human_vision(),
            DEFAULT_LUT_RANGE.0,
            DEFAULT_LUT_RANGE.1,
            DEFAULT_LUT_SIZE,
        )
        .expect("default LUT parameters are valid")
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
