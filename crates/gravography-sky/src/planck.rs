//! Blackbody (Planck) spectra and constants (`docs/PRD.md` §5.7, §6.1). SI units throughout.

/// Planck constant `h` (J s, exact, SI 2019).
pub const PLANCK_H: f64 = 6.626_070_15e-34;
/// Speed of light `c` (m/s, exact).
pub const SPEED_OF_LIGHT: f64 = 299_792_458.0;
/// Boltzmann constant `k_B` (J/K, exact, SI 2019).
pub const BOLTZMANN_K: f64 = 1.380_649e-23;
/// Second radiation constant `c₂ = h c / k_B` (m K).
pub const C2: f64 = PLANCK_H * SPEED_OF_LIGHT / BOLTZMANN_K;
/// Stefan–Boltzmann constant `σ = 2π⁵ k⁴ / (15 h³ c²)` (W m⁻² K⁻⁴).
pub const STEFAN_BOLTZMANN: f64 = 5.670_374_419e-8;
/// Wien displacement constant `b` for `λ_max T = b` (m K, CODATA 2018).
pub const WIEN_B: f64 = 2.897_771_955e-3;

/// Spectral radiance per unit wavelength `B_λ(T) = 2hc²/λ⁵ / (exp(c₂/(λT)) − 1)` in W m⁻² sr⁻¹ m⁻¹, for
/// wavelength `lambda_m` (m) and temperature `t` (K). Uses `exp_m1` for accuracy when `c₂/(λT) ≪ 1`; returns 0
/// when the exponential overflows.
pub fn planck_lambda(lambda_m: f64, t: f64) -> f64 {
    let x = C2 / (lambda_m * t);
    let denom = x.exp_m1();
    if !denom.is_finite() {
        return 0.0;
    }
    2.0 * PLANCK_H * SPEED_OF_LIGHT * SPEED_OF_LIGHT / lambda_m.powi(5) / denom
}

