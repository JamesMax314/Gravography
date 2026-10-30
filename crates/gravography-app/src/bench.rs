//! `gravography bench`: GPU performance measurements for NF-1 (`docs/PRD.md` §8, WP-12).
//!
//! For each scene and render scale it renders single progressive samples (trace + shade, exactly what the viewer
//! does per frame while moving) into a persistent frame at `scale × output size`, waiting for the GPU after each
//! one, and reports the wall-clock time per sample. It also times a static convergence: all samples of the sample
//! target at full resolution, as when the viewer is still.

use std::path::PathBuf;
use std::time::Instant;

use anyhow::{Context, Result};
use gravography_physics::TraceParams;
use gravography_render::gpu::{GpuContext, GpuFrame, GpuRenderer, GpuSky};
use gravography_render::{FrameParams, Scene};

use crate::load_sky;

/// Options of `gravography bench`.
#[derive(clap::Args)]
pub struct BenchArgs {
    /// Scene files (default: the built-in default scene).
    #[arg(long)]
    pub scene: Vec<PathBuf>,
    /// Output width (the window's physical width).
    #[arg(long, default_value_t = 1920)]
    pub width: u32,
    /// Output height.
    #[arg(long, default_value_t = 1080)]
    pub height: u32,
    /// Render scales to time (internal resolution = scale × output).
    #[arg(long, value_delimiter = ',', default_values_t = [1.0, 0.75, 0.5])]
    pub scales: Vec<f64>,
    /// Timed samples per scale (after 2 warm-up samples).
    #[arg(long, default_value_t = 20)]
    pub frames: u32,
    /// Sample target of the static convergence run.
    #[arg(long, default_value_t = 16)]
    pub samples: u32,
    /// Override the GPU integrator's relative tolerance (absolute = rtol / 100).
    #[arg(long)]
    pub rtol: Option<f32>,
    /// Override the GPU step budget.
    #[arg(long)]
    pub max_steps: Option<u32>,
    /// Override the far-field step cap (coordinate displacement ≤ factor · r).
    #[arg(long)]
    pub far_step: Option<f32>,
    /// Override the near-horizon step cap (|Δr| ≤ factor · (r − r₊)).
    #[arg(long)]
    pub horizon_step: Option<f32>,
    /// Override the escape radius factor (r_escape = max(factor · r_camera, min)).
    #[arg(long)]
    pub escape_factor: Option<f32>,
    /// Override the minimum escape radius.
    #[arg(long)]
    pub escape_min: Option<f32>,
    /// Override the capture margin: captured once r < r₊ + eps while moving inward.
    #[arg(long)]
    pub horizon_eps: Option<f32>,
    /// Override the trace dispatch tile side in pixels (one submission per tile).
    #[arg(long)]
    pub tile: Option<u32>,
    /// Override the photon-orbit capture fraction κ (capture margin = max(eps, κ (r_ph,pro − r₊))).
    #[arg(long)]
    pub capture_fraction: Option<f32>,
    /// Override the momentum blow-up factor (captured once |p| > factor · |p₀|).
    #[arg(long)]
    pub blowup: Option<f32>,
    /// Also run the PRD §9 test 15 comparison (10 cameras, 64² rays each, vs the CPU f64 reference) with these
    /// parameters.
    #[arg(long)]
    pub parity: bool,
    /// Write the steps heat map (FR-R6) of each scene's last timed sample to `<dir>/<scene>-<scale>-steps.png`.
    #[arg(long)]
    pub steps_map: Option<PathBuf>,
    /// Do not read or write the baked-sky cache.
    #[arg(long)]
    pub no_cache: bool,
}

impl BenchArgs {
    /// GPU tracing parameters: the defaults with the overrides applied.
    pub fn trace_params(&self) -> TraceParams<f32> {
        let mut p = TraceParams::<f32>::gpu_default();
        if let Some(r) = self.rtol {
            p.rtol = r;
            p.atol = r * 1e-2;
        }
        if let Some(m) = self.max_steps {
            p.max_steps = m;
        }
        if let Some(f) = self.far_step {
            p.far_step_factor = f;
        }
        if let Some(f) = self.horizon_step {
            p.horizon_step_factor = f;
        }
        if let Some(f) = self.escape_factor {
            p.escape_factor = f;
        }
        if let Some(m) = self.escape_min {
            p.escape_min_radius = m;
        }
        if let Some(e) = self.horizon_eps {
            p.horizon_epsilon = e;
        }
        if let Some(k) = self.capture_fraction {
            p.capture_photon_fraction = k;
        }
        if let Some(b) = self.blowup {
            p.momentum_blowup = b;
        }
        p
    }
}

