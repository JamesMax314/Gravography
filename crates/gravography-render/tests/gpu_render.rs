//! WP-09 acceptance: GPU shading and headless GPU render against the CPU reference (PRD §9 test 16), tone mapping
//! and debug views (FR-R6). Needs a GPU adapter; without one the tests print `SKIPPED …` and pass.

use std::path::{Path, PathBuf};

use gravography_physics::{RayStatus, TraceParams};
use gravography_render::cpu::status_index;
use gravography_render::gpu::{DebugView, GpuContext, GpuFrame, GpuRenderer, GpuSky, GpuStats, GpuTimer};
use gravography_render::parity::{frame_for, parity_cameras};
use gravography_render::tonemap::{luminance, to_srgb8};
use gravography_render::{CpuRenderer, CpuSettings, FrameParams, HdrImage, Scene, Sky, display_scale};
use gravography_sky::colour::{mat3_vec, xyz_to_linear_srgb_matrix};
use gravography_sky::cubemap::CubeLevel;
use gravography_sky::{BlackbodyLut, CubeMap, bake_sky};

fn scene_files() -> Vec<PathBuf> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scenes");
    let mut v: Vec<PathBuf> = std::fs::read_dir(dir)
        .expect("scenes/ exists")
        .map(|e| e.expect("readable entry").path())
        .filter(|p| p.extension().is_some_and(|e| e == "toml"))
        .collect();
    v.sort();
    v
}

/// Image comparison statistics (HDR luminance and display bytes).
#[derive(Debug)]
#[allow(dead_code)] // `mean_bytes` is reported via Debug only.
struct ImageDiff {
    /// |ΣY_gpu − ΣY_cpu| / ΣY_cpu.
    flux: f64,
    /// Mean of |Y_gpu − Y_cpu| / (Y_cpu + 0.1 Ȳ_cpu) over pixels.
    mean_rel: f64,
    /// Fraction of pixels with that relative error > 0.1.
    frac_rel_gt_10pct: f64,
    /// Fraction of pixels whose tone-mapped 8-bit value differs by more than 8 levels in any channel.
    frac_bytes_gt_8: f64,
    /// Mean absolute 8-bit difference over all channels.
    mean_bytes: f64,
}

fn diff(gpu: &HdrImage, cpu: &HdrImage, scale: f64, frame: &FrameParams) -> ImageDiff {
    let y = |p: &[f32; 3]| luminance(&p.map(f64::from));
    let (yg, yc): (Vec<f64>, Vec<f64>) = gpu
        .pixels
        .iter()
        .zip(&cpu.pixels)
        .map(|(g, c)| (y(g), y(c)))
        .unzip();
    let n = yc.len() as f64;
    let (sg, sc): (f64, f64) = (yg.iter().sum(), yc.iter().sum());
    let floor = 0.1 * sc / n;
    let rel: Vec<f64> = yg
        .iter()
        .zip(&yc)
        .map(|(g, c)| (g - c).abs() / (c + floor))
        .collect();
    let bg = to_srgb8(&gpu.pixels, scale, frame.tone_map);
    let bc = to_srgb8(&cpu.pixels, scale, frame.tone_map);
    let byte_diff: Vec<i32> = bg
        .iter()
        .zip(&bc)
        .map(|(a, b)| (i32::from(*a) - i32::from(*b)).abs())
        .collect();
    let px_over = byte_diff.chunks(3).filter(|c| c.iter().any(|&d| d > 8)).count();
    ImageDiff {
        flux: (sg - sc).abs() / sc,
        mean_rel: rel.iter().sum::<f64>() / n,
        frac_rel_gt_10pct: rel.iter().filter(|&&r| r > 0.1).count() as f64 / n,
        frac_bytes_gt_8: px_over as f64 / n,
        mean_bytes: byte_diff.iter().map(|&d| f64::from(d)).sum::<f64>() / byte_diff.len() as f64,
    }
}

