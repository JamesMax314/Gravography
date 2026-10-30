//! Orthonormal tetrads `{e_(0) = u, e_(1), e_(2), e_(3)}` with `g(e_(a), e_(b)) = η_ab` (`docs/PRD.md` §5.4).
//!
//! Spatial legs are labelled forward (1), left (2) and up (3) and must form a right-handed triad
//! (`docs/PRD.md` §5.4). [`Tetrad::gram_schmidt`] builds one from four seed vectors. WP-04 uses the same routine
//! for re-orthonormalisation along the observer worldline.

use crate::Real;
use crate::linalg::{Mat4, Vec4, inner};

/// An orthonormal tetrad: `e_up[a][μ]` is the contravariant component `μ` of leg `a`.
/// Leg 0 is the observer 4-velocity; legs 1, 2, 3 are forward, left, up.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tetrad<T> {
    /// `e_up[a][μ]`, leg index first.
    pub e_up: [Vec4<T>; 4],
}

impl<T: Real> Tetrad<T> {
    /// Gram–Schmidt orthonormalisation with respect to `g_lo` of the seeds `(u, forward, left, up)`.
    ///
    /// Keeps `e_(0) ∥ u` (normalised to `g(u, u) = −1`), then removes from each spatial seed its components
    /// along the previous legs: `v ← v − Σ_b η_bb g(v, e_b) e_b`. Returns `None` if `u` is not
    /// future-or-past timelike or a seed becomes (numerically) degenerate.
    pub fn gram_schmidt(g_lo: &Mat4<T>, seeds: &[Vec4<T>; 4]) -> Option<Self> {
        let uu = inner(g_lo, &seeds[0], &seeds[0]);
        if uu >= T::zero() || !uu.is_finite() {
            return None;
        }
        let mut e = [[T::zero(); 4]; 4];
        let inv = T::one() / (-uu).sqrt();
        e[0] = seeds[0].map(|c| c * inv);
        for a in 1..4 {
            let mut v = seeds[a];
            // Twice ("Gram–Schmidt twice is enough") for robustness against cancellation.
            for _ in 0..2 {
                for b in 0..a {
                    let eta_bb = if b == 0 { -T::one() } else { T::one() };
                    let c = eta_bb * inner(g_lo, &v, &e[b]);
                    for mu in 0..4 {
                        v[mu] = v[mu] - c * e[b][mu];
                    }
                }
            }
            let vv = inner(g_lo, &v, &v);
            let seed_norm = inner(g_lo, &seeds[a], &seeds[a]).abs();
            // Degenerate if the remaining spacelike part is tiny compared with the seed.
            if vv.is_nan() || vv <= T::lit(1e-24) * seed_norm.max(T::one()) {
                return None;
            }
            let inv = T::one() / vv.sqrt();
            e[a] = v.map(|c| c * inv);
        }
        Some(Self { e_up: e })
    }

    /// Observer 4-velocity `u^μ = e_(0)^μ`.
    pub fn u(&self) -> Vec4<T> {
        self.e_up[0]
    }

    /// Contravariant vector `Σ_a c^a e_(a)` from tetrad components `c = (c⁰, c¹, c², c³)`.
    pub fn to_coordinate(&self, c: &Vec4<T>) -> Vec4<T> {
        core::array::from_fn(|mu| (0..4).fold(T::zero(), |s, a| s + c[a] * self.e_up[a][mu]))
    }

    /// Tetrad with the spatial legs rotated by the SO(3) matrix `rot` (`e_(0)` unchanged):
    /// `e'_(i) = Σ_j rot[j][i] e_(j)`, i.e. column `i` of `rot` holds the new leg `i` in the old frame
    /// (forward, left, up). Used for the user look rotation (`docs/PRD.md` §5.8), which is not a physical
    /// rotation. A vector with components `v'` in the rotated frame has components `rot · v'` in the original one.
    pub fn rotated(&self, rot: &[[T; 3]; 3]) -> Self {
        let mut e = self.e_up;
        for i in 0..3 {
            e[i + 1] = core::array::from_fn(|mu| {
                (0..3).fold(T::zero(), |s, j| s + rot[j][i] * self.e_up[j + 1][mu])
            });
        }
        Self { e_up: e }
    }

