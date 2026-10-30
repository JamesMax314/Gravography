//! Validation of the reference renderer's lensing footprint against the analytic point-lens map.

use gravography_physics::{Kerr, RayStatus, Tetrad};
use gravography_render::cpu::RenderStats;
use gravography_render::tonemap::Exposure;
use gravography_render::{CpuRenderer, CpuSettings, Effects, FrameParams, Projection, Sky, ToneMap};
use gravography_sky::cubemap::CubeLevel;
use gravography_sky::{BlackbodyLut, CubeMap};

/// `P(u) = 1/b² − u² + 2u³` (`u = 1/r`, `M = 1`): the Schwarzschild photon orbit equation is
/// `(du/dφ)² = P(u)` (e.g. Chandrasekhar, "The Mathematical Theory of Black Holes", §20, or Hartle §9.3).
fn orbit_p(u: f64, b: f64) -> f64 {
    1.0 / (b * b) - u * u + 2.0 * u * u * u
}

/// `∫ du / sqrt(P(u))` from `u_lo` to the turning point `u_m`, with `u = u_m − t²` to remove the endpoint
/// singularity (`P ≈ t² Q(t)` with smooth `Q > 0`), by composite Simpson in `t` (2000 intervals: error ≲ 1e-13
/// for these smooth integrands, checked by halving).
fn azimuth_to_turning_point(u_lo: f64, u_m: f64, b: f64) -> f64 {
    let n = 2000;
    let t_max = (u_m - u_lo).sqrt();
    let h = t_max / f64::from(n);
    let f = |t: f64| {
        if t == 0.0 {
            // Limit 2t/sqrt(P(u_m − t²)) → 2/sqrt(−P'(u_m)) with P'(u) = −2u + 6u².
            2.0 / (2.0 * u_m - 6.0 * u_m * u_m).sqrt()
        } else {
            2.0 * t / orbit_p(u_m - t * t, b).sqrt()
        }
    };
    let mut sum = f(0.0) + f(t_max);
    for k in 1..n {
        sum += f(f64::from(k) * h) * if k % 2 == 1 { 4.0 } else { 2.0 };
    }
    sum * h / 3.0
}

