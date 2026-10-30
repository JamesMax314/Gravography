//! Cube-map sky storage: face geometry, exact texel solid angles, a flux-conserving mip chain and CPU sampling
//! (`docs/PRD.md` §6.3; `docs/physics.md` § "Sky cube map").
//!
//! **Face convention** (identical to OpenGL, Vulkan, Metal and WebGPU cube textures, so the GPU can sample with the
//! escape direction directly). Faces in order `+X, −X, +Y, −Y, +Z, −Z`. For a direction `d` with major axis `ma`:
//!
//! | face | `sc` | `tc` |
//! |---|---|---|
//! | +X | −z | −y |
//! | −X | +z | −y |
//! | +Y | +x | +z |
//! | −Y | +x | −z |
//! | +Z | +x | −y |
//! | −Z | −x | −y |
//!
//! `u = (sc/|ma| + 1)/2` (column, left to right), `v = (tc/|ma| + 1)/2` (row, top to bottom).
//!
//! Each texel stores `(w, w·T)` as `f32`: the solid-angle filling factor and filling-factor-weighted temperature.

/// Faces of a cube map, in storage order.
pub const FACE_COUNT: usize = 6;

/// Face index, `u` and `v` (both in `[0, 1]`) of the unit (or any non-zero) direction `d`.
pub fn dir_to_face_uv(d: &[f64; 3]) -> (usize, f64, f64) {
    let [x, y, z] = *d;
    let (ax, ay, az) = (x.abs(), y.abs(), z.abs());
    let (face, sc, tc, ma) = if ax >= ay && ax >= az {
        if x >= 0.0 { (0, -z, -y, ax) } else { (1, z, -y, ax) }
    } else if ay >= az {
        if y >= 0.0 { (2, x, z, ay) } else { (3, x, -z, ay) }
    } else if z >= 0.0 {
        (4, x, -y, az)
    } else {
        (5, -x, -y, az)
    };
    (face, 0.5 * (sc / ma + 1.0), 0.5 * (tc / ma + 1.0))
}

/// Unit direction for face `face` at `(u, v) ∈ [0, 1]²` (inverse of [`dir_to_face_uv`]).
pub fn face_uv_to_dir(face: usize, u: f64, v: f64) -> [f64; 3] {
    let (sc, tc) = (2.0 * u - 1.0, 2.0 * v - 1.0);
    let d = match face {
        0 => [1.0, -tc, -sc],
        1 => [-1.0, -tc, sc],
        2 => [sc, 1.0, tc],
        3 => [sc, -1.0, -tc],
        4 => [sc, -tc, 1.0],
        _ => [-sc, -tc, -1.0],
    };
    let l = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
    d.map(|c| c / l)
}

/// Exact solid angle of texel `(col, row)` on a face of `size × size` texels. With face coordinates
/// `a, b ∈ [−1, 1]` and `A(a, b) = atan2(a b, sqrt(a² + b² + 1))`, the solid angle of `[a₀, a₁] × [b₀, b₁]` is
/// `A(a₀, b₀) − A(a₀, b₁) − A(a₁, b₀) + A(a₁, b₁)` (the same for every face).
pub fn texel_solid_angle(size: usize, col: usize, row: usize) -> f64 {
    let s = size as f64;
    let a0 = 2.0 * col as f64 / s - 1.0;
    let a1 = 2.0 * (col + 1) as f64 / s - 1.0;
    let b0 = 2.0 * row as f64 / s - 1.0;
    let b1 = 2.0 * (row + 1) as f64 / s - 1.0;
    let area = |a: f64, b: f64| (a * b).atan2((a * a + b * b + 1.0).sqrt());
    area(a0, b0) - area(a0, b1) - area(a1, b0) + area(a1, b1)
}

/// One mip level: `size × size` texels per face, `[w, w·T]` each, stored face-major then row-major:
/// index `(face · size + row) · size + col`.
#[derive(Clone, Debug, PartialEq)]
pub struct CubeLevel {
    /// Face size in texels.
    pub size: usize,
    /// Texel data `[w, w·T]`.
    pub data: Vec<[f32; 2]>,
}

impl CubeLevel {
    /// Zero-filled level.
    pub fn zeros(size: usize) -> Self {
        Self {
            size,
            data: vec![[0.0; 2]; FACE_COUNT * size * size],
        }
    }

    /// Storage index of a texel.
    pub fn index(&self, face: usize, col: usize, row: usize) -> usize {
        (face * self.size + row) * self.size + col
    }

