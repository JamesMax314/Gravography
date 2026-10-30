//! Deterministic pseudo-random numbers: SplitMix64 (Steele, Lea & Flood 2014; reference implementation by
//! S. Vigna). Implemented here rather than taken from `rand` so that catalogues are bit-identical for a seed
//! regardless of dependency versions (`docs/PRD.md` NF-3).

/// SplitMix64 generator.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SplitMix64 {
    state: u64,
}

impl SplitMix64 {
    /// Generator seeded with `seed`.
    pub fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    /// Next 64 random bits.
    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        mix64(self.state)
    }

    /// Uniform `f64` in `[0, 1)` with 53 random bits.
    pub fn next_f64(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 * (1.0 / (1_u64 << 53) as f64)
    }

    /// Uniform `f64` in `(0, 1]` (safe for logarithms and negative powers).
    pub fn next_f64_open0(&mut self) -> f64 {
        1.0 - self.next_f64()
    }
}

/// The SplitMix64 output mixing function (a bijective 64-bit hash), also used for lattice hashing.
pub fn mix64(mut z: u64) -> u64 {
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reference_sequence() {
        // First outputs of Vigna's reference splitmix64.c with seed 1234567.
        let mut r = SplitMix64::new(1_234_567);
        let expect = [
            6_457_827_717_110_365_317_u64,
            3_203_168_211_198_807_973,
            9_817_491_932_198_370_423,
            4_593_380_528_125_082_431,
            16_408_922_859_458_223_821,
        ];
        for e in expect {
            assert_eq!(r.next_u64(), e);
        }
    }

    #[test]
    fn uniform_moments() {
        // 10⁵ samples: mean 1/2 and variance 1/12 within 5σ (σ_mean = 0.29/√n ≈ 9e-4).
        let mut r = SplitMix64::new(42);
        let n = 100_000;
        let xs: Vec<f64> = (0..n).map(|_| r.next_f64()).collect();
        let mean = xs.iter().sum::<f64>() / n as f64;
        let var = xs.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / n as f64;
        assert!((mean - 0.5).abs() < 5e-3);
        assert!((var - 1.0 / 12.0).abs() < 2e-3);
        assert!(xs.iter().all(|x| (0.0..1.0).contains(x)));
        let mut r = SplitMix64::new(7);
        assert!((0..1000).all(|_| {
            let x = r.next_f64_open0();
            x > 0.0 && x <= 1.0
        }));
    }
}