    /// Tetrad of an observer moving with velocity `beta = βⁱ` (units of `c`, components along forward, left, up)
    /// relative to this one: a pure Lorentz boost (`docs/physics.md` § "Boosted observers"),
    /// `e'_(0) = γ (e_(0) + β n̂ⁱ e_(i))`, `e'_(j) = e_(j) + n̂_j [(γ − 1) n̂ⁱ e_(i) + γ β e_(0)]`.
    /// The new spatial legs are the old ones carried along by the boost, so the view keeps its orientation
    /// (directions perpendicular to the motion are unchanged). Returns `None` unless `|β| < 1`.
    pub fn boosted(&self, beta: &[T; 3]) -> Option<Self> {
        let b2 = beta[0] * beta[0] + beta[1] * beta[1] + beta[2] * beta[2];
        if b2.is_nan() || b2 >= T::one() {
            return None;
        }
        if b2 <= T::zero() {
            return Some(*self);
        }
        let b = b2.sqrt();
        let gamma = T::one() / (T::one() - b2).sqrt();
        let n = beta.map(|c| c / b);
        let e = &self.e_up;
        // Σ_i n̂ⁱ e_(i)
        let along: Vec4<T> =
            core::array::from_fn(|mu| (0..3).fold(T::zero(), |s, i| s + n[i] * e[i + 1][mu]));
        let mut out = *e;
        out[0] = core::array::from_fn(|mu| gamma * (e[0][mu] + b * along[mu]));
        for j in 0..3 {
            out[j + 1] = core::array::from_fn(|mu| {
                e[j + 1][mu] + n[j] * ((gamma - T::one()) * along[mu] + gamma * b * e[0][mu])
            });
        }
        Some(Self { e_up: out })
    }

