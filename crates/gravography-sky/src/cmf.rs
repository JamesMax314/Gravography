//! CIE 1931 2° standard colorimetric observer: colour-matching functions `x̄, ȳ, z̄` (`docs/PRD.md` §6.1).
//!
//! The table is bundled unmodified as `assets/cie/CIE_xyz_1931_2deg.csv` (CIE 2019, DOI 10.25039/CIE.DS.xvudnb9b,
//! CC BY-SA 4.0; see `THIRD_PARTY.md`): 360–830 nm in 1 nm steps.

use std::sync::OnceLock;

use crate::SkyError;

const CIE_1931_2DEG_CSV: &str = include_str!("../../../assets/cie/CIE_xyz_1931_2deg.csv");

/// Colour-matching functions tabulated on a uniform wavelength grid.
#[derive(Clone, Debug, PartialEq)]
pub struct Cmf {
    /// Wavelength of the first sample (nm).
    pub lambda_min_nm: f64,
    /// Wavelength step (nm).
    pub step_nm: f64,
    /// `[x̄, ȳ, z̄]` per sample (dimensionless; `ȳ` peaks at 1).
    pub xyz: Vec<[f64; 3]>,
}

impl Cmf {
    /// Parses a CSV table `λ, x̄, ȳ, z̄` (no header) with uniformly increasing wavelengths.
    ///
    /// # Errors
    /// [`SkyError::InvalidTable`] for malformed rows, fewer than two rows or a non-uniform grid.
    pub fn parse_csv(text: &str) -> Result<Self, SkyError> {
        let mut lambdas = Vec::new();
        let mut xyz = Vec::new();
        for (n, line) in text.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }
            let vals: Vec<f64> = line
                .split(',')
                .map(|f| f.trim().parse::<f64>())
                .collect::<Result<_, _>>()
                .map_err(|e| SkyError::InvalidTable(format!("line {}: {e}", n + 1)))?;
            if vals.len() != 4 {
                return Err(SkyError::InvalidTable(format!(
                    "line {}: expected 4 columns",
                    n + 1
                )));
            }
            lambdas.push(vals[0]);
            xyz.push([vals[1], vals[2], vals[3]]);
        }
        if lambdas.len() < 2 {
            return Err(SkyError::InvalidTable("fewer than two rows".into()));
        }
        let step = lambdas[1] - lambdas[0];
        let uniform = lambdas
            .windows(2)
            .all(|w| ((w[1] - w[0]) - step).abs() <= 1e-9 * step.abs());
        if step.is_nan() || step <= 0.0 || !uniform {
            return Err(SkyError::InvalidTable(
                "wavelengths must be uniformly increasing".into(),
            ));
        }
        Ok(Self {
            lambda_min_nm: lambdas[0],
            step_nm: step,
            xyz,
        })
    }

    /// The bundled CIE 1931 2° table (parsed once).
    pub fn cie1931_2deg() -> &'static Self {
        static TABLE: OnceLock<Cmf> = OnceLock::new();
        TABLE.get_or_init(|| {
            Self::parse_csv(CIE_1931_2DEG_CSV).expect("the bundled CIE table is valid (checked by tests)")
        })
    }

    /// Wavelength of sample `i` (nm).
    pub fn lambda_nm(&self, i: usize) -> f64 {
        self.lambda_min_nm + self.step_nm * i as f64
    }

    /// Linearly interpolated `[x̄, ȳ, z̄]` at `lambda_nm`; zero outside the table.
    pub fn at(&self, lambda_nm: f64) -> [f64; 3] {
        let s = (lambda_nm - self.lambda_min_nm) / self.step_nm;
        let last = self.xyz.len() - 1;
        if s.is_nan() || s < 0.0 || s > last as f64 {
            return [0.0; 3];
        }
        let i = (s.floor() as usize).min(last - 1);
        let f = s - i as f64;
        let (a, b) = (self.xyz[i], self.xyz[i + 1]);
        [0, 1, 2].map(|c| a[c] + f * (b[c] - a[c]))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_relative_eq;

    #[test]
    fn bundled_table_shape() {
        let t = Cmf::cie1931_2deg();
        assert_eq!(t.xyz.len(), 471);
        assert_relative_eq!(t.lambda_min_nm, 360.0);
        assert_relative_eq!(t.step_nm, 1.0);
        assert_relative_eq!(t.lambda_nm(470), 830.0);
        // First row of the CIE file (360 nm).
        assert_relative_eq!(t.xyz[0][0], 0.000_129_9, max_relative = 1e-12);
        assert_relative_eq!(t.xyz[0][2], 0.000_606_1, max_relative = 1e-12);
    }

    #[test]
    fn y_bar_peaks_at_555_nm() {
        // CIE 1931: ȳ is the photopic luminous efficiency V(λ), normalised to 1 at 555 nm.
        let t = Cmf::cie1931_2deg();
        let (imax, _) = t
            .xyz
            .iter()
            .enumerate()
            .fold((0, 0.0), |m, (i, v)| if v[1] > m.1 { (i, v[1]) } else { m });
        assert_relative_eq!(t.lambda_nm(imax), 555.0);
        assert_relative_eq!(t.xyz[imax][1], 1.0, max_relative = 1e-6);
    }

    #[test]
    fn equal_energy_white_is_at_one_third() {
        // The CMFs are normalised so that the equal-energy spectrum E has x = y = 1/3 (CIE 015:2018). The 1 nm
        // table reproduces this to a few 1e-5.
        let t = Cmf::cie1931_2deg();
        let s = t
            .xyz
            .iter()
            .fold([0.0; 3], |a, v| [a[0] + v[0], a[1] + v[1], a[2] + v[2]]);
        let sum = s[0] + s[1] + s[2];
        assert!((s[0] / sum - 1.0 / 3.0).abs() < 1e-4);
        assert!((s[1] / sum - 1.0 / 3.0).abs() < 1e-4);
    }

    #[test]
    fn interpolation_and_bounds() {
        let t = Cmf::cie1931_2deg();
        let mid = t.at(555.5);
        for c in 0..3 {
            assert_relative_eq!(
                mid[c],
                0.5 * (t.xyz[195][c] + t.xyz[196][c]),
                max_relative = 1e-12
            );
        }
        assert_eq!(t.at(300.0), [0.0; 3]);
        assert_eq!(t.at(900.0), [0.0; 3]);
        assert_relative_eq!(t.at(830.0)[1], t.xyz[470][1]);
    }

    #[test]
    fn rejects_bad_tables() {
        assert!(Cmf::parse_csv("1,2,3").is_err());
        assert!(Cmf::parse_csv("1,0,0,0\n2,0,0,0\n4,0,0,0").is_err());
        assert!(Cmf::parse_csv("1,0,0,0").is_err());
        assert!(Cmf::parse_csv("1,0,x,0\n2,0,0,0").is_err());
    }
}
