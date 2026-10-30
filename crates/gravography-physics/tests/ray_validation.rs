//! Ray-tracing validation against analytic results (`docs/PRD.md` §9 tests 4–8 and 12).

use std::f64::consts::{FRAC_PI_2, PI};

use gravography_physics::ray::trace_ray_observed;
use gravography_physics::{
    Kerr, RayResult, RayStatus, Tetrad, TraceParams, Vec4, camera_ray_momentum, trace_ray,
};

fn params() -> TraceParams<f64> {
    TraceParams::cpu_reference()
}

/// Traces the ray of a distant observer (Bardeen image coordinates) from `r_obs`.
fn trace_bardeen(k: &Kerr<f64>, r_obs: f64, incl: f64, alpha: f64, beta: f64) -> RayResult<f64> {
    let (x, p) = k
        .bardeen_ray(r_obs, incl, 0.0, alpha, beta)
        .expect("valid distant ray");
    trace_ray(k, &x, &p, &params())
}

/// Bisection for the boundary between escaping and captured rays: `escapes(lo) != escapes(hi)` on entry.
/// Returns the boundary to within `(hi − lo) / 2ⁿ`.
fn bisect(mut lo: f64, mut hi: f64, n: u32, escapes: impl Fn(f64) -> bool) -> f64 {
    let lo_escapes = escapes(lo);
    assert_ne!(
        lo_escapes,
        escapes(hi),
        "bisection interval does not bracket the critical curve"
    );
    for _ in 0..n {
        let mid = 0.5 * (lo + hi);
        if escapes(mid) == lo_escapes {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    0.5 * (lo + hi)
}

/// Equatorial ray with impact parameter `b` (ξ = b, Q = 0) from a distant observer.
fn equatorial_escapes(k: &Kerr<f64>, r_obs: f64, alpha: f64) -> bool {
    let res = trace_bardeen(k, r_obs, FRAC_PI_2, alpha, 0.0);
    assert!(
        matches!(res.status, RayStatus::Escaped | RayStatus::Captured),
        "unexpected status {:?} at α = {alpha}",
        res.status
    );
    res.status == RayStatus::Escaped
}

/// Bardeen (1973) spherical-photon-orbit `ξ(r)` (docs/PRD.md Appendix A, M = 1).
fn xi_photon(a: f64, r: f64) -> f64 {
    (r * r * (3.0 - r) - a * a * (r + 1.0)) / (a * (r - 1.0))
}

fn angle_between(u: &[f64; 3], v: &[f64; 3]) -> f64 {
    // atan2(|u × v|, u · v) is accurate for small angles, unlike acos.
    let c = [
        u[1] * v[2] - u[2] * v[1],
        u[2] * v[0] - u[0] * v[2],
        u[0] * v[1] - u[1] * v[0],
    ];
    let s = (c[0] * c[0] + c[1] * c[1] + c[2] * c[2]).sqrt();
    s.atan2(u[0] * v[0] + u[1] * v[1] + u[2] * v[2])
}

fn unit_spatial(v: &Vec4<f64>) -> [f64; 3] {
    let n = (v[1] * v[1] + v[2] * v[2] + v[3] * v[3]).sqrt();
    [v[1] / n, v[2] / n, v[3] / n]
}

/// Static observer (u ∝ ∂_t) at the KS event `pos` looking at the hole, `up` ≈ +z.
fn static_camera(k: &Kerr<f64>, pos: &Vec4<f64>) -> Tetrad<f64> {
    let g = k.g_lo(pos);
    let r = (pos[1] * pos[1] + pos[2] * pos[2] + pos[3] * pos[3]).sqrt();
    let fwd = [0.0, -pos[1] / r, -pos[2] / r, -pos[3] / r];
    // left = up × forward with a provisional up = +z (or +x if looking along z).
    let up0 = if pos[1].abs() + pos[2].abs() > 1e-9 {
        [0.0, 0.0, 1.0]
    } else {
        [1.0, 0.0, 0.0]
    };
    let f = [fwd[1], fwd[2], fwd[3]];
    let left = [
        up0[1] * f[2] - up0[2] * f[1],
        up0[2] * f[0] - up0[0] * f[2],
        up0[0] * f[1] - up0[1] * f[0],
    ];
    let up = [
        f[1] * left[2] - f[2] * left[1],
        f[2] * left[0] - f[0] * left[2],
        f[0] * left[1] - f[1] * left[0],
    ];
    let seeds = [
        [1.0, 0.0, 0.0, 0.0],
        fwd,
        [0.0, left[0], left[1], left[2]],
        [0.0, up[0], up[1], up[2]],
    ];
    Tetrad::gram_schmidt(&g, &seeds).expect("static observer outside the ergosphere")
}

// ---------------------------------------------------------------------------------------------------------------
// Test 4: conservation along rays.

#[test]
fn invariants_are_conserved_along_rays() {
    // Camera: static observer at r = 20, θ = 60°, a = 0.9; a 9×9 fan of directions over ±23° (the shadow's angular radius is ≈ 14°) plus a few sideways and backward rays,
    // giving a mix of escaped, strongly bent and captured rays. Drifts are measured along the whole path while
    // r > r_+ + 0.5 (closer in, captured rays' momenta grow without bound as they approach the past horizon;
    // relative errors stay bounded but the "natural scale" is ill-defined).
    //
    // Tolerances (DOPRI5 rtol = 1e-10, ≲ 2000 steps, errors accumulate roughly linearly):
    const H_TOL: f64 = 1e-8; // |H| / max|p|²
    const LZ_TOL: f64 = 1e-8; // |ΔL_z| / (|L_z| + E · r_cam)
    const Q_TOL: f64 = 1e-8; // |ΔQ| / (|Q| + L_z² + E² r_cam²)
    let k = Kerr::new(0.9).unwrap();
    let (th, r) = (PI / 3.0, 20.0);
    let pos = [0.0, r * th.sin(), 0.0, r * th.cos()];
    let cam = static_camera(&k, &pos);
    let g = k.g_lo(&pos);
    let mut statuses = std::collections::HashMap::new();
    let mut worst = (0.0_f64, 0.0_f64, 0.0_f64);
    let mut dirs = Vec::new();
    for i in 0..9 {
        for j in 0..9 {
            let (ax, ay) = ((i as f64 - 4.0) / 4.0 * 0.4, (j as f64 - 4.0) / 4.0 * 0.4);
            let n = [1.0, ax.tan(), ay.tan()];
            dirs.push(n);
        }
    }
    dirs.extend([
        [-1.0, 0.0, 0.0],
        [-1.0, 0.5, 0.2],
        [0.0, 1.0, 0.0],
        [0.0, 0.0, -1.0],
    ]);
    for n in dirs {
        let norm = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
        let n = n.map(|c| c / norm);
        let p0 = camera_ray_momentum(&g, &cam, &n);
        let (e0, lz0, q0) = (
            k.energy(&p0),
            k.angular_momentum_z(&pos, &p0),
            k.carter_q(&pos, &p0),
        );
        let r_plus = k.outer_horizon();
        let res = trace_ray_observed(&k, &pos, &p0, &params(), |_, x, p| {
            if k.r(x) < r_plus + 0.5 {
                return;
            }
            let pmax = p.iter().fold(0.0_f64, |m, c| m.max(c.abs()));
            assert!(
                k.energy(p).to_bits() == e0.to_bits(),
                "p_t must be conserved exactly"
            );
            let dh = k.hamiltonian(x, p).abs() / (pmax * pmax);
            let dl = (k.angular_momentum_z(x, p) - lz0).abs() / (lz0.abs() + e0.abs() * r);
            let dq = (k.carter_q(x, p) - q0).abs() / (q0.abs() + lz0 * lz0 + e0 * e0 * r * r);
            worst = (worst.0.max(dh), worst.1.max(dl), worst.2.max(dq));
        });
        *statuses.entry(format!("{:?}", res.status)).or_insert(0) += 1;
    }
    println!("statuses {statuses:?}, worst drift (H, Lz, Q) = {worst:?}");
    assert!(statuses.get("Escaped").copied().unwrap_or(0) > 10);
    assert!(statuses.get("Captured").copied().unwrap_or(0) > 3);
    assert!(
        statuses.len() == 2,
        "only escaped/captured rays expected: {statuses:?}"
    );
    assert!(
        worst.0 < H_TOL && worst.1 < LZ_TOL && worst.2 < Q_TOL,
        "drift {worst:?}"
    );
}

// ---------------------------------------------------------------------------------------------------------------
// Test 5: Schwarzschild critical impact parameter b_c = 3√3 M.

#[test]
fn schwarzschild_critical_impact_parameter() {
    // Distant observer (r_obs = 1e4, exact constants of motion); bisection in b = −α on captured/escaped.
    // Accuracy limited by integration error near the photon sphere (rtol 1e-10): expect ≲ 1e-8; PRD asks 1e-5.
    let k = Kerr::<f64>::schwarzschild();
    let b = bisect(5.0, 5.5, 36, |b| equatorial_escapes(&k, 1e4, -b));
    let bc = 3.0 * 3.0_f64.sqrt();
    println!("b_c = {b:.12}, error {:e}", (b - bc).abs());
    assert!((b - bc).abs() <= 1e-5);
}

// ---------------------------------------------------------------------------------------------------------------
// Test 6: weak deflection δ ≈ 4M/b + (15π/4)(M/b)².

#[test]
fn weak_field_deflection() {
    // Equatorial rays from r_obs = 1e6 (bending outside r_obs and outside r_escape = 8e6: ~M b/R² ≲ 1e-8 rad).
    // The next term of the series is (128/3)(M/b)³: relative size ≈ 10.7/b² ≤ 4e-5 at b = 500. PRD: ≤ 1e-3.
    let k = Kerr::<f64>::schwarzschild();
    for b in [500.0, 1000.0, 2000.0, 5000.0] {
        let (x, p) = k.bardeen_ray(1e6, FRAC_PI_2, 0.0, -b, 0.0).unwrap();
        let (dx0, _) = k.hamiltonian_rhs(&x, &p);
        let res = trace_ray(&k, &x, &p, &params());
        assert_eq!(res.status, RayStatus::Escaped);
        let delta = angle_between(&unit_spatial(&dx0), &res.escape_dir.unwrap());
        let expected = 4.0 / b + 15.0 * PI / 4.0 / (b * b);
        let rel = (delta - expected).abs() / expected;
        println!("b = {b}: δ = {delta:.10e}, series {expected:.10e}, rel {rel:.2e}");
        assert!(rel <= 1e-3);
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Test 7: exact Schwarzschild deflection vs quadrature of the orbit equation.

/// `F(u) = 1/b² − u² + 2Mu³` for the Schwarzschild orbit equation `(du/dφ)² = F(u)`, `u = 1/r`, M = 1.
fn orbit_f(b: f64, u: f64) -> f64 {
    1.0 / (b * b) - u * u + 2.0 * u * u * u
}

/// Turning point `u₀ = 1/r_min`: the root of F in (0, 1/3) for b > b_c (F(0) > 0 > F(1/3)), by bisection.
fn turning_point(b: f64) -> f64 {
    let (mut lo, mut hi) = (0.0, 1.0 / 3.0);
    for _ in 0..200 {
        let mid = 0.5 * (lo + hi);
        if orbit_f(b, mid) > 0.0 {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    0.5 * (lo + hi)
}

/// `∫_{u_end}^{u₀} du / √F(u)` with `u = u₀ − (u₀ − u_end) s²`, which removes the inverse-square-root endpoint
/// singularity. Composite 5-point Gauss–Legendre on [0, 1] (smooth integrand; 2000 panels ⇒ error ≪ 1e-12).
fn orbit_quadrature(b: f64, u_end: f64) -> f64 {
    const X: [f64; 5] = [
        0.0,
        -0.538_469_310_105_683_1,
        0.538_469_310_105_683_1,
        -0.906_179_845_938_664,
        0.906_179_845_938_664,
    ];
    const W: [f64; 5] = [
        0.568_888_888_888_888_9,
        0.478_628_670_499_366_5,
        0.478_628_670_499_366_5,
        0.236_926_885_056_189_1,
        0.236_926_885_056_189_1,
    ];
    let u0 = turning_point(b);
    let span = u0 - u_end;
    let panels = 2000;
    let hw = 0.5 / panels as f64;
    let mut sum = 0.0;
    for k in 0..panels {
        let mid = (2 * k + 1) as f64 * hw;
        for (x, w) in X.iter().zip(W) {
            let s = mid + hw * x;
            let u = u0 - span * s * s;
            sum += w * hw * 2.0 * span * s / orbit_f(b, u).sqrt();
        }
    }
    sum
}

#[test]
fn quadrature_reproduces_known_limits() {
    // Self-check of the reference: for large b the total deflection 2∫₀^{u₀} − π → 4/b + 15π/(4b²).
    let b = 1e3;
    let delta = 2.0 * orbit_quadrature(b, 0.0) - PI;
    assert!(((delta - (4.0 / b + 15.0 * PI / (4.0 * b * b))) / delta).abs() < 1e-4);
}

#[test]
fn schwarzschild_strong_deflection_matches_orbit_quadrature() {
    // Equatorial rays from r_obs = 1000; the swept azimuth Δφ of the traced path is compared with the exact
    // integral over the same radial range, ∫_{1/r_obs}^{u₀} + ∫_{1/r_end}^{u₀} (no asymptotic approximation).
    // Tolerance: DOPRI5 rtol 1e-10 over ≲ 1e3 steps gives ~1e-8 in Δφ; near b_c the sensitivity grows like
    // 1/(b − b_c), so allow 1e-6 rad absolute.
    let k = Kerr::<f64>::schwarzschild();
    let r_obs = 1000.0;
    for b in [5.3, 5.6, 6.5, 8.0, 15.0, 40.0] {
        let (x, p) = k.bardeen_ray(r_obs, FRAC_PI_2, 0.0, -b, 0.0).unwrap();
        let mut phi = x[2].atan2(x[1]);
        let mut swept = 0.0;
        let res = trace_ray_observed(&k, &x, &p, &params(), |_, x, _| {
            let new = x[2].atan2(x[1]);
            let mut d = new - phi;
            if d > PI {
                d -= 2.0 * PI;
            } else if d < -PI {
                d += 2.0 * PI;
            }
            swept += d;
            phi = new;
        });
        assert_eq!(res.status, RayStatus::Escaped, "b = {b}");
        let r_end = k.r(&res.x);
        let expected = orbit_quadrature(b, 1.0 / r_obs) + orbit_quadrature(b, 1.0 / r_end);
        let err = (swept.abs() - expected).abs();
        println!(
            "b = {b}: Δφ = {:.12}, quadrature {expected:.12}, error {err:.2e}",
            swept.abs()
        );
        assert!(err <= 1e-6);
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Test 8: Kerr shadow edge for a distant equatorial observer.

#[test]
fn kerr_shadow_edges_on_the_equatorial_axis() {
    // i = 90°, β = 0 ⇒ Q = 0: equatorial photons. The shadow edges on β = 0 are α = −ξ(r_ph,pro) and
    // α = −ξ(r_ph,retro) (Bardeen 1973; docs/PRD.md Appendix A). The constants of motion are exact (no finite-
    // distance error); r_obs = 1e4 as the PRD asks. Bisection to 2^-34 of the bracket; PRD tolerance 1e-3 relative.
    for a in [0.5, 0.9, 0.998] {
        let k = Kerr::new(a).unwrap();
        let alpha_pro = -xi_photon(a, k.photon_orbit(true));
        let alpha_retro = -xi_photon(a, k.photon_orbit(false));
        let inside = 0.5 * (alpha_pro + alpha_retro);
        let left = bisect(alpha_pro - 1.0, inside, 34, |al| equatorial_escapes(&k, 1e4, al));
        let right = bisect(inside, alpha_retro + 1.0, 34, |al| {
            equatorial_escapes(&k, 1e4, al)
        });
        let (el, er) = (
            (left - alpha_pro).abs() / alpha_pro.abs(),
            (right - alpha_retro).abs() / alpha_retro.abs(),
        );
        println!(
            "a = {a}: left {left:.9} (exp {alpha_pro:.9}, rel {el:.1e}), right {right:.9} (exp {alpha_retro:.9}, rel {er:.1e})"
        );
        assert!(el <= 1e-3 && er <= 1e-3);
    }
}

#[test]
fn kerr_shadow_edge_from_a_static_camera() {
    // The same edge found through the camera pipeline (tetrad → camera_ray_momentum → trace): a static observer at
    // r = 1e4 in the equatorial plane scans rays in the equatorial plane. At the edge, the ray's own ξ = L_z/E must
    // equal ξ(r_ph) (exact, independent of distance), and the viewing angle obeys α ≈ −r_obs sin ψ up to
    // finite-distance corrections O(M/r_obs) ≈ 1e-4 (checked to 1e-3 relative).
    let a = 0.9;
    let k = Kerr::new(a).unwrap();
    let r_obs = 1e4;
    let pos = [0.0, r_obs, 0.0, 0.0];
    let cam = static_camera(&k, &pos);
    let g = k.g_lo(&pos);
    // Camera legs: forward = −x̂, left = −ŷ, up = +ẑ. Viewing angle ψ toward "left" (−y).
    let ray = |psi: f64| camera_ray_momentum(&g, &cam, &[psi.cos(), psi.sin(), 0.0]);
    let escapes = |psi: f64| trace_ray(&k, &pos, &ray(psi), &params()).status == RayStatus::Escaped;
    for prograde in [true, false] {
        let xi_c = xi_photon(a, k.photon_orbit(prograde));
        // The shadow is the central range of ψ: |ψ| below the edge is captured. Bracket around ψ0 ≈ ξ/r_obs
        // (ψ > 0 turns the ray toward −y, giving L_z/E > 0).
        let psi0 = xi_c / r_obs;
        let psi = bisect(0.7 * psi0, 1.3 * psi0, 34, escapes);
        let p = ray(psi);
        let xi = k.angular_momentum_z(&pos, &p) / k.energy(&p);
        let alpha = -r_obs * psi.sin();
        println!(
            "prograde {prograde}: ψ = {psi:.6e}, ξ = {xi:.8} (exp {xi_c:.8}), α ≈ {alpha:.6} (exp {:.6})",
            -xi_c
        );
        assert!(((xi - xi_c) / xi_c).abs() <= 1e-6);
        assert!(((alpha + xi_c) / xi_c).abs() <= 1e-3);
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Test 12: escape-direction convergence with increasing r_escape.

#[test]
fn escape_direction_converges_with_escape_radius() {
    // Rays from cameras at r = 30 and r = 100 (a = 0.9). The reference escapes at r = 1e9. The finite-radius
    // error must fall quickly with R (measured ≈ R⁻⁴: the KS coordinate direction of an outgoing traced ray is
    // nearly parallel to the straight ingoing congruence, docs/physics.md § "Escape direction"), and the CPU
    // reference default (r_escape = max(16 r_cam, 400)) must be accurate to 1e-6 rad (measured ≤ 3e-7).
    let k = Kerr::new(0.9).unwrap();
    for (r_cam, psi) in [(30.0, 0.35), (30.0, 1.3), (100.0, 1.5), (100.0, 2.6)] {
        let pos = [0.0, r_cam, 0.0, 0.0];
        let cam = static_camera(&k, &pos);
        let p = camera_ray_momentum(
            &k.g_lo(&pos),
            &cam,
            &[f64::cos(psi), 0.8 * f64::sin(psi), 0.6 * f64::sin(psi)],
        );
        let dir_at = |escape_factor: f64, escape_min_radius: f64| {
            let mut q = params();
            q.escape_factor = escape_factor;
            q.escape_min_radius = escape_min_radius;
            let res = trace_ray(&k, &pos, &p, &q);
            assert_eq!(res.status, RayStatus::Escaped);
            res.escape_dir.unwrap()
        };
        let reference = dir_at(0.0, 1e9);
        let errs: Vec<f64> = [2.0, 4.0, 8.0, 16.0]
            .iter()
            .map(|&f| angle_between(&dir_at(0.0, f * r_cam), &reference))
            .collect();
        let default_err = angle_between(
            &dir_at(params().escape_factor, params().escape_min_radius),
            &reference,
        );
        println!(
            "r_cam {r_cam}, ψ {psi}: errors at R = (2, 4, 8, 16)·r_cam: {errs:?}; default {default_err:.2e}"
        );
        for w in errs.windows(2) {
            // Each doubling of R must cut the error by ≥ 6× (≈16× expected), unless already at the integration floor.
            assert!(w[1] < w[0] / 6.0 || w[1] < 1e-9, "{errs:?}");
        }
        assert!(default_err < 1e-6);
    }
}