    /// Total "flux" `Σ w Ω` (sr) and `Σ w T Ω` (sr K) over all texels, accumulated in f64.
    pub fn totals(&self) -> [f64; 2] {
        let n = self.size;
        let omega: Vec<f64> = (0..n * n).map(|k| texel_solid_angle(n, k % n, k / n)).collect();
        let mut acc = [0.0; 2];
        for (k, t) in self.data.iter().enumerate() {
            let o = omega[k % (n * n)];
            acc[0] += f64::from(t[0]) * o;
            acc[1] += f64::from(t[1]) * o;
        }
        acc
    }

    /// Bilinear sample `[w, w·T]` at direction `d`. Texel centres are at `(i + ½)/size`; texel coordinates are
    /// clamped at face edges (no cross-face filtering).
    pub fn sample_bilinear(&self, d: &[f64; 3]) -> [f64; 2] {
        let (face, u, v) = dir_to_face_uv(d);
        let n = self.size;
        let max = (n - 1) as f64;
        let s = (u * n as f64 - 0.5).clamp(0.0, max);
        let t = (v * n as f64 - 0.5).clamp(0.0, max);
        let (c0, r0) = (s.floor() as usize, t.floor() as usize);
        let (c1, r1) = ((c0 + 1).min(n - 1), (r0 + 1).min(n - 1));
        let (fs, ft) = (s - c0 as f64, t - r0 as f64);
        let get = |c, r| {
            let x = self.data[self.index(face, c, r)];
            [f64::from(x[0]), f64::from(x[1])]
        };
        let (a, b, c, e) = (get(c0, r0), get(c1, r0), get(c0, r1), get(c1, r1));
        [0, 1].map(|k| (1.0 - ft) * ((1.0 - fs) * a[k] + fs * b[k]) + ft * ((1.0 - fs) * c[k] + fs * e[k]))
    }

    /// Next coarser level (half size): each parent texel is the solid-angle-weighted mean of its four children,
    /// `w_p = Σ w_c Ω_c / Σ Ω_c` (likewise `w·T`), so `Σ w Ω` is conserved exactly up to rounding.
    pub fn downsample(&self) -> Self {
        let n = self.size;
        let m = n / 2;
        let mut out = Self::zeros(m);
        for face in 0..FACE_COUNT {
            for row in 0..m {
                for col in 0..m {
                    let mut acc = [0.0_f64; 2];
                    let mut omega = 0.0;
                    for (dc, dr) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                        let (c, r) = (2 * col + dc, 2 * row + dr);
                        let o = texel_solid_angle(n, c, r);
                        let x = self.data[self.index(face, c, r)];
                        acc[0] += f64::from(x[0]) * o;
                        acc[1] += f64::from(x[1]) * o;
                        omega += o;
                    }
                    let i = out.index(face, col, row);
                    out.data[i] = [(acc[0] / omega) as f32, (acc[1] / omega) as f32];
                }
            }
        }
        out
    }
}

/// A cube map with its full mip chain (level 0 = finest; the last level is 1×1).
#[derive(Clone, Debug, PartialEq)]
pub struct CubeMap {
    /// Mip levels, finest first.
    pub levels: Vec<CubeLevel>,
}

impl CubeMap {
    /// Builds the mip chain below `base` (whose size must be a power of two).
    pub fn with_mips(base: CubeLevel) -> Self {
        let mut levels = vec![base];
        while levels[levels.len() - 1].size > 1 {
            let next = levels[levels.len() - 1].downsample();
            levels.push(next);
        }
        Self { levels }
    }

    /// Face size of level 0.
    pub fn base_size(&self) -> usize {
        self.levels[0].size
    }