/// Spectral radiance per unit frequency `B_ν(T) = 2hν³/c² / (exp(hν/(kT)) − 1)` in W m⁻² sr⁻¹ Hz⁻¹.
pub fn planck_nu(nu: f64, t: f64) -> f64 {
    let x = PLANCK_H * nu / (BOLTZMANN_K * t);
    let denom = x.exp_m1();
    if !denom.is_finite() {
        return 0.0;
    }
    2.0 * PLANCK_H * nu * nu * nu / (SPEED_OF_LIGHT * SPEED_OF_LIGHT) / denom
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_relative_eq;
    use proptest::prelude::*;

    /// Composite Simpson rule on `[a, b]` with `n` (even) intervals.
    fn simpson(f: impl Fn(f64) -> f64, a: f64, b: f64, n: usize) -> f64 {
        let h = (b - a) / n as f64;
        let mut s = f(a) + f(b);
        for i in 1..n {
            s += f(a + h * i as f64) * if i % 2 == 1 { 4.0 } else { 2.0 };
        }
        s * h / 3.0
    }

    #[test]
    fn constants_are_consistent() {
        // c₂ = 1.438776877e-2 m K (CODATA 2018); σ from h, c, k.
        assert_relative_eq!(C2, 1.438_776_877e-2, max_relative = 1e-9);
        let sigma = 2.0 * std::f64::consts::PI.powi(5) * BOLTZMANN_K.powi(4)
            / (15.0 * PLANCK_H.powi(3) * SPEED_OF_LIGHT.powi(2));
        assert_relative_eq!(sigma, STEFAN_BOLTZMANN, max_relative = 1e-9);
    }

    #[test]
    fn wien_peak() {
        // PRD §9 test 14: λ_max T = b. Golden-section search on B_λ; the peak is flat, so locate it to 1e-7.
        for t in [300.0, 3000.0, 5778.0, 30_000.0] {
            let (mut a, mut b) = (0.2 * WIEN_B / t, 5.0 * WIEN_B / t);
            let g = 0.5 * (5.0_f64.sqrt() - 1.0);
            for _ in 0..200 {
                let c = b - g * (b - a);
                let d = a + g * (b - a);
                if planck_lambda(c, t) > planck_lambda(d, t) {
                    b = d;
                } else {
                    a = c;
                }
            }
            assert_relative_eq!(0.5 * (a + b) * t, WIEN_B, max_relative = 1e-7);
        }
    }

    #[test]
    fn stefan_boltzmann_law() {
        // PRD §9 test 14: π ∫ B_ν dν = σ T⁴. Integrate in x = hν/kT over [1e-8, 60] (the tail beyond is
        // ~60³e⁻⁶⁰ ≈ 2e-21 of the total); Simpson with 2·10⁴ intervals is accurate to ~1e-12.
        for t in [100.0, 5778.0, 1e7] {
            let scale = BOLTZMANN_K * t / PLANCK_H;
            let integral = simpson(|x| planck_nu(x * scale, t) * scale, 1e-8, 60.0, 20_000);
            assert_relative_eq!(
                std::f64::consts::PI * integral,
                STEFAN_BOLTZMANN * t.powi(4),
                max_relative = 1e-9
            );
        }
    }

    #[test]
    fn wavelength_and_frequency_forms_agree() {
        // B_λ = B_ν c / λ² (same energy in dλ and dν).
        for (lambda, t) in [(5e-7, 5778.0), (2e-6, 300.0), (1e-8, 1e6)] {
            let nu = SPEED_OF_LIGHT / lambda;
            assert_relative_eq!(
                planck_lambda(lambda, t),
                planck_nu(nu, t) * SPEED_OF_LIGHT / (lambda * lambda),
                max_relative = 1e-12
            );
        }
    }

    #[test]
    fn limits() {
        // Rayleigh–Jeans (hν ≪ kT): B_ν → 2ν²kT/c². Wien (hν ≫ kT): B_ν → 2hν³/c² e^{−hν/kT}.
        let t = 1e4;
        let nu = 1e9;
        assert_relative_eq!(
            planck_nu(nu, t),
            2.0 * nu * nu * BOLTZMANN_K * t / SPEED_OF_LIGHT.powi(2),
            max_relative = 1e-5
        );
        let nu = 5e16;
        let x = PLANCK_H * nu / (BOLTZMANN_K * t);
        assert_relative_eq!(
            planck_nu(nu, t),
            2.0 * PLANCK_H * nu.powi(3) / SPEED_OF_LIGHT.powi(2) * (-x).exp(),
            max_relative = 1e-12
        );
        assert!(
            planck_lambda(3.6e-7, 1.0) == 0.0,
            "overflowing exponent gives zero"
        );
    }

    proptest! {
        #[test]
        fn blackbody_transforms_into_blackbody(log_t in 2.0..7.0_f64, log_g in -1.5..1.5_f64, log_nu in 12.0..17.0_f64) {
            // PRD §9 test 14 / §5.7: I_ν,obs(ν) = g³ I_ν,emit(ν/g), and for a blackbody g³ B_ν(ν/g, T) = B_ν(ν, gT).
            // Likewise g⁵ B_λ(gλ, T) = B_λ(λ, gT). Pure algebra: equal to rounding (1e-12).
            let (t, g, nu) = (10f64.powf(log_t), 10f64.powf(log_g), 10f64.powf(log_nu));
            let lhs = g.powi(3) * planck_nu(nu / g, t);
            let rhs = planck_nu(nu, g * t);
            prop_assert!((lhs - rhs).abs() <= 1e-12 * rhs.abs().max(1e-300));
            let lambda = SPEED_OF_LIGHT / nu;
            let lhs = g.powi(5) * planck_lambda(g * lambda, t);
            let rhs = planck_lambda(lambda, g * t);
            prop_assert!((lhs - rhs).abs() <= 1e-12 * rhs.abs().max(1e-300));
        }
    }
}
