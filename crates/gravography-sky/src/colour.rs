//! Colour-space conversions: chromaticity, XYZ → linear sRGB (D65), and the sRGB transfer function
//! (`docs/PRD.md` §6.1; IEC 61966-2-1).

/// A 3×3 matrix, `m[row][col]`.
pub type Mat3 = [[f64; 3]; 3];

/// sRGB primaries `(x, y)` for R, G, B (IEC 61966-2-1 / ITU-R BT.709).
pub const SRGB_PRIMARIES: [[f64; 2]; 3] = [[0.64, 0.33], [0.30, 0.60], [0.15, 0.06]];
/// D65 white point chromaticity used by sRGB (IEC 61966-2-1).
pub const D65_WHITE: [f64; 2] = [0.3127, 0.3290];

/// Chromaticity `(x, y) = (X, Y) / (X + Y + Z)`. Returns `None` for a zero (or non-finite) sum.
pub fn chromaticity(xyz: &[f64; 3]) -> Option<[f64; 2]> {
    let s = xyz[0] + xyz[1] + xyz[2];
    if s.is_finite() && s.abs() > 0.0 {
        Some([xyz[0] / s, xyz[1] / s])
    } else {
        None
    }
}

/// XYZ from chromaticity `(x, y)` and the sum `S = X + Y + Z`: `S (x, y, 1 − x − y)`.
pub fn xyz_from_chromaticity_sum(xy: &[f64; 2], sum: f64) -> [f64; 3] {
    [sum * xy[0], sum * xy[1], sum * (1.0 - xy[0] - xy[1])]
}

/// Matrix–vector product.
pub fn mat3_vec(m: &Mat3, v: &[f64; 3]) -> [f64; 3] {
    [0, 1, 2].map(|i| m[i][0] * v[0] + m[i][1] * v[1] + m[i][2] * v[2])
}

/// Inverse of a 3×3 matrix (adjugate), or `None` if singular.
pub fn mat3_inverse(m: &Mat3) -> Option<Mat3> {
    let c = |r1: usize, c1: usize, r2: usize, c2: usize| m[r1][c1] * m[r2][c2] - m[r1][c2] * m[r2][c1];
    let adj = [
        [c(1, 1, 2, 2), -c(0, 1, 2, 2), c(0, 1, 1, 2)],
        [-c(1, 0, 2, 2), c(0, 0, 2, 2), -c(0, 0, 1, 2)],
        [c(1, 0, 2, 1), -c(0, 0, 2, 1), c(0, 0, 1, 1)],
    ];
    let det = m[0][0] * adj[0][0] + m[0][1] * adj[1][0] + m[0][2] * adj[2][0];
    if !det.is_finite() || det.abs() <= 0.0 {
        return None;
    }
    Some(adj.map(|row| row.map(|v| v / det)))
}

/// RGB → XYZ matrix for primaries `(x, y)` and a white point `(x, y)` normalised to `Y = 1` (standard derivation,
/// e.g. SMPTE RP 177): columns are the primaries' XYZ scaled so that RGB = (1, 1, 1) maps to the white point.
pub fn rgb_to_xyz_matrix(primaries: &[[f64; 2]; 3], white: &[f64; 2]) -> Option<Mat3> {
    let col = |p: &[f64; 2]| [p[0] / p[1], 1.0, (1.0 - p[0] - p[1]) / p[1]];
    let cols = [col(&primaries[0]), col(&primaries[1]), col(&primaries[2])];
    let p: Mat3 = [0, 1, 2].map(|i| [cols[0][i], cols[1][i], cols[2][i]]);
    let w = col(white);
    let s = mat3_vec(&mat3_inverse(&p)?, &w);
    Some([0, 1, 2].map(|i| [0, 1, 2].map(|j| p[i][j] * s[j])))
}

/// XYZ → linear sRGB (D65) matrix, derived from [`SRGB_PRIMARIES`] and [`D65_WHITE`].
pub fn xyz_to_linear_srgb_matrix() -> Mat3 {
    let m = rgb_to_xyz_matrix(&SRGB_PRIMARIES, &D65_WHITE).expect("sRGB primaries are linearly independent");
    mat3_inverse(&m).expect("the sRGB RGB→XYZ matrix is invertible")
}

/// Converts XYZ (with the white point at `Y = 1`) to linear sRGB. Out-of-gamut colours give negative or > 1
/// components; clamping belongs to the display pass (`docs/PRD.md` §6.6).
pub fn xyz_to_linear_srgb(xyz: &[f64; 3]) -> [f64; 3] {
    mat3_vec(&xyz_to_linear_srgb_matrix(), xyz)
}