/// Mean and 95th percentile of a set of timings (ms).
pub fn summarize(ms: &mut [f64]) -> (f64, f64) {
    if ms.is_empty() {
        return (f64::NAN, f64::NAN);
    }
    ms.sort_by(f64::total_cmp);
    let mean = ms.iter().sum::<f64>() / ms.len() as f64;
    let i = ((ms.len() as f64 * 0.95).ceil() as usize).clamp(1, ms.len()) - 1;
    (mean, ms[i])
}

/// Runs the benchmark.
///
/// # Errors
/// Scene, sky or GPU errors.
pub fn bench(args: &BenchArgs) -> Result<()> {
    let ctx = GpuContext::new_headless()?.context("no GPU adapter available")?;
    println!("GPU: {}", ctx.describe());
    let mut renderer = GpuRenderer::new(&ctx);
    if let Some(t) = args.tile {
        renderer.tracer.tile = t;
    }
    let params = args.trace_params();
    println!(
        "output {}×{}, rtol {:.0e}, max steps {}, far step {}, horizon step {}, escape max({} r_cam, {})",
        args.width,
        args.height,
        params.rtol,
        params.max_steps,
        params.far_step_factor,
        params.horizon_step_factor,
        params.escape_factor,
        params.escape_min_radius
    );
    if args.parity {
        parity(&ctx, &renderer, &params)?;
    }
    let scenes: Vec<(String, Scene)> = if args.scene.is_empty() {
        vec![("default".into(), Scene::default())]
    } else {
        args.scene
            .iter()
            .map(|p| {
                let s = Scene::load(p).with_context(|| format!("loading {}", p.display()))?;
                Ok((
                    p.file_stem()
                        .map_or_else(String::new, |s| s.to_string_lossy().into()),
                    s,
                ))
            })
            .collect::<Result<_>>()?
    };
    let mut uploaded: Option<(gravography_render::scene::SkySpec, GpuSky)> = None;
    for (name, scene) in &scenes {
        if uploaded.as_ref().is_none_or(|(spec, _)| *spec != scene.sky) {
            let sky = load_sky(scene, args.no_cache)?;
            uploaded = Some((scene.sky, GpuSky::upload(&ctx, &sky)?));
        }
        let (_, sky) = uploaded.as_ref().expect("uploaded above");
        println!("\n{name}:");
        for &scale in &args.scales {
            let (w, h) = scaled(args.width, args.height, scale);
            let frame = frame_for(scene, w, h, args.samples)?;
            let target = GpuFrame::new(&ctx, w, h, params.max_steps);
            let mut ms = Vec::new();
            for i in 0..args.frames + 2 {
                let t = Instant::now();
                renderer.render_sample(&ctx, &frame, sky, &params, &target, i % args.samples)?;
                wait(&ctx)?;
                if i >= 2 {
                    ms.push(t.elapsed().as_secs_f64() * 1e3);
                }
            }
            let mut trace_ms = Vec::new();
            for i in 0..args.frames.min(5) {
                let t = Instant::now();
                renderer.tracer.trace(
                    &ctx,
                    &frame,
                    &params,
                    gravography_render::cpu::sample_offset(i),
                    &target.gbuffer,
                )?;
                wait(&ctx)?;
                trace_ms.push(t.elapsed().as_secs_f64() * 1e3);
            }
            let (trace_mean, _) = summarize(&mut trace_ms);
            println!("    trace only: {trace_mean:6.2} ms");
            let stats = renderer.read_stats(&ctx, &target)?;
            if let Some(dir) = &args.steps_map {
                let bytes = renderer.debug_view(&ctx, &target, gravography_render::gpu::DebugView::Steps)?;
                let path = dir.join(format!("{name}-{scale:.2}-steps.png"));
                gravography_render::output::write_rgb8_png(&path, w, h, bytes)?;
            }
            let (mean, p95) = summarize(&mut ms);
            println!(
                "  scale {scale:.2} ({w}×{h}): {mean:6.2} ms/sample (p95 {p95:6.2}) → {:5.1} fps; \
                 mean steps {:.0}, max {}",
                1e3 / mean,
                stats.steps as f64 / stats.rays.max(1) as f64,
                stats.max_steps
            );
        }
        let frame = frame_for(scene, args.width, args.height, args.samples)?;
        let target = GpuFrame::new(&ctx, args.width, args.height, params.max_steps);
        let t = Instant::now();
        for s in 0..args.samples {
            renderer.render_sample(&ctx, &frame, sky, &params, &target, s)?;
        }
        wait(&ctx)?;
        println!(
            "  static convergence: {} samples at {}×{} in {:.0} ms",
            args.samples,
            args.width,
            args.height,
            t.elapsed().as_secs_f64() * 1e3
        );
    }
    Ok(())
}

