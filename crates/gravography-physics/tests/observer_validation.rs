//! Observer validation against analytic results (`docs/PRD.md` §9 tests 9–11, plus geodetic precession and
//! frame dragging).

use std::f64::consts::{FRAC_PI_2, PI};

use gravography_physics::{
    Kerr, Observer, ObserverError, ObserverParams, RayStatus, Region, TraceParams, camera_ray_momentum,
    sky_frequency_shift, trace_ray,
};

fn params() -> ObserverParams<f64> {
    ObserverParams::default()
}

fn azimuth(pos: &[f64; 4]) -> f64 {
    pos[2].atan2(pos[1])
}

// ---------------------------------------------------------------------------------------------------------------
// Test 9: special-relativity limit.

#[test]
fn special_relativity_aberration_and_doppler() {
    // M = 1e-12 (Kerr does not allow M = 0): deflection ~4M/b ≈ 1e-13. Observer moving along +x with speed β,
    // forward = +x. A camera direction at angle θ from forward escapes toward cos θ_sky = (cos θ − β)/(1 − β cos θ)
    // and has g = 1/(γ(1 − β cos θ)) (docs/PRD.md §9 test 9; Rindler, *Relativity*, §4.3). The camera sits at
    // y = 50 and all rays lie in the plane y = 50, far from the origin. Tolerance: DOPRI5 rtol = 1e-10 over
    // ~40 straight steps, so 1e-9.
    let k = Kerr::with_mass(1e-12_f64, 0.0).unwrap();
    let pos = [0.0, 0.0, 50.0, 0.0];
    for beta in [0.0_f64, 0.5, 0.9, 0.99] {
        let gamma = 1.0 / (1.0 - beta * beta).sqrt();
        let u = [gamma, gamma * beta, 0.0, 0.0];
        let obs = Observer::new(&k, &pos, &u, &[1.0, 0.0, 0.0], &[0.0, 0.0, 1.0], &params()).unwrap();
        let g_lo = k.g_lo(&obs.pos);
        for i in 0..=12 {
            let th = PI * f64::from(i) / 12.0;
            let n = [th.cos(), 0.0, th.sin()];
            let p = camera_ray_momentum(&g_lo, &obs.tetrad, &n);
            let g = sky_frequency_shift(&p);
            assert!(
                (g - 1.0 / (gamma * (1.0 - beta * th.cos()))).abs() < 1e-12 * g,
                "β = {beta}, θ = {th}"
            );
            let res = trace_ray(&k, &obs.pos, &p, &TraceParams::cpu_reference());
            assert_eq!(res.status, RayStatus::Escaped);
            let d = res.escape_dir.unwrap();
            let cos_sky = (th.cos() - beta) / (1.0 - beta * th.cos());
            assert!(
                (d[0] - cos_sky).abs() < 1e-9,
                "β = {beta}, θ = {th}: {} vs {cos_sky}",
                d[0]
            );
            assert!(d[1].abs() < 1e-9);
            assert!((d[2] - (1.0 - cos_sky * cos_sky).max(0.0).sqrt()).abs() < 1e-7);
            // p_t is conserved exactly, so g at escape equals g at the camera.
            assert!((sky_frequency_shift(&res.p) - g).abs() < 1e-12 * g);
        }
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Test 10: static observer blueshift.

#[test]
fn static_observer_sees_uniform_blueshift() {
    // Hover preset in Schwarzschild: g = (1 − 2M/r)^{-1/2} for every direction (docs/PRD.md §9 test 10,
    // Appendix A). 26 directions (cube faces, edges, corners). The camera g is exact up to rounding (1e-12); for
    // escaping rays the value is also checked at the end of the trace.
    let k = Kerr::<f64>::schwarzschild();
    for r in [2.2, 3.0, 6.0, 20.0] {
        let obs = Observer::hover(&k, r, 1.0, 0.3, &params()).unwrap();
        let expect = 1.0 / (1.0 - 2.0 / r).sqrt();
        let g_lo = k.g_lo(&obs.pos);
        let mut escaped = 0;
        for i in -1..=1 {
            for j in -1..=1 {
                for l in -1..=1 {
                    if i == 0 && j == 0 && l == 0 {
                        continue;
                    }
                    let v = [f64::from(i), f64::from(j), f64::from(l)];
                    let nn = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
                    let n = v.map(|c| c / nn);
                    let p = camera_ray_momentum(&g_lo, &obs.tetrad, &n);
                    assert!(
                        (sky_frequency_shift(&p) - expect).abs() < 1e-12 * expect,
                        "r = {r}, n = {n:?}"
                    );
                    let res = trace_ray(&k, &obs.pos, &p, &TraceParams::cpu_reference());
                    if res.status == RayStatus::Escaped {
                        escaped += 1;
                        assert!((sky_frequency_shift(&res.p) - expect).abs() < 1e-12 * expect);
                    }
                }
            }
        }
        assert!(escaped > 0, "no ray escaped at r = {r}");
    }
}

// ---------------------------------------------------------------------------------------------------------------
// Test 11: circular orbits and radial infall.

#[test]
fn circular_orbits_stay_circular_with_the_kepler_period() {
    // r = 10, 10 orbits, a ∈ {0, 0.9}, prograde and retrograde, as free geodesics (no thrust). Expected
    // Ω = dφ/dt = ±1/(r^{3/2} ± a) (Appendix A; BPT 1972). Bounds: radius drift ≤ 1e-7 relative and Ω to 5e-8
    // relative; pre-correction constraint drift ≤ 1e-9 per substep. The phase error is global DOPRI5 error,
    // accumulating over ~2·10³ substeps at rtol = 1e-10: measured 3e-9 (a = 0, prograde) and 1.1e-8 (a = 0.9,
    // retrograde, where r = 10 is just outside the retrograde ISCO ≈ 8.72 and errors grow faster); a convergence probe
    // during WP-04 showed it falling ~15× per decade of rtol, i.e. truncation error, not a modelling error.
    let r = 10.0;
    for a in [0.0, 0.9] {
        let k = Kerr::new(a).unwrap();
        for pro in [true, false] {
            let omega = k.keplerian_angular_velocity(r, pro);
            let mut o = Observer::circular_orbit(&k, r, pro, 0.0, &params()).unwrap();
            let period_t = 2.0 * PI / omega.abs();
            let period_tau = period_t / o.u()[0];
            let mut phi_total = 0.0;
            let mut phi_prev = azimuth(&o.pos);
            let chunks = 200;
            let mut worst_dr: f64 = 0.0;
            let mut worst_constraint: f64 = 0.0;
            for _ in 0..chunks {
                let rep = o
                    .advance(&k, 10.0 * period_tau / f64::from(chunks), &[0.0; 3], &params())
                    .unwrap();
                worst_constraint = worst_constraint.max(rep.max_constraint_error);
                let phi = azimuth(&o.pos);
                let mut d = phi - phi_prev;
                if d > PI {
                    d -= 2.0 * PI;
                } else if d < -PI {
                    d += 2.0 * PI;
                }
                phi_total += d;
                phi_prev = phi;
                worst_dr = worst_dr.max((k.r(&o.pos) - r).abs() / r);
                assert!(o.pos[3].abs() < 1e-9, "left the equatorial plane");
            }
            let omega_measured = phi_total / o.pos[0];
            assert!(worst_dr < 1e-7, "a = {a}, pro = {pro}: radius drift {worst_dr}");
            assert!(
                ((omega_measured - omega) / omega).abs() < 5e-8,
                "a = {a}, pro = {pro}: Ω = {omega_measured} vs {omega}"
            );
            assert!(worst_constraint < 1e-9, "constraint drift {worst_constraint}");
            assert!((phi_total.abs() - 20.0 * PI).abs() < 1e-6);
        }
    }
}

#[test]
fn radial_infall_proper_time_matches_schwarzschild() {
    // From rest at infinity, r(τ) = (r₀^{3/2} − (3/2) sqrt(2M) τ)^{2/3}, so τ(r₀ → 0) = (2/3) sqrt(r₀³/(2M))
    // (docs/PRD.md §9 test 11; MTW §31.4). Checked along the way to 1e-9 relative, through the horizon, until the
    // inner limit r_min = 0.05 stops the simulation; then the remaining time to r = 0 is added analytically.
    let k = Kerr::<f64>::schwarzschild();
    let p = params();
    let r0: f64 = 10.0;
    let mut o = Observer::radial_infall(&k, r0, 0.7, 2.0, &p).unwrap();
    let dir0 = o.pos.map(|c| c / r0);
    let mut seen_inside = false;
    let stop = loop {
        let last = o;
        match o.advance(&k, 0.05, &[0.0; 3], &p) {
            Ok(_) => {
                let r = k.r(&o.pos);
                let expect = (r0.powf(1.5) - 1.5 * 2.0_f64.sqrt() * o.tau).powf(2.0 / 3.0);
                assert!(
                    (r - expect).abs() < 1e-9 * r0,
                    "τ = {}: r = {r} vs {expect}",
                    o.tau
                );
                // Radial: the direction from the centre is unchanged.
                for i in 1..4 {
                    assert!((o.pos[i] / r - dir0[i]).abs() < 1e-9);
                }
                seen_inside |= o.region(&k) == Region::InsideHorizon;
            }
            Err(e) => break (e, o, last),
        }
    };
    let (err, o, _) = stop;
    assert!(matches!(err, ObserverError::InnerLimit { .. }), "{err}");
    assert!(seen_inside);
    let r_last = k.r(&o.pos);
    assert!(r_last >= p.r_min(&k) && r_last < 0.2);
    let tau_total = o.tau + (2.0 / 3.0) * (r_last.powi(3) / 2.0).sqrt();
    let expect = (2.0 / 3.0) * (r0.powi(3) / 2.0).sqrt();
    assert!(
        (tau_total - expect).abs() < 1e-9 * expect,
        "{tau_total} vs {expect}"
    );
}

#[test]
fn kerr_axis_infall_stops_before_the_inner_horizon() {
    // a = 0.9 on the spin axis: crosses r_+ = 1.436, stops at r_min = r_− + 0.05 = 0.614 with a message. E = 1 and
    // L_z = 0 are conserved (rtol 1e-10 over a few hundred substeps ⇒ 1e-8).
    let k = Kerr::new(0.9_f64).unwrap();
    let p = params();
    let mut o = Observer::radial_infall(&k, 6.0, 0.0, 0.0, &p).unwrap();
    let mut regions = vec![o.region(&k)];
    let err = loop {
        match o.advance(&k, 0.1, &[0.0; 3], &p) {
            Ok(_) => {
                let reg = o.region(&k);
                if regions.last() != Some(&reg) {
                    regions.push(reg);
                }
                let u_lo = gravography_physics::linalg::lower(&k.g_lo(&o.pos), &o.u());
                assert!((k.energy(&u_lo) - 1.0).abs() < 1e-8);
                assert!(k.angular_momentum_z(&o.pos, &u_lo).abs() < 1e-8);
                assert!(o.pos[1].abs() < 1e-9 && o.pos[2].abs() < 1e-9, "left the axis");
            }
            Err(e) => break e,
        }
    };
    assert_eq!(regions, vec![Region::OutsideErgosphere, Region::InsideHorizon]);
    match err {
        ObserverError::InnerLimit { r, r_min } => {
            assert!((r_min - (k.inner_horizon() + 0.05)).abs() < 1e-12);
            assert!(r < r_min);
            assert!(k.r(&o.pos) >= r_min);
        }
        other => panic!("unexpected {other}"),
    }
    assert!(!err.to_string().is_empty());
}

#[test]
fn zero_angular_momentum_infall_is_dragged_prograde() {
    // Frame dragging (sign check, docs/PRD.md §2): an observer dropped from rest at infinity in the equatorial
    // plane with L_z = 0 is dragged in +φ for a > 0, and keeps θ = π/2 (Q = 0).
    let k = Kerr::new(0.9_f64).unwrap();
    let mut o = Observer::radial_infall(&k, 8.0, FRAC_PI_2, 0.0, &params()).unwrap();
    let phi0 = azimuth(&o.pos);
    let _ = o.advance(&k, 15.0, &[0.0; 3], &params());
    assert!(azimuth(&o.pos) > phi0 + 0.1);
    assert!(o.pos[3].abs() < 1e-9);
}

// ---------------------------------------------------------------------------------------------------------------
// Fermi–Walker transport: geodetic precession.

#[test]
fn geodetic_precession_on_a_schwarzschild_circular_orbit() {
    // A gyroscope on a circular geodesic orbit at radius r precesses by Δφ = 2π [1 − (1 − 3M/r)^{1/2}] per orbit,
    // in the sense of the orbit (Hartle, *Gravity*, §14.4; de Sitter precession). After one orbital period the
    // observer is back at the same KS position with the same u (the orbit is stationary), so the transported
    // forward leg (initially toward the hole) must equal the initial one rotated by Δφ in the orbital plane:
    // e₁(end) = cos Δφ e₁ + sin Δφ e₂ for a prograde (counter-clockwise) orbit with e₂ = left. Tolerance 1e-7:
    // global DOPRI5 error over one orbit at rtol = 1e-10 is ~2e-9 at r = 10 and ~1e-8 at r = 6 (the ISCO, where the
    // orbit is only marginally stable and errors grow faster); it converges ~15× per decade of rtol.
    let k = Kerr::<f64>::schwarzschild();
    for r in [6.0, 10.0, 30.0] {
        for pro in [true, false] {
            let p = params();
            let mut o = Observer::circular_orbit(&k, r, pro, 0.0, &p).unwrap();
            let start = o;
            let omega = k.keplerian_angular_velocity(r, pro);
            let period_tau = 2.0 * PI / omega.abs() / o.u()[0];
            o.advance(&k, period_tau, &[0.0; 3], &p).unwrap();
            for i in 1..4 {
                assert!(
                    (o.pos[i] - start.pos[i]).abs() < 1e-7 * r,
                    "not back at the start"
                );
            }
            let g = k.g_lo(&start.pos);
            let ip = |a: &[f64; 4], b: &[f64; 4]| gravography_physics::linalg::inner(&g, a, b);
            let f_end = o.tetrad.e_up[1];
            let c1 = ip(&f_end, &start.tetrad.e_up[1]);
            let c2 = ip(&f_end, &start.tetrad.e_up[2]);
            let dphi = 2.0 * PI * (1.0 - (1.0 - 3.0 / r).sqrt());
            // At φ = 0, forward = −x̂ and left = ẑ × forward = −ŷ. Rotating forward counter-clockwise (+φ, the
            // prograde sense) by δ gives (−cos δ, −sin δ, 0) = cos δ forward + sin δ left. Retrograde flips the sign.
            let s = if pro { 1.0 } else { -1.0 };
            assert!(
                (c1 - dphi.cos()).abs() < 1e-7,
                "r = {r}, pro = {pro}: c1 = {c1}, expected {}",
                dphi.cos()
            );
            assert!(
                (c2 - s * dphi.sin()).abs() < 1e-7,
                "r = {r}, pro = {pro}: c2 = {c2}, expected {}",
                s * dphi.sin()
            );
        }
    }
}