/// sRGB transfer function (linear → encoded), IEC 61966-2-1: `12.92 c` for `c ≤ 0.0031308`, else
/// `1.055 c^{1/2.4} − 0.055`. Input is clamped to `[0, 1]`.
pub fn srgb_encode(c: f64) -> f64 {
    let c = c.clamp(0.0, 1.0);
    if c <= 0.003_130_8 {
        12.92 * c
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    }
}

/// Inverse sRGB transfer function (encoded → linear). Input is clamped to `[0, 1]`.
pub fn srgb_decode(e: f64) -> f64 {
    let e = e.clamp(0.0, 1.0);
    if e <= 0.040_45 {
        e / 12.92
    } else {
        ((e + 0.055) / 1.055).powf(2.4)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_relative_eq;

    #[test]
    fn d65_white_maps_to_unit_rgb() {
        let w = xyz_from_chromaticity_sum(&D65_WHITE, 1.0 / D65_WHITE[1]);
        let rgb = xyz_to_linear_srgb(&w);
        for c in rgb {
            assert_relative_eq!(c, 1.0, max_relative = 1e-12);
        }
    }

    #[test]
    fn primaries_map_to_unit_vectors() {
        let m = rgb_to_xyz_matrix(&SRGB_PRIMARIES, &D65_WHITE).unwrap();
        for (i, p) in SRGB_PRIMARIES.iter().enumerate() {
            let col = [m[0][i], m[1][i], m[2][i]];
            let xy = chromaticity(&col).unwrap();
            assert_relative_eq!(xy[0], p[0], max_relative = 1e-12);
            assert_relative_eq!(xy[1], p[1], max_relative = 1e-12);
        }
    }

    #[test]
    fn matrix_matches_published_srgb() {
        // IEC 61966-2-1:1999 publishes the XYZ → sRGB matrix to 4 decimals; the standard rounded its own matrix,
        // so agreement is to a few 1e-4.
        let published = [
            [3.2406, -1.5372, -0.4986],
            [-0.9689, 1.8758, 0.0415],
            [0.0557, -0.2040, 1.0570],
        ];
        let m = xyz_to_linear_srgb_matrix();
        for i in 0..3 {
            for j in 0..3 {
                assert!(
                    (m[i][j] - published[i][j]).abs() < 5e-4,
                    "[{i}][{j}] = {}",
                    m[i][j]
                );
            }
        }
    }

    #[test]
    fn transfer_function() {
        // Reference points: encode(0.5) = 0.735357 (1.055·0.5^{1/2.4} − 0.055), linear segment, end points.
        assert_relative_eq!(
            srgb_encode(0.5),
            1.055 * 0.5_f64.powf(1.0 / 2.4) - 0.055,
            max_relative = 1e-15
        );
        assert_relative_eq!(srgb_encode(0.5), 0.735_357, max_relative = 1e-6);
        assert_relative_eq!(srgb_encode(0.001), 0.012_92, max_relative = 1e-12);
        assert_relative_eq!(srgb_encode(1.0), 1.0, max_relative = 1e-15);
        assert_relative_eq!(srgb_encode(-1.0), 0.0);
        // The two segments meet within 1e-4 at the breakpoint (they are not exactly continuous by design).
        assert!((12.92 * 0.003_130_8 - (1.055 * 0.003_130_8_f64.powf(1.0 / 2.4) - 0.055)).abs() < 1e-4);
        for i in 0..=100 {
            let c = f64::from(i) / 100.0;
            assert!((srgb_decode(srgb_encode(c)) - c).abs() < 1e-12);
        }
    }

    #[test]
    fn inverse_and_chromaticity_helpers() {
        let m = [[2.0, 1.0, 0.0], [0.0, 3.0, 1.0], [1.0, 0.0, 1.0]];
        let inv = mat3_inverse(&m).unwrap();
        for i in 0..3 {
            let v = mat3_vec(&m, &[inv[0][i], inv[1][i], inv[2][i]]);
            for j in 0..3 {
                assert_relative_eq!(v[j], if i == j { 1.0 } else { 0.0 }, epsilon = 1e-15);
            }
        }
        assert!(mat3_inverse(&[[1.0, 2.0, 3.0], [2.0, 4.0, 6.0], [0.0, 0.0, 1.0]]).is_none());
        assert!(chromaticity(&[0.0; 3]).is_none());
        let xyz = [0.3, 0.5, 0.2];
        let xy = chromaticity(&xyz).unwrap();
        let back = xyz_from_chromaticity_sum(&xy, 1.0);
        for c in 0..3 {
            assert_relative_eq!(back[c], xyz[c], max_relative = 1e-15);
        }
    }
}