/// PRD §9 test 15 with `params` (as `crates/gravography-render/tests/gpu_tracer.rs`, 64² rays per camera, 90°
/// field of view): status agreement, escape-direction errors and mean GPU steps.
fn parity(ctx: &GpuContext, renderer: &GpuRenderer, params: &TraceParams<f32>) -> Result<()> {
    use gravography_render::cpu::status_from_index;
    use gravography_render::parity::{Comparison, RayOutcome, compare, frame_for, parity_cameras};
    use gravography_render::{CpuRenderer, CpuSettings, Sky};
    use gravography_sky::cubemap::CubeLevel;
    use gravography_sky::{BlackbodyLut, CubeMap};

    let side = 64;
    let sky = Sky::new(
        CubeMap::with_mips(CubeLevel::zeros(1)),
        BlackbodyLut::human_vision(),
    );
    let mut all = Comparison::default();
    let mut steps = 0_u64;
    let t = Instant::now();
    for cam in parity_cameras() {
        let frame = frame_for(&cam, side, 90.0);
        let gpu = renderer.tracer.trace_to_cpu(ctx, &frame, params)?;
        let cpu = CpuRenderer::new(&frame, &sky, CpuSettings::default())?;
        let n = (side * side) as usize;
        let threads = std::thread::available_parallelism().map_or(4, |n| n.get());
        let chunk = n.div_ceil(threads);
        let cpu_out: Vec<RayOutcome> = std::thread::scope(|s| {
            let handles: Vec<_> = (0..n)
                .step_by(chunk)
                .map(|start| {
                    let cpu = &cpu;
                    s.spawn(move || {
                        (start..(start + chunk).min(n))
                            .map(|k| {
                                let k = k as u32;
                                let h = cpu.cast_pixel(f64::from(k % side) + 0.5, f64::from(k / side) + 0.5);
                                RayOutcome {
                                    status: h.status,
                                    dir: h.dir,
                                    steps: h.steps,
                                }
                            })
                            .collect::<Vec<_>>()
                    })
                })
                .collect();
            handles
                .into_iter()
                .flat_map(|h| h.join().expect("CPU reference thread panicked"))
                .collect()
        });
        let gpu_out: Vec<RayOutcome> = gpu
            .iter()
            .map(|g| RayOutcome::from_gpu(g.status, g.dir_g, g.steps))
            .collect();
        debug_assert!(gpu.iter().all(|g| status_from_index(g.status).is_some()));
        steps += gpu.iter().map(|g| u64::from(g.steps + g.rejected)).sum::<u64>();
        all.merge(&compare(&gpu_out, &cpu_out));
    }
    println!(
        "parity (test 15, 64²×10): {}; within 1e-3 rad {:.4}; GPU mean steps {:.0} ({:.1} s)",
        all.summary(),
        all.fraction_within(1e-3),
        steps as f64 / all.rays as f64,
        t.elapsed().as_secs_f64()
    );
    Ok(())
}

fn scaled(w: u32, h: u32, s: f64) -> (u32, u32) {
    let f = |n: u32| ((f64::from(n) * s).round() as u32).max(1);
    (f(w), f(h))
}

fn frame_for(scene: &Scene, w: u32, h: u32, samples: u32) -> Result<FrameParams> {
    let mut s = scene.clone();
    s.camera.width = w;
    s.camera.height = h;
    s.camera.samples = samples;
    Ok(s.frame()?)
}

fn wait(ctx: &GpuContext) -> Result<()> {
    ctx.device
        .poll(wgpu::PollType::wait_indefinitely())
        .map_err(|e| anyhow::anyhow!("poll failed: {e}"))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn summary_statistics() {
        let mut v = vec![5.0, 1.0, 3.0, 2.0, 4.0];
        let (mean, p95) = summarize(&mut v);
        assert!((mean - 3.0).abs() < 1e-12);
        assert_eq!(p95, 5.0);
        let mut one = vec![7.0];
        assert_eq!(summarize(&mut one), (7.0, 7.0));
        assert!(summarize(&mut []).0.is_nan());
        assert_eq!(scaled(1920, 1080, 0.5), (960, 540));
    }
}
