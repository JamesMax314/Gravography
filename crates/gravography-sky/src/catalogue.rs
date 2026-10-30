//! Procedural star catalogue, deterministic from a `u64` seed (`docs/PRD.md` §6.2; `docs/physics.md`
//! § "Procedural star field").
//!
//! - Temperatures from a spectral-class mixture dominated by cool K/M stars, log-uniform within each class.
//! - Apparent brightness from a truncated power law `N(> F) ∝ F^{−α}` (`α = 3/2` for a uniform population in
//!   Euclidean space), with `F` the flux in the band's channel 1 (`Y` for human vision).
//! - Positions isotropic, plus a fraction concentrated toward a "galactic plane" with an exponential latitude
//!   profile.
//!
//! A star is a blackbody of temperature `T` subtending solid angle `Ω_*`, so its specific flux is `Ω_* B_ν(T)` and
//! `Ω_* = F / Y_BB(T)`.

use crate::BlackbodyLut;
use crate::rng::SplitMix64;

/// One star: a blackbody disc at infinity.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Star {
    /// Unit direction toward the star (KS Cartesian axes; `+z` is the spin axis).
    pub dir: [f64; 3],
    /// Effective temperature (K).
    pub temperature: f64,
    /// Solid angle `Ω_*` (sr): `I_ν = B_ν(T)` over the disc.
    pub solid_angle: f64,
}

/// Spectral classes: `(name, fraction, T_min, T_max)` (K). Fractions are those of a magnitude-limited field
/// dominated by K/M stars, with few hot O/B stars (PRD §6.2 asks only for "roughly realistic").
pub const SPECTRAL_CLASSES: [(&str, f64, f64, f64); 7] = [
    ("O", 0.005, 30_000.0, 50_000.0),
    ("B", 0.035, 10_000.0, 30_000.0),
    ("A", 0.06, 7_500.0, 10_000.0),
    ("F", 0.08, 6_000.0, 7_500.0),
    ("G", 0.12, 5_200.0, 6_000.0),
    ("K", 0.25, 3_700.0, 5_200.0),
    ("M", 0.45, 2_400.0, 3_700.0),
];

/// Catalogue parameters.
#[derive(Clone, Debug, PartialEq)]
pub struct StarParams {
    /// PRNG seed.
    pub seed: u64,
    /// Number of stars.
    pub count: usize,
    /// Faintest flux `F_min` (arbitrary units of the band's channel 1, W m⁻² for `Y`).
    pub flux_min: f64,
    /// Brightest flux as a multiple of `flux_min` (`10⁴` = 10 magnitudes).
    pub flux_range: f64,
    /// Power-law index `α` of `N(> F) ∝ F^{−α}`.
    pub flux_slope: f64,
    /// Fraction of stars drawn from the galactic-plane population (the rest are isotropic).
    pub galactic_fraction: f64,
    /// Scale of the exponential galactic latitude profile `∝ exp(−|b|/h)` (rad).
    pub galactic_scale: f64,
}

impl Default for StarParams {
    /// 2·10⁵ stars, 10 magnitudes, `α = 1.5`, 60 % in a disc with `h = 0.15 rad`. `flux_min = 0.01` makes a faint
    /// star fill ≈ 1–2 % of a 2048² texel.
    fn default() -> Self {
        Self {
            seed: 1,
            count: 200_000,
            flux_min: 0.01,
            flux_range: 1e4,
            flux_slope: 1.5,
            galactic_fraction: 0.6,
            galactic_scale: 0.15,
        }
    }
}

/// Orthonormal frame `(e₁, e₂, n)` with `n` the normalised galactic pole.
pub fn galactic_frame(normal: &[f64; 3]) -> [[f64; 3]; 3] {
    let n = normalise(normal);
    let helper = if n[0].abs() < 0.9 {
        [1.0, 0.0, 0.0]
    } else {
        [0.0, 1.0, 0.0]
    };
    let e1 = normalise(&cross(&helper, &n));
    let e2 = cross(&n, &e1);
    [e1, e2, n]
}

/// Galactic latitude `b = asin(d · n)` of the unit direction `d`.
pub fn galactic_latitude(dir: &[f64; 3], normal: &[f64; 3]) -> f64 {
    let n = normalise(normal);
    (dir[0] * n[0] + dir[1] * n[1] + dir[2] * n[2])
        .clamp(-1.0, 1.0)
        .asin()
}

