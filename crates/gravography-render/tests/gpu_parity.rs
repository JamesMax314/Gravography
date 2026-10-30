//! WP-07 acceptance: RK4 GPU vs CPU-f32 vs CPU-f64 parity on the fixed camera set (PRD §9 test 15 skeleton).
//! Needs a GPU adapter; without one the tests print `SKIPPED …` and pass (`CLAUDE.md` §4).

use gravography_physics::ray::Integrator;
use gravography_physics::ray::trace_ray_observed;
use gravography_physics::{Kerr, TraceParams, ks_fields, trace_ray};
use gravography_render::gpu::{GpuContext, GpuRay, TestRayPipeline};
use gravography_render::parity::{Comparison, RayOutcome, camera_rays, compare, parity_cameras, to_f32};

/// Rays per camera side (32 × 32 grid over a 90° field of view).
const SIDE: u32 = 32;
const FOV_DEG: f64 = 90.0;

fn rk4<T: gravography_physics::Real>() -> TraceParams<T> {
    let mut p = TraceParams::<T>::cpu_reference();
    p.integrator = Integrator::Rk4;
    // RK4 takes one step per step cap (≲ a few hundred per ray here); 20 000 keeps the GPU well inside watchdog
    // limits while never being reached by these cameras.
    p.max_steps = 20_000;
    p
}

#[test]
fn gpu_rhs_matches_cpu_f32() {
    let Some(ctx) = GpuContext::for_tests("gpu_rhs_matches_cpu_f32") else {
        return;
    };
    println!("adapter: {}", ctx.describe());
    let pipe = TestRayPipeline::new(&ctx);
    // Phase-space points along CPU f64 rays of every camera (strong field included), every 7th accepted step.
    let mut worst = 0.0_f64;
    for cam in parity_cameras() {
        let k32 = Kerr::new(cam.kerr.spin() as f32).unwrap();
        let mut pts = Vec::new();
        for (x, p) in camera_rays(&cam, 8, FOV_DEG) {
            let mut n = 0;
            trace_ray_observed(&cam.kerr, &x, &p, &rk4(), |_, x, p| {
                if n % 7 == 0 {
                    pts.push(GpuRay {
                        x: to_f32(x),
                        p: to_f32(p),
                    });
                }
                n += 1;
            });
        }
        let gpu = pipe.eval_rhs(&ctx, &k32, &pts).unwrap();
        for (ray, out) in pts.iter().zip(&gpu) {
            let (dx, dp) = k32.hamiltonian_rhs(&ray.x, &ray.p);
            // Natural scales (as in the physics crate's RHS tests): |p|(1 + f) for dx, |p|²(1 + f)²/r for dp.
            let fl = ks_fields(1.0_f32, k32.spin(), &ray.x);
            let pn = ray.p.iter().fold(0.0_f32, |m, c| m.max(c.abs()));
            let sx = f64::from(pn * (1.0 + fl.f));
            let sp = f64::from(pn * pn * (1.0 + fl.f).powi(2) / fl.r.max(1e-3));
            for mu in 0..4 {
                worst = worst.max(f64::from((out.dx[mu] - dx[mu]).abs()) / sx);
                worst = worst.max(f64::from((out.dp[mu] - dp[mu]).abs()) / sp);
            }
        }
    }
    println!("max scaled |RHS_gpu − RHS_cpu_f32| = {worst:.2e}");
    // Same formulas in the same order; differences come only from f32 rounding (FMA contraction, sqrt/division
    // rounding on the GPU). Measured 5.5e-7 (≈ 5 f32 ulp, ε = 1.2e-7) on an Apple M2 Pro; 1e-5 (≈ 80 ulp) leaves
    // room for other drivers while still catching any formula difference, which shows up at O(1).
    const TOL_RHS: f64 = 1e-5;
    assert!(
        worst < TOL_RHS,
        "GPU RHS deviates from CPU f32 by {worst:.2e} (scaled)"
    );
}