    /// Largest deviation `|g(e_a, e_b) − η_ab|` (diagnostic for re-orthonormalisation).
    pub fn orthonormality_error(&self, g_lo: &Mat4<T>) -> T {
        let mut worst = T::zero();
        for a in 0..4 {
            for b in 0..4 {
                let eta = match (a, b) {
                    (0, 0) => -T::one(),
                    _ if a == b => T::one(),
                    _ => T::zero(),
                };
                worst = worst.max((inner(g_lo, &self.e_up[a], &self.e_up[b]) - eta).abs());
            }
        }
        worst
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Kerr;
    use crate::linalg::eta;
    use crate::test_util::event;
    use proptest::prelude::*;

    #[test]
    fn minkowski_identity_tetrad() {
        let g = eta::<f64>();
        let seeds = [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ];
        let t = Tetrad::gram_schmidt(&g, &seeds).unwrap();
        assert_eq!(t.e_up, seeds);
    }

    #[test]
    fn rejects_non_timelike_velocity() {
        let g = eta::<f64>();
        let seeds = [
            [1.0, 2.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ];
        assert!(Tetrad::gram_schmidt(&g, &seeds).is_none());
        let degenerate = [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 2.0, 0.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ];
        assert!(Tetrad::gram_schmidt(&g, &degenerate).is_none());
    }

    #[test]
    fn rotation_keeps_orthonormality_and_handedness() {
        // Kerr a = 0.9 static observer at r ≈ 12; rotate by 40° about "up", then 25° about "forward".
        let k = Kerr::new(0.9_f64).unwrap();
        let pos = [0.0, 7.0, 6.0, 8.0];
        let g = k.g_lo(&pos);
        let seeds = [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0, 0.0],
            [0.0, 0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ];
        let t = Tetrad::gram_schmidt(&g, &seeds).unwrap();
        let (s1, c1) = 40.0_f64.to_radians().sin_cos();
        let (s2, c2) = 25.0_f64.to_radians().sin_cos();
        let yaw = [[c1, -s1, 0.0], [s1, c1, 0.0], [0.0, 0.0, 1.0]];
        let roll = [[1.0, 0.0, 0.0], [0.0, c2, -s2], [0.0, s2, c2]];
        let rot: [[f64; 3]; 3] =
            core::array::from_fn(|i| core::array::from_fn(|j| (0..3).map(|k| yaw[i][k] * roll[k][j]).sum()));
        let r = t.rotated(&rot);
        assert!(r.orthonormality_error(&g) < 1e-13);
        assert_eq!(r.e_up[0], t.e_up[0]);
        // Orientation preserved: det e_up has the same sign (positive for a right-handed, future tetrad).
        assert!(crate::linalg::determinant(&t.e_up) > 0.0);
        assert!(crate::linalg::determinant(&r.e_up) > 0.0);
        // New forward leg = cos40° forward + sin40° left (column 0 of rot).
        let expect = t.to_coordinate(&[0.0, rot[0][0], rot[1][0], rot[2][0]]);
        for mu in 0..4 {
            assert!((r.e_up[1][mu] - expect[mu]).abs() < 1e-14);
        }
    }

    #[test]
    fn boost_gives_relativistic_doppler_in_flat_space() {
        // M = 0 limit via the Minkowski tetrad: an observer moving forward with speed β sees a source straight
        // ahead (direction n = forward) blueshifted by g = sqrt((1 + β)/(1 − β)) and one behind redshifted by the
        // inverse (special-relativistic Doppler, e.g. Rindler, "Relativity: Special, General, and Cosmological"
        // §4.3). Here g = 1/p_t for p = η(−e'_(0) + n e'_(i)) (`docs/PRD.md` §5.7).
        let g = eta::<f64>();
        let t = Tetrad::gram_schmidt(&g, &crate::linalg::identity()).unwrap();
        let beta = 0.6;
        let b = t.boosted(&[beta, 0.0, 0.0]).unwrap();
        assert!(b.orthonormality_error(&g) < 1e-15);
        let shift = |n: [f64; 3]| {
            let k = b.to_coordinate(&[-1.0, n[0], n[1], n[2]]);
            1.0 / crate::linalg::lower(&g, &k)[0]
        };
        let expect = ((1.0 + beta) / (1.0 - beta)).sqrt();
        assert!((shift([1.0, 0.0, 0.0]) - expect).abs() < 1e-14);
        assert!((shift([-1.0, 0.0, 0.0]) - 1.0 / expect).abs() < 1e-14);
        // Perpendicular legs are untouched by a boost along forward.
        assert_eq!(b.e_up[2], t.e_up[2]);
        assert_eq!(b.e_up[3], t.e_up[3]);
        assert!(t.boosted(&[0.8, 0.6, 0.0]).is_none());
        assert_eq!(t.boosted(&[0.0; 3]), Some(t));
    }

    proptest! {
        #[test]
        fn boosted_tetrad_is_orthonormal_in_kerr((a, pos) in event(), v in proptest::array::uniform3(-0.55..0.55_f64)) {
            // |β| < 0.96 here, γ ≤ 3.6; the boost mixes legs with weights ≤ γ², so errors stay at a few ε · γ² · |g|.
            let k = Kerr::new(a).unwrap();
            let g = k.g_lo(&pos);
            let g_up = k.g_up(&pos);
            let n = [-g_up[0][0], -g_up[1][0], -g_up[2][0], -g_up[3][0]];
            let z = 0.0;
            let seeds = [n, [z, 1.0, z, z], [z, z, 1.0, z], [z, z, z, 1.0]];
            let t = Tetrad::gram_schmidt(&g, &seeds).unwrap();
            let b = t.boosted(&v).unwrap();
            prop_assert!(b.orthonormality_error(&g) < 1e-11);
            // e'_(0) has the requested velocity relative to e_(0): γ = −g(u, u').
            let gamma = 1.0 / (1.0 - v.iter().map(|c| c * c).sum::<f64>()).sqrt();
            prop_assert!((-inner(&g, &t.u(), &b.u()) - gamma).abs() < 1e-11 * gamma);
        }

        #[test]
        fn gram_schmidt_is_orthonormal_in_kerr((a, pos) in event(), v in proptest::array::uniform3(-0.5..0.5_f64)) {
            // Observer u = n + w (0, v): n^μ = −g^{μt} is the KS-normal observer (∇t is timelike everywhere in
            // KS, also inside the horizon); w is halved until u is safely timelike (deterministic, no rejects).
            let k = Kerr::new(a).unwrap();
            let g = k.g_lo(&pos);
            let g_up = k.g_up(&pos);
            let n = [-g_up[0][0], -g_up[1][0], -g_up[2][0], -g_up[3][0]];
            let mut w = 1.0;
            let mut u = n;
            for _ in 0..60 {
                u = [n[0], n[1] + w * v[0], n[2] + w * v[1], n[3] + w * v[2]];
                if inner(&g, &u, &u) < 0.5 * inner(&g, &n, &n) {
                    break;
                }
                w *= 0.5;
            }
            let seeds = [u, [0.0, 1.0, 0.0, 0.0], [0.0, 0.0, 1.0, 0.0], [0.0, 0.0, 0.0, 1.0]];
            let t = Tetrad::gram_schmidt(&g, &seeds).unwrap();
            // Entries of g are ~(1 + f) ≲ 8; two Gram–Schmidt passes keep errors at a few ε of that.
            prop_assert!(t.orthonormality_error(&g) < 1e-12);
            // e_(0) is parallel to u with the same time orientation.
            prop_assert!(t.u()[0] > 0.0);
            let ratio = t.u()[1] / t.u()[0];
            prop_assert!((ratio - u[1] / u[0]).abs() < 1e-12 * (1.0 + (u[1] / u[0]).abs()));
        }
    }
}