    /// Trilinear sample `[w, w·T]` at direction `d` and level of detail `lod` (0 = finest, clamped to the chain).
    pub fn sample_trilinear(&self, d: &[f64; 3], lod: f64) -> [f64; 2] {
        let max = (self.levels.len() - 1) as f64;
        let l = if lod.is_nan() { 0.0 } else { lod.clamp(0.0, max) };
        let i = (l.floor() as usize).min(self.levels.len() - 1);
        let f = l - i as f64;
        let a = self.levels[i].sample_bilinear(d);
        if f <= 0.0 || i + 1 >= self.levels.len() {
            return a;
        }
        let b = self.levels[i + 1].sample_bilinear(d);
        [0, 1].map(|k| (1.0 - f) * a[k] + f * b[k])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    #[test]
    fn solid_angles_sum_to_4pi() {
        for n in [1, 2, 7, 64] {
            let total: f64 = (0..n)
                .flat_map(|r| (0..n).map(move |c| texel_solid_angle(n, c, r)))
                .sum::<f64>()
                * 6.0;
            assert!(
                (total - 4.0 * std::f64::consts::PI).abs() < 1e-12,
                "n = {n}: {total}"
            );
        }
        // 1×1 face: 4π/6.
        assert!((texel_solid_angle(1, 0, 0) - 4.0 * std::f64::consts::PI / 6.0).abs() < 1e-14);
    }

    #[test]
    fn face_centres_and_known_orientations() {
        // Centres: +X, −X, +Y, −Y, +Z, −Z.
        let axes = [
            [1.0, 0.0, 0.0],
            [-1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, -1.0, 0.0],
            [0.0, 0.0, 1.0],
            [0.0, 0.0, -1.0],
        ];
        for (f, a) in axes.iter().enumerate() {
            let (face, u, v) = dir_to_face_uv(a);
            assert_eq!(face, f);
            assert!((u - 0.5).abs() < 1e-15 && (v - 0.5).abs() < 1e-15);
        }
        // +Z face: u grows with +x, v grows with −y (GL convention: sc = +x, tc = −y).
        let (_, u, v) = dir_to_face_uv(&[0.3, 0.2, 1.0]);
        assert!(u > 0.5 && v < 0.5);
        // +X face: u grows with −z.
        let (_, u, _) = dir_to_face_uv(&[1.0, 0.0, -0.4]);
        assert!(u > 0.5);
    }

    proptest! {
        #[test]
        fn face_uv_round_trip(x in -1.0..1.0_f64, y in -1.0..1.0_f64, z in -1.0..1.0_f64) {
            let l = (x * x + y * y + z * z).sqrt();
            prop_assume!(l > 1e-3);
            let d = [x / l, y / l, z / l];
            let (face, u, v) = dir_to_face_uv(&d);
            prop_assert!((0.0..=1.0).contains(&u) && (0.0..=1.0).contains(&v));
            let back = face_uv_to_dir(face, u, v);
            for i in 0..3 {
                prop_assert!((back[i] - d[i]).abs() < 1e-14);
            }
        }
    }

    fn random_level(n: usize, seed: u64) -> CubeLevel {
        let mut rng = crate::rng::SplitMix64::new(seed);
        let mut l = CubeLevel::zeros(n);
        for t in &mut l.data {
            let w = rng.next_f64() as f32;
            *t = [w, w * (3000.0 + 5000.0 * rng.next_f64() as f32)];
        }
        l
    }

    #[test]
    fn mips_conserve_flux() {
        // Σ w Ω is conserved at every level (f32 storage: relative 1e-6 over ≤ 6·32² texels).
        let cube = CubeMap::with_mips(random_level(32, 3));
        assert_eq!(cube.levels.len(), 6);
        assert_eq!(cube.levels[5].size, 1);
        let base = cube.levels[0].totals();
        for l in &cube.levels {
            let t = l.totals();
            assert!((t[0] / base[0] - 1.0).abs() < 1e-6 && (t[1] / base[1] - 1.0).abs() < 1e-6);
        }
    }

    #[test]
    fn sampling() {
        // Constant map → constant samples at any direction and lod; texel centre → that texel's value.
        let mut l = CubeLevel::zeros(8);
        l.data.fill([2.0, 10_000.0]);
        let cube = CubeMap::with_mips(l);
        for d in [[0.3, -0.8, 0.1], [1.0, 1.0, 1.0], [0.0, 0.0, -1.0]] {
            for lod in [0.0, 0.4, 2.5, 10.0] {
                let s = cube.sample_trilinear(&d, lod);
                assert!((s[0] - 2.0).abs() < 1e-6 && (s[1] - 10_000.0).abs() < 1e-2);
            }
        }
        let l = random_level(8, 9);
        let (face, col, row) = (3, 5, 2);
        let d = face_uv_to_dir(face, (col as f64 + 0.5) / 8.0, (row as f64 + 0.5) / 8.0);
        let s = l.sample_bilinear(&d);
        let x = l.data[l.index(face, col, row)];
        assert!((s[0] - f64::from(x[0])).abs() < 1e-6 && (s[1] - f64::from(x[1])).abs() < 1e-3);
    }
}