#[test]
fn rk4_gpu_vs_cpu_f32_vs_cpu_f64() {
    let Some(ctx) = GpuContext::for_tests("rk4_gpu_vs_cpu_f32_vs_cpu_f64") else {
        return;
    };
    println!("adapter: {}", ctx.describe());
    let pipe = TestRayPipeline::new(&ctx);
    let (p64, p32) = (rk4::<f64>(), rk4::<f32>());
    let mut all = [
        Comparison::default(),
        Comparison::default(),
        Comparison::default(),
    ];
    let labels = ["GPU vs CPU-f32", "GPU vs CPU-f64", "CPU-f32 vs CPU-f64"];
    for cam in parity_cameras() {
        let k32 = Kerr::new(cam.kerr.spin() as f32).unwrap();
        let rays = camera_rays(&cam, SIDE, FOV_DEG);
        let gpu_in: Vec<GpuRay> = rays
            .iter()
            .map(|(x, p)| GpuRay {
                x: to_f32(x),
                p: to_f32(p),
            })
            .collect();
        let gpu: Vec<RayOutcome> = pipe
            .trace(&ctx, &k32, &p32, &gpu_in)
            .unwrap()
            .iter()
            .map(|r| RayOutcome::from_gpu(r.status, r.dir, r.steps))
            .collect();
        let cpu32: Vec<RayOutcome> = gpu_in
            .iter()
            .map(|r| RayOutcome::from(&trace_ray(&k32, &r.x, &r.p, &p32)))
            .collect();
        let cpu64: Vec<RayOutcome> = rays
            .iter()
            .map(|(x, p)| RayOutcome::from(&trace_ray(&cam.kerr, x, p, &p64)))
            .collect();
        let cmps = [
            compare(&gpu, &cpu32),
            compare(&gpu, &cpu64),
            compare(&cpu32, &cpu64),
        ];
        println!("{}:", cam.name);
        for (k, c) in cmps.iter().enumerate() {
            println!("  {:<19} {}", labels[k], c.summary());
            all[k].merge(c);
        }
    }
    println!("overall:");
    for (k, c) in all.iter().enumerate() {
        println!("  {:<19} {}", labels[k], c.summary());
    }
    report_assertions(&all);
}

fn report_assertions(all: &[Comparison; 3]) {
    // Every tracer saw the same rays.
    assert!(all.iter().all(|c| c.rays == all[0].rays && c.rays > 0));
    // PRD §9 test 15 thresholds (stated there for the final GPU tracer vs the CPU reference): status agreement
    // ≥ 99.5 % and ≥ 99 % of the agreeing escaped rays within 1e-3 rad. Here they are applied to the RK4 batch for
    // all three pairs. All three tracers run the same algorithm, so the only differences are f32 vs f64 rounding
    // and GPU vs CPU f32 rounding, amplified near the critical curve. Measured on an Apple M2 Pro: 100 % status
    // agreement; 99th-percentile angle 8e-5 rad (GPU vs CPU-f32), 6e-5 (GPU vs f64), 4e-5 (CPU f32 vs f64).
    const MIN_STATUS_AGREEMENT: f64 = 0.995;
    const ANGLE_TOL: f64 = 1e-3;
    const MIN_FRACTION_WITHIN: f64 = 0.99;
    for (label, c) in ["GPU vs CPU-f32", "GPU vs CPU-f64", "CPU-f32 vs CPU-f64"]
        .iter()
        .zip(all)
    {
        assert!(
            c.status_agreement() >= MIN_STATUS_AGREEMENT,
            "{label}: status agreement {:.4}",
            c.status_agreement()
        );
        assert!(
            c.fraction_within(ANGLE_TOL) >= MIN_FRACTION_WITHIN,
            "{label}: only {:.4} of rays within {ANGLE_TOL} rad",
            c.fraction_within(ANGLE_TOL)
        );
    }
}
