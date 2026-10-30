//! Diffuse "Milky Way" band: an exponential latitude profile around the galactic plane modulated by
//! deterministic fractal value noise, with its own effective temperature (`docs/PRD.md` §6.2).

use crate::catalogue::galactic_latitude;
use crate::rng::mix64;

/// Parameters of the diffuse band. Its filling factor at direction `d` is
/// `w(d) = amplitude · exp(−|b|/scale) · (1 − noise_amount + noise_amount · fbm(frequency · d))`, with `fbm ∈ [0, 1]`.
#[derive(Clone, Debug, PartialEq)]
pub struct DiffuseBandParams {
    /// Noise seed.
    pub seed: u64,
    /// Filling factor `w` on the plane (before noise).
    pub amplitude: f64,
    /// Latitude scale `h` (rad).
    pub scale: f64,
    /// Effective temperature of the band (K).
    pub temperature: f64,
    /// Noise modulation depth in `[0, 1]`.
    pub noise_amount: f64,
    /// Base noise frequency (lattice cells per unit length on the unit sphere).
    pub frequency: f64,
    /// Number of fBm octaves (gain 1/2, lacunarity 2).
    pub octaves: u32,
}

impl Default for DiffuseBandParams {
    /// A patchy band: `w = 5e-3` on the plane (chosen by eye with `examples/sky_preview.rs`: clearly visible but
    /// not dominating the default star field, whose mean filling factor is ≈ 0.01), `h = 0.12 rad`, `T = 4500 K`.
    fn default() -> Self {
        Self {
            seed: 7,
            amplitude: 5e-3,
            scale: 0.12,
            temperature: 4500.0,
            noise_amount: 0.7,
            frequency: 3.0,
            octaves: 5,
        }
    }
}

impl DiffuseBandParams {
    /// Filling factor `w` at unit direction `dir` for galactic pole `normal`.
    pub fn filling_factor(&self, dir: &[f64; 3], normal: &[f64; 3]) -> f64 {
        let b = galactic_latitude(dir, normal);
        let profile = (-b.abs() / self.scale).exp();
        let p = dir.map(|c| c * self.frequency);
        let noise = fbm(self.seed, &p, self.octaves);
        self.amplitude * profile * (1.0 - self.noise_amount + self.noise_amount * noise)
    }
}

/// Lattice value in `[0, 1)` for integer cell `(i, j, k)`.
fn lattice(seed: u64, i: i64, j: i64, k: i64) -> f64 {
    let h = mix64(seed ^ mix64((i as u64) ^ mix64((j as u64) ^ mix64(k as u64))));
    (h >> 11) as f64 * (1.0 / (1_u64 << 53) as f64)
}

/// Trilinear value noise with smoothstep fade, in `[0, 1)`.
pub fn value_noise(seed: u64, p: &[f64; 3]) -> f64 {
    let fl = p.map(f64::floor);
    let f = [0, 1, 2].map(|i| {
        let t = p[i] - fl[i];
        t * t * (3.0 - 2.0 * t)
    });
    let c = fl.map(|v| v as i64);
    let mut acc = 0.0;
    for corner in 0..8 {
        let (dx, dy, dz) = (corner & 1, (corner >> 1) & 1, (corner >> 2) & 1);
        let w = (if dx == 1 { f[0] } else { 1.0 - f[0] })
            * (if dy == 1 { f[1] } else { 1.0 - f[1] })
            * (if dz == 1 { f[2] } else { 1.0 - f[2] });
        acc += w * lattice(seed, c[0] + dx, c[1] + dy, c[2] + dz);
    }
    acc
}

/// Fractal Brownian motion: normalised sum of `octaves` value-noise octaves, in `[0, 1)`.
pub fn fbm(seed: u64, p: &[f64; 3], octaves: u32) -> f64 {
    let (mut sum, mut norm, mut amp, mut freq) = (0.0, 0.0, 1.0, 1.0);
    for o in 0..octaves.max(1) {
        sum += amp * value_noise(seed.wrapping_add(u64::from(o)), &p.map(|c| c * freq));
        norm += amp;
        amp *= 0.5;
        freq *= 2.0;
    }
    sum / norm
}

#[cfg(test)]
mod tests {
    use super::*;

    const POLE: [f64; 3] = [0.0, 0.0, 1.0];

    #[test]
    fn noise_is_bounded_deterministic_and_continuous() {
        let mut prev = value_noise(3, &[0.0, 0.37, -1.2]);
        for i in 1..2000 {
            let p = [f64::from(i) * 0.0013, 0.37, -1.2];
            let v = value_noise(3, &p);
            assert!((0.0..1.0).contains(&v));
            // Lipschitz: smoothstep slope ≤ 1.5 per unit per axis, so |Δ| ≤ 1.5 · 0.0013.
            assert!((v - prev).abs() <= 1.5 * 0.0013 + 1e-12);
            prev = v;
            assert!((value_noise(3, &p) - v).abs() < 1e-300);
        }
        // At lattice points the noise equals the lattice value.
        assert!((value_noise(9, &[2.0, -3.0, 5.0]) - lattice(9, 2, -3, 5)).abs() < 1e-15);
        let f = fbm(1, &[0.3, 0.4, 0.5], 5);
        assert!((0.0..1.0).contains(&f));
    }

    #[test]
    fn band_profile() {
        let p = DiffuseBandParams::default();
        // Bounds: w ≤ amplitude everywhere, ≥ amplitude (1 − noise_amount) on the plane.
        let on_plane = p.filling_factor(&[1.0, 0.0, 0.0], &POLE);
        assert!(on_plane <= p.amplitude && on_plane >= p.amplitude * (1.0 - p.noise_amount));
        let pole = p.filling_factor(&[0.0, 0.0, 1.0], &POLE);
        assert!(pole <= p.amplitude * (-std::f64::consts::FRAC_PI_2 / p.scale).exp() * (1.0 + 1e-12));
        let none = DiffuseBandParams {
            noise_amount: 0.0,
            ..p.clone()
        };
        let b: f64 = 0.2;
        let d = [b.cos(), 0.0, b.sin()];
        assert!((none.filling_factor(&d, &POLE) - none.amplitude * (-b / none.scale).exp()).abs() < 1e-15);
    }
}