/// A scene at test size with a small sky (face 256, 2·10⁴ stars): CPU-feasible, same physics and shading paths.
fn test_scene(path: &Path, w: u32, h: u32, samples: u32) -> (FrameParams, Sky) {
    let mut scene = Scene::load(path).expect("scene parses");
    scene.camera.width = w;
    scene.camera.height = h;
    scene.camera.samples = samples;
    scene.sky.face_size = 256;
    scene.sky.star_count = 20_000;
    let lut = BlackbodyLut::human_vision();
    let (_, cube) = bake_sky(&scene.sky_params().expect("valid sky"), &lut).expect("bake");
    (scene.frame().expect("valid frame"), Sky::new(cube, lut))
}

fn test16(w: u32, h: u32, samples: u32, name: &str) {
    let Some(ctx) = GpuContext::for_tests(name) else {
        return;
    };
    println!("adapter: {}", ctx.describe());
    let renderer = GpuRenderer::new(&ctx);
    for path in scene_files() {
        let (frame, sky) = test_scene(&path, w, h, samples);
        let gpu_sky = GpuSky::upload(&ctx, &sky).expect("upload");
        let out = renderer
            .render(&ctx, &frame, &gpu_sky, &TraceParams::gpu_default())
            .expect("GPU render");
        let gpu = renderer.read_hdr(&ctx, &out).expect("readback");
        let (cpu, _) = CpuRenderer::new(&frame, &sky, CpuSettings::default())
            .expect("valid frame")
            .render();
        // The CPU with the GPU's footprint method (one-pixel finite differences): isolates the f32 port.
        let same_method = CpuSettings {
            differential_offset: 1.0,
            ..CpuSettings::default()
        };
        let (cpu1, _) = CpuRenderer::new(&frame, &sky, same_method)
            .expect("valid frame")
            .render();
        let scale = display_scale(&frame, &cpu, &sky);
        let d = diff(&gpu, &cpu, scale, &frame);
        let d1 = diff(&gpu, &cpu1, scale, &frame);
        let file = path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned();
        println!("{file} vs CPU reference (ε = 0.25 px): {d:?}");
        println!("{file} vs CPU with ε = 1 px:          {d1:?}");

        // Against the reference (docs/physics.md § "GPU shading", test 16). Differences come from the footprint
        // (GPU: neighbouring pixels, ±1 px; CPU: differential rays at ±0.25 px), which changes the mip level where
        // the lens map curves within a pixel (near the critical curve and the photon rings); the ε = 1 comparison
        // below shows that almost nothing else differs. Worst measured at 96 × 54 (inside_horizon, the most
        // strongly lensed scene): 0.37 % total luminance (fast_observer), 2.0 % mean relative pixel luminance,
        // 3.3 % of pixels off by > 8 display levels. Bounds ≈ 1.8–2.7× those (other GPUs round differently).
        assert!(d.flux < 0.01, "{file}: total luminance differs by {:.2e}", d.flux);
        assert!(
            d.mean_rel < 0.04,
            "{file}: mean relative luminance error {:.2e}",
            d.mean_rel
        );
        assert!(
            d.frac_bytes_gt_8 < 0.06,
            "{file}: {:.2e} of pixels differ by > 8 levels",
            d.frac_bytes_gt_8
        );

        // Against the same method: only the f32 tracer and shading differ. Worst measured: 1.4e-4 total
        // luminance, 1.1e-3 mean relative (inside_horizon), 0.19 % of pixels off by > 8 levels (rays whose status
        // flips at the shadow edge).
        assert!(
            d1.flux < 1e-3,
            "{file}: (ε = 1) total luminance differs by {:.2e}",
            d1.flux
        );
        assert!(
            d1.mean_rel < 3e-3,
            "{file}: (ε = 1) mean relative error {:.2e}",
            d1.mean_rel
        );
        assert!(
            d1.frac_bytes_gt_8 < 0.005,
            "{file}: (ε = 1) {:.2e} of pixels off",
            d1.frac_bytes_gt_8
        );
    }
}

/// Default test-16 size: 96 × 54, 1 sample (the CPU f64 references dominate the run time in the test profile).
#[test]
fn test16_gpu_images_match_cpu_reference() {
    test16(96, 54, 1, "test16_gpu_images_match_cpu_reference");
}

#[test]
#[ignore = "slow (≈ 2–3 min): test 16 at 192 × 108 with 2 samples; run before ending a WP that touches shading"]
fn test16_larger() {
    test16(192, 108, 2, "test16_larger");
}

