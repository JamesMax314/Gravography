//! Minimal fixed-size 4×4 linear algebra on plain arrays, indexed `[t, x, y, z]` (`docs/PRD.md` §2).
//!
//! Plain arrays keep the code generic over [`Real`] and map one-to-one to WGSL later.

use core::array::from_fn;

use crate::Real;

/// A 4-vector or covector, components indexed `[t, x, y, z]` (or `[t, r, θ, φ]` in Boyer–Lindquist).
pub type Vec4<T> = [T; 4];

/// A 4×4 matrix, `m[row][col]`. For a metric, `g[μ][ν] = g_{μν}`.
pub type Mat4<T> = [[T; 4]; 4];

/// Minkowski metric `η = diag(−1, 1, 1, 1)` (its own inverse).
pub fn eta<T: Real>() -> Mat4<T> {
    from_fn(|i| {
        from_fn(|j| match (i, j) {
            (0, 0) => -T::one(),
            _ if i == j => T::one(),
            _ => T::zero(),
        })
    })
}

/// The 4×4 identity matrix.
pub fn identity<T: Real>() -> Mat4<T> {
    from_fn(|i| from_fn(|j| if i == j { T::one() } else { T::zero() }))
}

/// Matrix product `a · b`.
pub fn mat_mul<T: Real>(a: &Mat4<T>, b: &Mat4<T>) -> Mat4<T> {
    from_fn(|i| from_fn(|j| (0..4).fold(T::zero(), |s, k| s + a[i][k] * b[k][j])))
}

/// Matrix–vector product `m · v`, i.e. `(m v)^i = m[i][j] v^j`.
pub fn mat_vec<T: Real>(m: &Mat4<T>, v: &Vec4<T>) -> Vec4<T> {
    from_fn(|i| (0..4).fold(T::zero(), |s, j| s + m[i][j] * v[j]))
}

/// Transpose `mᵀ`.
pub fn transpose<T: Real>(m: &Mat4<T>) -> Mat4<T> {
    from_fn(|i| from_fn(|j| m[j][i]))
}

/// Bilinear form `g_{μν} u^μ v^ν`.
pub fn inner<T: Real>(g_lo: &Mat4<T>, u: &Vec4<T>, v: &Vec4<T>) -> T {
    let gv = mat_vec(g_lo, v);
    (0..4).fold(T::zero(), |s, i| s + u[i] * gv[i])
}

/// Lowers an index: `v_μ = g_{μν} v^ν`.
pub fn lower<T: Real>(g_lo: &Mat4<T>, v_up: &Vec4<T>) -> Vec4<T> {
    mat_vec(g_lo, v_up)
}

/// Raises an index: `p^μ = g^{μν} p_ν`.
pub fn raise<T: Real>(g_up: &Mat4<T>, p_lo: &Vec4<T>) -> Vec4<T> {
    mat_vec(g_up, p_lo)
}

/// 2×2 minors shared by [`determinant`] and [`inverse`] (Laplace expansion along the first two rows).
struct Minors<T> {
    s: [T; 6],
    c: [T; 6],
}

fn minors<T: Real>(m: &Mat4<T>) -> Minors<T> {
    Minors {
        s: [
            m[0][0] * m[1][1] - m[1][0] * m[0][1],
            m[0][0] * m[1][2] - m[1][0] * m[0][2],
            m[0][0] * m[1][3] - m[1][0] * m[0][3],
            m[0][1] * m[1][2] - m[1][1] * m[0][2],
            m[0][1] * m[1][3] - m[1][1] * m[0][3],
            m[0][2] * m[1][3] - m[1][2] * m[0][3],
        ],
        c: [
            m[2][0] * m[3][1] - m[3][0] * m[2][1],
            m[2][0] * m[3][2] - m[3][0] * m[2][2],
            m[2][0] * m[3][3] - m[3][0] * m[2][3],
            m[2][1] * m[3][2] - m[3][1] * m[2][2],
            m[2][1] * m[3][3] - m[3][1] * m[2][3],
            m[2][2] * m[3][3] - m[3][2] * m[2][3],
        ],
    }
}

impl<T: Real> Minors<T> {
    fn det(&self) -> T {
        let (s, c) = (&self.s, &self.c);
        s[0] * c[5] - s[1] * c[4] + s[2] * c[3] + s[3] * c[2] - s[4] * c[1] + s[5] * c[0]
    }
}

/// Determinant via Laplace expansion in 2×2 minors.
pub fn determinant<T: Real>(m: &Mat4<T>) -> T {
    minors(m).det()
}

