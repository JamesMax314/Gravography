//! WP-08 acceptance: the per-pixel GPU DOPRI5 tracer against the CPU f64 reference (PRD §9 test 15).
//! Needs a GPU adapter; without one the tests print `SKIPPED …` and pass (`CLAUDE.md` §4).

use gravography_physics::{Kerr, RayStatus, TraceParams, trace_ray};
use gravography_render::cpu::status_from_index;
use gravography_render::gpu::{GBufferTexel, GpuContext, GpuRay, GpuTracer, TestRayPipeline};
use gravography_render::parity::{
    Comparison, RayOutcome, camera_rays, compare, frame_for, near_status_boundary, parity_cameras, to_f32,
};
use gravography_render::{CpuRenderer, CpuSettings, Effects, FrameParams, Sky};
use gravography_sky::cubemap::CubeLevel;
use gravography_sky::{BlackbodyLut, CubeMap};
use rayon::prelude::*;

/// Pixels per side of each parity frame in the default run (64² = 4096 rays per camera, 40 960 in total; the CPU
/// f64 reference dominates the run time). The ignored test uses 128².
const SIDE: u32 = 64;
const SIDE_FULL: u32 = 128;
const FOV_DEG: f64 = 90.0;

fn dummy_sky() -> Sky {
    // The tracer comparison never shades; a 1×1 sky satisfies CpuRenderer's constructor.
    Sky::new(
        CubeMap::with_mips(CubeLevel::zeros(1)),
        BlackbodyLut::human_vision(),
    )
}

fn gpu_outcome(t: &GBufferTexel) -> RayOutcome {
    RayOutcome::from_gpu(t.status, t.dir_g, t.steps)
}

/// CPU f64 reference (`rtol = 1e-10`) at the pixel centres of `frame`: outcomes and g.
fn cpu_reference(frame: &FrameParams) -> (Vec<RayOutcome>, Vec<f64>) {
    let sky = dummy_sky();
    let r = CpuRenderer::new(frame, &sky, CpuSettings::default()).expect("valid parity frame");
    let w = frame.width;
    let hits: Vec<_> = (0..w * frame.height)
        .into_par_iter()
        .map(|k| r.cast_pixel(f64::from(k % w) + 0.5, f64::from(k / w) + 0.5))
        .collect();
    let out = hits
        .iter()
        .map(|h| RayOutcome {
            status: h.status,
            dir: h.dir,
            steps: h.steps,
        })
        .collect();
    (out, hits.iter().map(|h| h.g).collect())
}

#[test]
fn test15_gpu_tracer_matches_cpu_reference() {
    test15(SIDE, "test15_gpu_tracer_matches_cpu_reference");
}

#[test]
#[ignore = "slow (≈ 1 min): test 15 at 128² per camera; run before ending a WP that touches the tracer"]
fn test15_full_resolution() {
    test15(SIDE_FULL, "test15_full_resolution");
}

fn test15(side: u32, name: &str) {
    let Some(ctx) = GpuContext::for_tests(name) else {
        return;
    };
    println!("adapter: {}", ctx.describe());
    let tracer = GpuTracer::new(&ctx);
    let params = TraceParams::<f32>::gpu_default();
    let mut all = Comparison::default();
    let (mut far_from_boundary, mut gpu_max_steps, mut worst_g) = (0_usize, 0_u64, 0.0_f64);
    let mut steps_total = 0_u64;
    for cam in parity_cameras() {
        let frame = frame_for(&cam, side, FOV_DEG);
        let gpu = tracer
            .trace_to_cpu(&ctx, &frame, &params)
            .expect("GPU trace succeeds");
        let gpu_out: Vec<RayOutcome> = gpu.iter().map(gpu_outcome).collect();
        let (cpu_out, cpu_g) = cpu_reference(&frame);
        let c = compare(&gpu_out, &cpu_out);
        // Disagreements must sit next to a status boundary of the reference (the critical curve). A radius of 2 px
        // allows for the boundary moving by up to ~2 pixels under the GPU's f32 / rtol = 1e-5 errors.
        let cpu_status: Vec<RayStatus> = cpu_out.iter().map(|o| o.status).collect();
        let mut far = 0;
        for (i, (a, b)) in gpu_out.iter().zip(&cpu_out).enumerate() {
            if a.status != b.status && !near_status_boundary(&cpu_status, side as usize, i, 2) {
                far += 1;
            }
            if a.status == RayStatus::Escaped && b.status == RayStatus::Escaped {
                worst_g = worst_g.max((f64::from(gpu[i].dir_g[3]) / cpu_g[i] - 1.0).abs());
            }
        }
        let max_steps = gpu_out.iter().filter(|o| o.status == RayStatus::MaxSteps).count();
        let steps: u64 = gpu.iter().map(|t| u64::from(t.steps + t.rejected)).sum();
        println!(
            "{:<28} {}; far from boundary {far}; GPU max-steps {max_steps}; GPU mean steps {:.0}",
            cam.name,
            c.summary(),
            steps as f64 / gpu.len() as f64
        );
        far_from_boundary += far;
        gpu_max_steps += max_steps as u64;
        steps_total += steps;
        all.merge(&c);
    }
    println!("overall: {}", all.summary());
    println!(
        "within 1e-3 rad: {:.4}; disagreements far from a boundary: {far_from_boundary}; GPU max-steps: \
         {gpu_max_steps}; worst |g_gpu/g_cpu − 1|: {worst_g:.2e}; GPU mean steps {:.0}",
        all.fraction_within(1e-3),
        steps_total as f64 / all.rays as f64
    );
    // PRD §9 test 15: status agreement ≥ 99.5 % overall, disagreements confined to near the critical curve, and
    // escape-direction error ≤ 1e-3 rad for ≥ 99 % of agreeing (escaped) rays.
    assert!(
        all.status_agreement() >= 0.995,
        "status agreement {:.4}",
        all.status_agreement()
    );
    assert_eq!(
        far_from_boundary, 0,
        "status disagreements away from any status boundary"
    );
    assert!(
        all.fraction_within(1e-3) >= 0.99,
        "within 1e-3 rad: {:.4}",
        all.fraction_within(1e-3)
    );
    // g = 1/p_t is computed at the camera from the f32 tetrad and metric: f32 rounding of O(1) quantities with
    // γ ≲ 3 amplification (boosted camera) — measured ≲ 1e-6; 1e-5 is the bound.
    assert!(worst_g < 1e-5, "g relative error {worst_g:.2e}");
}