#[test]
fn gpu_tonemap_matches_cpu() {
    // The GPU tone-map pass reproduces tonemap::to_srgb8 on the same HDR data up to f32 rounding: ±1 level.
    let Some(ctx) = GpuContext::for_tests("gpu_tonemap_matches_cpu") else {
        return;
    };
    let renderer = GpuRenderer::new(&ctx);
    let path = &scene_files()[0];
    let (frame, sky) = test_scene(path, 96, 54, 1);
    let gpu_sky = GpuSky::upload(&ctx, &sky).expect("upload");
    let out = renderer
        .render(&ctx, &frame, &gpu_sky, &TraceParams::gpu_default())
        .expect("GPU render");
    let hdr = renderer.read_hdr(&ctx, &out).expect("readback");
    for op in [
        gravography_render::ToneMap::Aces,
        gravography_render::ToneMap::Reinhard,
    ] {
        for scale in [display_scale(&frame, &hdr, &sky), 1e-3, 1e-6] {
            let gpu = renderer.tonemap(&ctx, &out, scale, op).expect("tonemap");
            let cpu = to_srgb8(&hdr.pixels, scale, op);
            let worst = gpu
                .iter()
                .zip(&cpu)
                .map(|(a, b)| (i32::from(*a) - i32::from(*b)).abs())
                .max()
                .unwrap_or(0);
            assert!(
                worst <= 1,
                "{op:?} at scale {scale:e}: max byte difference {worst}"
            );
        }
    }
}

#[test]
fn uniform_sky_is_exact_on_gpu() {
    // A uniform (w, T) sky seen by a static Schwarzschild observer: every escaped pixel shows w · XYZ(gT) with the
    // uniform g = (1 − 2/r)^{-1/2} (PRD §9 test 10; surface brightness is conserved, so lensing must not matter).
    // f32 LUT interpolation and matrix products: relative error ≲ 1e-5 (ln S ≈ 30 carries ~4e-6 into exp).
    let Some(ctx) = GpuContext::for_tests("uniform_sky_is_exact_on_gpu") else {
        return;
    };
    let mut base = CubeLevel::zeros(16);
    base.data.fill([0.5, 0.5 * 5000.0]);
    let sky = Sky::new(CubeMap::with_mips(base), BlackbodyLut::human_vision());
    let cam = &parity_cameras()[0];
    // One sample per pixel (pixel centres): every pixel is fully lit or fully black, no partial edge pixels.
    let frame = frame_for(cam, 48, 40.0);
    let renderer = GpuRenderer::new(&ctx);
    let out = renderer
        .render(
            &ctx,
            &frame,
            &GpuSky::upload(&ctx, &sky).expect("upload"),
            &TraceParams::gpu_default(),
        )
        .expect("render");
    let hdr = renderer.read_hdr(&ctx, &out).expect("readback");
    let g = (1.0 - 2.0 / 30.0_f64).powf(-0.5);
    let xyz = sky.lut.lookup(5000.0 * g).response.map(|c| 0.5 * c);
    let expect = mat3_vec(&xyz_to_linear_srgb_matrix(), &xyz);
    let mut lit = 0;
    for p in &hdr.pixels {
        if p[1] > 0.0 {
            lit += 1;
            for k in 0..3 {
                let rel = (f64::from(p[k]) / expect[k] - 1.0).abs();
                assert!(
                    rel < 1e-5,
                    "channel {k}: {} vs {} (rel {rel:.2e})",
                    p[k],
                    expect[k]
                );
            }
        }
    }
    // Static camera at r = 30, 40° field: the shadow (radius ≈ 9.6°) is a minority of the image.
    assert!(lit > 48 * 48 / 2, "only {lit} lit pixels");
}