/// Inverse via the adjugate. Returns `None` if the determinant is zero or not finite.
pub fn inverse<T: Real>(m: &Mat4<T>) -> Option<Mat4<T>> {
    let mn = minors(m);
    let det = mn.det();
    if !det.is_finite() || det.abs() <= T::zero() {
        return None;
    }
    let (s, c) = (&mn.s, &mn.c);
    let adj = [
        [
            m[1][1] * c[5] - m[1][2] * c[4] + m[1][3] * c[3],
            -m[0][1] * c[5] + m[0][2] * c[4] - m[0][3] * c[3],
            m[3][1] * s[5] - m[3][2] * s[4] + m[3][3] * s[3],
            -m[2][1] * s[5] + m[2][2] * s[4] - m[2][3] * s[3],
        ],
        [
            -m[1][0] * c[5] + m[1][2] * c[2] - m[1][3] * c[1],
            m[0][0] * c[5] - m[0][2] * c[2] + m[0][3] * c[1],
            -m[3][0] * s[5] + m[3][2] * s[2] - m[3][3] * s[1],
            m[2][0] * s[5] - m[2][2] * s[2] + m[2][3] * s[1],
        ],
        [
            m[1][0] * c[4] - m[1][1] * c[2] + m[1][3] * c[0],
            -m[0][0] * c[4] + m[0][1] * c[2] - m[0][3] * c[0],
            m[3][0] * s[4] - m[3][1] * s[2] + m[3][3] * s[0],
            -m[2][0] * s[4] + m[2][1] * s[2] - m[2][3] * s[0],
        ],
        [
            -m[1][0] * c[3] + m[1][1] * c[1] - m[1][2] * c[0],
            m[0][0] * c[3] - m[0][1] * c[1] + m[0][2] * c[0],
            -m[3][0] * s[3] + m[3][1] * s[1] - m[3][2] * s[0],
            m[2][0] * s[3] - m[2][1] * s[1] + m[2][2] * s[0],
        ],
    ];
    let inv_det = T::one() / det;
    Some(from_fn(|i| from_fn(|j| adj[i][j] * inv_det)))
}

#[cfg(test)]
pub(crate) mod test_util {
    use super::Mat4;

    /// Largest absolute entry-wise difference between two matrices.
    pub fn max_abs_diff(a: &Mat4<f64>, b: &Mat4<f64>) -> f64 {
        let mut d = 0.0_f64;
        for (ra, rb) in a.iter().zip(b) {
            for (x, y) in ra.iter().zip(rb) {
                d = d.max((x - y).abs());
            }
        }
        d
    }

    /// Largest absolute entry of a matrix.
    pub fn max_abs(a: &Mat4<f64>) -> f64 {
        a.iter().flatten().fold(0.0_f64, |m, x| m.max(x.abs()))
    }
}

#[cfg(test)]
mod tests {
    use super::test_util::max_abs_diff;
    use super::*;
    use approx::assert_relative_eq;
    use proptest::prelude::*;

    // Inversion of well-conditioned random matrices in f64: entries O(1), cond ≲ 1e3 after the
    // diagonal shift below, so errors ≲ 1e3 · 4 · ε ≈ 1e-12.
    const INV_TOL: f64 = 1e-11;

    fn matrix() -> impl Strategy<Value = Mat4<f64>> {
        proptest::array::uniform4(proptest::array::uniform4(-1.0..1.0_f64)).prop_map(|mut m| {
            // Diagonal dominance keeps the matrix comfortably invertible.
            for (i, row) in m.iter_mut().enumerate() {
                row[i] += 5.0;
            }
            m
        })
    }

    #[test]
    fn eta_is_minkowski_and_self_inverse() {
        let e = eta::<f64>();
        assert_relative_eq!(determinant(&e), -1.0);
        assert!(max_abs_diff(&mat_mul(&e, &e), &identity()) < 1e-15);
    }

    #[test]
    fn determinant_of_known_matrix() {
        // Upper-triangular: det = product of the diagonal.
        let m = [
            [2.0, 1.0, 7.0, -3.0],
            [0.0, 3.0, 4.0, 1.0],
            [0.0, 0.0, -1.0, 2.0],
            [0.0, 0.0, 0.0, 5.0],
        ];
        assert_relative_eq!(determinant(&m), -30.0, epsilon = 1e-12);
        // Swapping two rows flips the sign.
        let swapped = [m[1], m[0], m[2], m[3]];
        assert_relative_eq!(determinant(&swapped), 30.0, epsilon = 1e-12);
    }

    #[test]
    fn singular_matrix_has_no_inverse() {
        let m = [
            [1.0, 2.0, 3.0, 4.0],
            [2.0, 4.0, 6.0, 8.0],
            [0.0, 1.0, 0.0, 1.0],
            [1.0, 0.0, 0.0, 0.0],
        ];
        assert!(inverse(&m).is_none());
    }

    #[test]
    fn inner_lower_raise_are_consistent() {
        let g = eta::<f64>();
        let u = [2.0, 1.0, -1.0, 0.5];
        let v = [1.0, 3.0, 0.0, 2.0];
        assert_relative_eq!(inner(&g, &u, &v), -2.0 + 3.0 + 0.0 + 1.0);
        assert_eq!(
            raise(&g, &lower(&g, &u)).map(|c| c.to_bits()),
            u.map(f64::to_bits)
        );
    }

    proptest! {
        #[test]
        fn inverse_is_two_sided(m in matrix()) {
            let inv = inverse(&m).expect("diagonally dominant matrix is invertible");
            prop_assert!(max_abs_diff(&mat_mul(&m, &inv), &identity()) < INV_TOL);
            prop_assert!(max_abs_diff(&mat_mul(&inv, &m), &identity()) < INV_TOL);
        }

        #[test]
        fn determinant_is_multiplicative(a in matrix(), b in matrix()) {
            let lhs = determinant(&mat_mul(&a, &b));
            let rhs = determinant(&a) * determinant(&b);
            // Relative: entries ≲ 6, determinants ≲ 1e6, a few hundred flops.
            prop_assert!((lhs - rhs).abs() <= 1e-12 * rhs.abs());
            prop_assert!((determinant(&transpose(&a)) - determinant(&a)).abs() <= 1e-12 * determinant(&a).abs());
        }
    }
}