#[test]
fn dopri5_test_rays_match_cpu_f32() {
    // Same algorithm, same f32 inputs: the GPU DOPRI5 tracer reproduces the CPU f32 DOPRI5 tracer up to rounding
    // (FMA contraction, transcendental functions), which accepted/rejected-step decisions can amplify.
    let Some(ctx) = GpuContext::for_tests("dopri5_test_rays_match_cpu_f32") else {
        return;
    };
    let pipe = TestRayPipeline::new(&ctx);
    let p32 = TraceParams::<f32>::gpu_default();
    let mut all = Comparison::default();
    for cam in parity_cameras() {
        let k32 = Kerr::new(cam.kerr.spin() as f32).unwrap();
        let rays: Vec<GpuRay> = camera_rays(&cam, 32, FOV_DEG)
            .iter()
            .map(|(x, p)| GpuRay {
                x: to_f32(x),
                p: to_f32(p),
            })
            .collect();
        let gpu: Vec<RayOutcome> = pipe
            .trace_dopri5(&ctx, &k32, &p32, &rays)
            .unwrap()
            .iter()
            .map(|r| RayOutcome::from_gpu(r.status, r.dir, r.steps))
            .collect();
        let cpu: Vec<RayOutcome> = rays
            .iter()
            .map(|r| RayOutcome::from(&trace_ray(&k32, &r.x, &r.p, &p32)))
            .collect();
        all.merge(&compare(&gpu, &cpu));
    }
    println!("GPU DOPRI5 vs CPU-f32 DOPRI5: {}", all.summary());
    assert!(all.status_agreement() >= 0.995);
    assert!(all.fraction_within(1e-3) >= 0.99);
}

#[test]
fn tiling_does_not_change_the_result() {
    // Tiles are independent dispatches over disjoint pixels, so the G-buffer must be bit-identical for any tile
    // size (and across runs: NF-3 GPU determinism on one machine).
    let Some(ctx) = GpuContext::for_tests("tiling_does_not_change_the_result") else {
        return;
    };
    let cam = &parity_cameras()[5];
    let mut frame = frame_for(cam, 0, FOV_DEG);
    frame.width = 100;
    frame.height = 70;
    let params = TraceParams::<f32>::gpu_default();
    let mut tracer = GpuTracer::new(&ctx);
    let big = tracer.trace_to_cpu(&ctx, &frame, &params).unwrap();
    tracer.tile = 24;
    let small = tracer.trace_to_cpu(&ctx, &frame, &params).unwrap();
    assert_eq!(big.len(), 7000);
    assert!(big == small, "tiled G-buffer differs from the single-tile one");
    assert!(big.iter().all(|t| status_from_index(t.status).is_some()));
}

#[test]
fn lensing_off_and_no_frequency_shift() {
    // Effect flags: lensing off gives the straight-ray direction of CpuRenderer::cast (f32 rounding of unit
    // vectors: ≲ 1e-6 rad); frequency shift off gives g = 1 exactly.
    let Some(ctx) = GpuContext::for_tests("lensing_off_and_no_frequency_shift") else {
        return;
    };
    let cam = &parity_cameras()[2];
    let mut frame = frame_for(cam, 40, FOV_DEG);
    frame.effects = Effects {
        lensing: false,
        frequency_shift: false,
        beaming: true,
    };
    let gpu = GpuTracer::new(&ctx)
        .trace_to_cpu(&ctx, &frame, &TraceParams::gpu_default())
        .unwrap();
    let (cpu, _) = cpu_reference(&frame);
    let c = compare(&gpu.iter().map(gpu_outcome).collect::<Vec<_>>(), &cpu);
    assert_eq!(c.status_agreement(), 1.0);
    assert!(c.angle_quantile(1.0).unwrap() < 1e-5, "{}", c.summary());
    // g = 1 is assigned exactly (no arithmetic), so compare bit patterns.
    assert!(
        gpu.iter()
            .all(|t| t.dir_g[3].to_bits() == 1.0_f32.to_bits() && t.steps == 0)
    );
}