#[test]
fn debug_views_render() {
    // FR-R6: status view colours follow the G-buffer statuses exactly; the g and steps views produce images.
    let Some(ctx) = GpuContext::for_tests("debug_views_render") else {
        return;
    };
    let path = &scene_files()[1];
    let (frame, sky) = test_scene(path, 64, 36, 1);
    let renderer = GpuRenderer::new(&ctx);
    let out = renderer
        .render(
            &ctx,
            &frame,
            &GpuSky::upload(&ctx, &sky).expect("upload"),
            &TraceParams::gpu_default(),
        )
        .expect("render");
    let texels = out.gbuffer.read(&ctx).expect("readback");
    let status = renderer.debug_view(&ctx, &out, DebugView::Status).expect("debug");
    assert_eq!(status.len(), texels.len() * 3);
    let stats = renderer.read_stats(&ctx, &out).expect("stats");
    assert!(stats.status[status_index(RayStatus::Captured)] > 0 && stats.status[0] > 0);
    for (t, rgb) in texels.iter().zip(status.chunks(3)) {
        let expect: [u8; 3] = match t.status {
            0 => [51, 102, 255],
            1 => [0, 0, 0],
            2 => [255, 255, 0],
            3 => [255, 0, 0],
            _ => [255, 0, 255],
        };
        assert_eq!(rgb, expect);
    }
    for view in [DebugView::Shift, DebugView::Steps] {
        let img = renderer.debug_view(&ctx, &out, view).expect("debug");
        assert_eq!(img.len(), texels.len() * 3);
        // Not a constant image: the shadow and the sky differ.
        assert!(img.chunks(3).any(|c| c != &img[..3]), "{view:?} view is uniform");
    }
}

#[test]
fn gpu_render_is_deterministic() {
    // NF-3: GPU images are identical on the same machine.
    let Some(ctx) = GpuContext::for_tests("gpu_render_is_deterministic") else {
        return;
    };
    let path = &scene_files()[2];
    let (frame, sky) = test_scene(path, 64, 36, 2);
    let renderer = GpuRenderer::new(&ctx);
    let gpu_sky = GpuSky::upload(&ctx, &sky).expect("upload");
    let a = renderer
        .render(&ctx, &frame, &gpu_sky, &TraceParams::gpu_default())
        .and_then(|o| renderer.read_hdr(&ctx, &o))
        .expect("render");
    let b = renderer
        .render(&ctx, &frame, &gpu_sky, &TraceParams::gpu_default())
        .and_then(|o| renderer.read_hdr(&ctx, &o))
        .expect("render");
    assert!(a == b);
}

#[test]
fn present_pass_matches_tonemap() {
    // The viewer's present pass (fragment shader into a non-sRGB target, shader-side sRGB encoding) must show the
    // same pixels as the headless tone map when the destination rectangle equals the image (1:1, nearest): ±1
    // level for the different rounding paths (8-bit UNORM store vs floor(x + 0.5)).
    let Some(ctx) = GpuContext::for_tests("present_pass_matches_tonemap") else {
        return;
    };
    let path = &scene_files()[3];
    let (frame, sky) = test_scene(path, 64, 40, 1);
    let renderer = GpuRenderer::new(&ctx);
    let out = renderer
        .render(
            &ctx,
            &frame,
            &GpuSky::upload(&ctx, &sky).expect("upload"),
            &TraceParams::gpu_default(),
        )
        .expect("render");
    let hdr = renderer.read_hdr(&ctx, &out).expect("readback");
    let scale = display_scale(&frame, &hdr, &sky);
    let expect = renderer
        .tonemap(&ctx, &out, scale, frame.tone_map)
        .expect("tonemap");

    let format = wgpu::TextureFormat::Rgba8Unorm;
    let presenter = gravography_render::gpu::Presenter::new(&ctx, format);
    let (w, h) = (frame.width, frame.height);
    let texture = ctx.device.create_texture(&wgpu::TextureDescriptor {
        label: Some("present target"),
        size: wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    // Rows padded to the 256-byte copy alignment.
    let row = (w * 4).div_ceil(256) * 256;
    let readback = ctx.device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("present readback"),
        size: u64::from(row * h),
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let mut encoder = ctx
        .device
        .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
    {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("present test"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::RED),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        presenter.draw(
            &ctx,
            &mut pass,
            &out,
            scale,
            frame.tone_map,
            [0.0, 0.0, w as f32, h as f32],
        );
    }
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture: &texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &readback,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(row),
                rows_per_image: Some(h),
            },
        },
        wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
    );
    ctx.queue.submit([encoder.finish()]);
    let slice = readback.slice(..);
    slice.map_async(wgpu::MapMode::Read, |r| r.expect("map"));
    ctx.device
        .poll(wgpu::PollType::wait_indefinitely())
        .expect("poll");
    let data = slice.get_mapped_range().expect("mapped").to_vec();
    let mut worst = 0;
    for y in 0..h as usize {
        for x in 0..w as usize {
            let p = &data[y * row as usize + 4 * x..][..3];
            let e = &expect[3 * (y * w as usize + x)..][..3];
            for k in 0..3 {
                worst = worst.max((i32::from(p[k]) - i32::from(e[k])).abs());
            }
        }
    }
    assert!(
        worst <= 1,
        "present pass differs from the tone map by {worst} levels"
    );
}