pub(crate) fn normalise(v: &[f64; 3]) -> [f64; 3] {
    let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    [v[0] / l, v[1] / l, v[2] / l]
}

fn cross(a: &[f64; 3], b: &[f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

/// Generates the catalogue. `lut` converts flux to solid angle via its channel 1; `galactic_normal` is the pole of
/// the galactic plane. Identical inputs give bit-identical output.
pub fn generate(params: &StarParams, lut: &BlackbodyLut, galactic_normal: &[f64; 3]) -> Vec<Star> {
    let mut rng = SplitMix64::new(params.seed);
    let frame = galactic_frame(galactic_normal);
    // Truncated power law: N(> F) ∝ F^{−α} on [F_min, R F_min] ⇒ F = F_min v^{−1/α}, v uniform on [R^{−α}, 1].
    let v_min = params.flux_range.powf(-params.flux_slope);
    (0..params.count)
        .map(|_| {
            let dir = if rng.next_f64() < params.galactic_fraction {
                galactic_direction(&mut rng, &frame, params.galactic_scale)
            } else {
                isotropic_direction(&mut rng)
            };
            let temperature = sample_temperature(&mut rng);
            let v = v_min + (1.0 - v_min) * rng.next_f64_open0();
            let flux = params.flux_min * v.powf(-1.0 / params.flux_slope);
            let y = lut.lookup(temperature).response[1];
            Star {
                dir,
                temperature,
                solid_angle: flux / y,
            }
        })
        .collect()
}

/// Uniform direction on the sphere (`z = 2u − 1`, `φ = 2πv`).
fn isotropic_direction(rng: &mut SplitMix64) -> [f64; 3] {
    let z = 2.0 * rng.next_f64() - 1.0;
    let phi = 2.0 * std::f64::consts::PI * rng.next_f64();
    let s = (1.0 - z * z).max(0.0).sqrt();
    [s * phi.cos(), s * phi.sin(), z]
}

/// Direction with uniform galactic longitude and latitude `|b| ~ Exp(1/h)` (redrawn if `|b| ≥ π/2`).
fn galactic_direction(rng: &mut SplitMix64, frame: &[[f64; 3]; 3], h: f64) -> [f64; 3] {
    let half_pi = std::f64::consts::FRAC_PI_2;
    let mut b = 0.0;
    for _ in 0..64 {
        let cand = -h * rng.next_f64_open0().ln();
        if cand < half_pi {
            b = cand;
            break;
        }
    }
    if rng.next_f64() < 0.5 {
        b = -b;
    }
    let l = 2.0 * std::f64::consts::PI * rng.next_f64();
    let (cb, sb) = (b.cos(), b.sin());
    let [e1, e2, n] = frame;
    [0, 1, 2].map(|i| cb * (l.cos() * e1[i] + l.sin() * e2[i]) + sb * n[i])
}

fn sample_temperature(rng: &mut SplitMix64) -> f64 {
    let u = rng.next_f64();
    let mut acc = 0.0;
    let mut class = SPECTRAL_CLASSES[SPECTRAL_CLASSES.len() - 1];
    for c in SPECTRAL_CLASSES {
        acc += c.1;
        if u < acc {
            class = c;
            break;
        }
    }
    let (lo, hi) = (class.2, class.3);
    lo * (hi / lo).powf(rng.next_f64())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::OnceLock;

    fn lut() -> &'static BlackbodyLut {
        static L: OnceLock<BlackbodyLut> = OnceLock::new();
        L.get_or_init(BlackbodyLut::human_vision)
    }

    fn params(n: usize) -> StarParams {
        StarParams {
            count: n,
            ..StarParams::default()
        }
    }

    const POLE: [f64; 3] = [0.0, 0.866_025_403_784_438_6, 0.5];

    #[test]
    fn deterministic_for_a_seed() {
        let a = generate(&params(2000), lut(), &POLE);
        let b = generate(&params(2000), lut(), &POLE);
        assert_eq!(a, b);
        let c = generate(
            &StarParams {
                seed: 2,
                ..params(2000)
            },
            lut(),
            &POLE,
        );
        assert_ne!(a, c);
    }

    #[test]
    fn class_fractions_and_temperature_ranges() {
        // n = 10⁵: each class fraction within 5σ = 5 sqrt(p(1 − p)/n) of its target.
        let n = 100_000;
        let stars = generate(&params(n), lut(), &POLE);
        for (name, p, lo, hi) in SPECTRAL_CLASSES {
            let k = stars
                .iter()
                .filter(|s| s.temperature >= lo && s.temperature < hi)
                .count();
            let f = k as f64 / n as f64;
            let sigma = (p * (1.0 - p) / n as f64).sqrt();
            assert!((f - p).abs() < 5.0 * sigma + 1e-12, "class {name}: {f} vs {p}");
        }
        assert!(stars.iter().all(|s| (2_400.0..50_000.0).contains(&s.temperature)));
    }

    #[test]
    fn flux_follows_the_power_law() {
        // Fraction with F > 10 F_min should be (10^{−α} − R^{−α}) / (1 − R^{−α}) for the truncated law; 5σ bound.
        let n = 100_000;
        let p = params(n);
        let stars = generate(&p, lut(), &POLE);
        let flux = |s: &Star| s.solid_angle * lut().lookup(s.temperature).response[1];
        let above = stars.iter().filter(|s| flux(s) > 10.0 * p.flux_min).count() as f64 / n as f64;
        let rmin = p.flux_range.powf(-p.flux_slope);
        let expect = (10f64.powf(-p.flux_slope) - rmin) / (1.0 - rmin);
        let sigma = (expect * (1.0 - expect) / n as f64).sqrt();
        assert!((above - expect).abs() < 5.0 * sigma, "{above} vs {expect}");
        for s in &stars {
            let f = flux(s);
            assert!(f >= p.flux_min * (1.0 - 1e-9) && f <= p.flux_min * p.flux_range * (1.0 + 1e-9));
        }
    }

    #[test]
    fn directions_are_unit_and_concentrated_toward_the_plane() {
        // Isotropic stars have |sin b| < 0.1 with probability 0.1; the disc population (h = 0.15) mostly lies there:
        // P(|b| < 0.1) = 1 − e^{−0.1/0.15} ≈ 0.49 (sin b ≈ b). Expected total ≈ 0.4·0.1 + 0.6·0.49 ≈ 0.33.
        let n = 50_000;
        let stars = generate(&params(n), lut(), &POLE);
        for s in &stars {
            let l = s.dir.iter().map(|c| c * c).sum::<f64>();
            assert!((l - 1.0).abs() < 1e-12);
        }
        let near = stars
            .iter()
            .filter(|s| galactic_latitude(&s.dir, &POLE).sin().abs() < 0.1)
            .count() as f64
            / n as f64;
        let b_lim = 0.1_f64.asin();
        let expect = 0.4 * 0.1 + 0.6 * (1.0 - (-b_lim / 0.15).exp());
        assert!((near - expect).abs() < 0.01, "{near} vs {expect}");
        // Purely isotropic: mean direction ≈ 0.
        let iso = generate(
            &StarParams {
                galactic_fraction: 0.0,
                ..params(n)
            },
            lut(),
            &POLE,
        );
        let mean = [0, 1, 2].map(|i| iso.iter().map(|s| s.dir[i]).sum::<f64>() / n as f64);
        // σ of each mean component = sqrt(1/3 / n) ≈ 2.6e-3.
        assert!(mean.iter().all(|m| m.abs() < 0.013), "{mean:?}");
    }

    #[test]
    fn galactic_frame_is_orthonormal_and_right_handed() {
        for n in [POLE, [0.0, 0.0, 1.0], [1.0, 0.0, 0.0], [0.3, -0.2, 0.9]] {
            let [e1, e2, nn] = galactic_frame(&n);
            let dot = |a: &[f64; 3], b: &[f64; 3]| a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
            assert!(
                dot(&e1, &e2).abs() < 1e-15 && dot(&e1, &nn).abs() < 1e-15 && dot(&e2, &nn).abs() < 1e-15
            );
            assert!((dot(&e1, &e1) - 1.0).abs() < 1e-15 && (dot(&e2, &e2) - 1.0).abs() < 1e-15);
            assert!((dot(&cross(&e1, &e2), &nn) - 1.0).abs() < 1e-15);
        }
    }
}