/// Exact sky angle `ψ` (measured from the direction to the hole, positive on the side of the ray) of the ray that
/// leaves a static camera at radius `d` at local angle `θ` from the hole direction. The impact parameter is
/// `b = d sin θ / sqrt(1 − 2/d)` (local angle of a static observer); the ray sweeps the azimuth
/// `Δφ = ∫_{1/d}^{u_m} + ∫_0^{u_m}` and leaves toward `ψ = π − Δφ` (flat space: `Δφ = π − θ`, `ψ = θ`).
fn sky_angle(theta: f64, d: f64) -> f64 {
    let b = d * theta.sin() / (1.0 - 2.0 / d).sqrt();
    // Turning point: the smallest positive root of P, in (0, 1/3) for b > 3√3 (bisection to machine precision).
    let (mut lo, mut hi) = (0.0, 1.0 / 3.0);
    for _ in 0..200 {
        let mid = 0.5 * (lo + hi);
        if orbit_p(mid, b) > 0.0 {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    let u_m = lo;
    let dphi = azimuth_to_turning_point(1.0 / d, u_m, b) + azimuth_to_turning_point(0.0, u_m, b);
    std::f64::consts::PI - dphi
}

#[test]
fn ray_differential_footprint_matches_point_lens_magnification() {
    // Schwarzschild, static camera at d = 10⁴ looking at the hole; Einstein angle θ_E = sqrt(4/d) = 0.02 rad.
    // For pixels at local angle θ the inverse magnification of the lens map is
    //   dΩ_sky / dΩ_image = (sin ψ / sin θ) |dψ/dθ|  (axisymmetric map; Schneider, Ehlers & Falco §2.3),
    // ≈ |1 − (θ_E/θ)⁴| in the weak field; here ψ(θ) comes from exact quadrature of the orbit equation. The
    // renderer's escape direction must match ψ, and its ray-differential footprint divided by the analytic pixel
    // solid angle must reproduce the ratio, inside (secondary image, parity flip) and outside the Einstein ring.
    //
    // Tolerances: ψ to TOL_PSI = 1e-8 rad (DOPRI5 rtol 1e-10; measured ≈ 7e-12 rad; the quadrature is exact to
    // ~1e-13). Ratio to TOL_RATIO = 1e-3 relative: the footprint uses forward differences (ε = 0.05 px, 4°/256 px),
    // whose O(ε) truncation error is deterministic and largest where the map curves most, inside the Einstein ring:
    // measured 6.8e-4 at θ = 0.7 θ_E, 4e-5 at 2.5 θ_E. (In rendering the footprint only selects the mip level,
    // where 1e-3 in solid angle is 7e-4 of a level.) dψ/dθ below is a central difference with step 1e-6
    // (error ≲ 1e-7).
    const TOL_PSI: f64 = 1e-8;
    const TOL_RATIO: f64 = 1e-3;
    let d = 1e4;
    let kerr = Kerr::schwarzschild();
    let position = [0.0, d, 0.0, 0.0];
    let g = kerr.g_lo(&position);
    let seeds = [
        [1.0, 0.0, 0.0, 0.0],
        [0.0, -1.0, 0.0, 0.0],
        [0.0, 0.0, -1.0, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ];
    let (width, height) = (256_u32, 256_u32);
    let frame = FrameParams {
        kerr,
        position,
        tetrad: Tetrad::gram_schmidt(&g, &seeds).unwrap(),
        width,
        height,
        projection: Projection::Rectilinear { fov_deg: 4.0 },
        samples: 1,
        effects: Effects::default(),
        exposure: Exposure::default(),
        tone_map: ToneMap::default(),
    };
    let mut base = CubeLevel::zeros(4);
    base.data.fill([1.0, 5000.0]);
    let sky = Sky::new(CubeMap::with_mips(base), BlackbodyLut::human_vision());
    let settings = CpuSettings {
        differential_offset: 0.05,
        ..CpuSettings::default()
    };
    let r = CpuRenderer::new(&frame, &sky, settings).unwrap();
    let theta_e = (4.0 / d).sqrt();
    let half_tan = 2f64.to_radians().tan();
    for ratio in [0.7, 0.9, 1.2, 1.5, 2.5] {
        // Pixel row whose centre column direction is at angle ≈ ratio · θ_E above the axis (x on the centre line).
        let theta_target = ratio * theta_e;
        let y = 0.5 * f64::from(height) * (1.0 - theta_target.tan() / half_tan);
        let x = 0.5 * f64::from(width);
        let n = frame.projection.direction(width, height, x, y);
        let theta = n[0].acos();
        let hit = r.cast_pixel(x, y);
        assert_eq!(hit.status, RayStatus::Escaped);
        let dir = hit.dir.unwrap();
        // Escape direction: angle from −x (toward the hole); it stays in the x–z plane.
        assert!(dir[1].abs() < 1e-12);
        let psi_numeric = dir[2].atan2(-dir[0]);
        let psi = sky_angle(theta, d);
        assert!(
            (psi_numeric - psi).abs() < TOL_PSI,
            "ψ at θ/θ_E = {ratio}: {psi_numeric} vs {psi}"
        );
        let (sr, fallback) = r.footprint(x, y, &dir, &mut RenderStats::default());
        assert_eq!(fallback, 0);
        let measured = sr / frame.projection.pixel_solid_angle(width, height, x, y);
        let h = 1e-6;
        let dpsi = (sky_angle(theta + h, d) - sky_angle(theta - h, d)) / (2.0 * h);
        let expect = (psi.sin() / theta.sin() * dpsi).abs();
        let weak = (1.0 - (theta_e / theta).powi(4)).abs();
        println!(
            "θ/θ_E = {ratio}: |Δψ| = {:.2e}, ratio {measured:.6} vs exact {expect:.6} (rel {:.1e}), weak field {weak:.6}",
            (psi_numeric - psi).abs(),
            measured / expect - 1.0
        );
        assert!(
            (measured / expect - 1.0).abs() < TOL_RATIO,
            "θ/θ_E = {ratio}: measured {measured}, expected {expect} (weak-field {weak})"
        );
    }
}