#[test]
fn gpu_timer_measures_a_sample() {
    // WP-12: the GPU busy time of a sample is positive, no longer than the blocking wall-clock time of the same
    // submission, and comes back with its tag. How it scales with the pixel count is printed but not asserted:
    // other GPU tests running in parallel share the device and inflate busy times unpredictably (CLAUDE.md §4).
    let Some(ctx) = GpuContext::for_tests("gpu_timer_measures_a_sample") else {
        return;
    };
    let path = &scene_files()[2];
    let renderer = GpuRenderer::new(&ctx);
    let params = TraceParams::gpu_default();
    let mut timer = GpuTimer::new();
    let mut time = |w: u32, h: u32| {
        let (frame, sky) = test_scene(path, w, h, 1);
        let gpu_sky = GpuSky::upload(&ctx, &sky).expect("upload");
        let target = GpuFrame::new(&ctx, frame.width, frame.height, 2000);
        let _ = timer.wait(&ctx);
        let mut best = f64::INFINITY;
        for i in 0..3 {
            let t = std::time::Instant::now();
            timer.begin();
            renderer
                .render_sample(&ctx, &frame, &gpu_sky, &params, &target, i)
                .expect("render");
            timer.finish(&ctx, w as f32);
            let (ms, tag) = timer.wait(&ctx).expect("completion is reported after waiting");
            assert_eq!(tag, w as f32);
            let wall = t.elapsed().as_secs_f64() * 1e3;
            assert!(ms > 0.0 && ms <= wall, "GPU {ms} ms vs wall {wall} ms");
            best = best.min(ms);
        }
        best
    };
    let small = time(320, 180);
    let large = time(1280, 720);
    println!("GPU time: {small:.3} ms at 320×180, {large:.3} ms at 1280×720");
    assert_eq!(timer.wait(&ctx), None, "nothing left in flight");
}

#[test]
fn gpu_stats_match_cpu_readbacks() {
    // WP-12: the GPU statistics pass reproduces the CPU log-average luminance of the read-back image (f32 partial
    // sums of ln Y over 256 pixels, combined in f64: relative error ≲ 1e-5 for these sizes; bound 1e-4) and the
    // exact ray-status counts of the G-buffer. Sizes that are not multiples of 256 exercise the partial last group.
    let Some(ctx) = GpuContext::for_tests("gpu_stats_match_cpu_readbacks") else {
        return;
    };
    let renderer = GpuRenderer::new(&ctx);
    let mut stats = GpuStats::new(&ctx);
    for (k, path) in scene_files().iter().enumerate() {
        let (frame, sky) = test_scene(path, 333 + 7 * k as u32, 187, 2);
        let gpu_sky = GpuSky::upload(&ctx, &sky).expect("upload");
        let out = renderer
            .render(&ctx, &frame, &gpu_sky, &TraceParams::gpu_default())
            .expect("render");
        assert!(stats.request(&ctx, &out));
        let got = stats.wait(&ctx).expect("statistics arrive after waiting");
        let hdr = renderer.read_hdr(&ctx, &out).expect("hdr");
        let cpu = gravography_render::tonemap::log_average_luminance(&hdr.pixels);
        let counts = renderer.read_stats(&ctx, &out).expect("stats").status;
        println!(
            "{}: GPU {:?} CPU {cpu:?}; status {:?}",
            path.display(),
            got.log_average,
            got.status
        );
        assert_eq!(got.status, counts, "{}", path.display());
        match (got.log_average, cpu) {
            (Some(g), Some(c)) => assert!((g / c - 1.0).abs() < 1e-4, "{g} vs {c}"),
            (g, c) => assert_eq!(g.is_some(), c.is_some()),
        }
    }
    assert_eq!(stats.wait(&ctx), None, "nothing in flight");
}
