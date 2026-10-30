//! `gravography`: interactive black-hole viewer (`docs/PRD.md` §4.2) and headless `render` CLI (FR-X2).

mod bench;
mod viewer;

use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand, ValueEnum};
use gravography_physics::RayStatus;
use gravography_physics::TraceParams;
use gravography_render::cpu::{RenderStats, status_index};
use gravography_render::gpu::{DebugView, GpuContext, GpuRenderer, GpuSky};
use gravography_render::output::write_rgb8_png;
use gravography_render::{
    CpuRenderer, FrameParams, HdrImage, OutputFormat, Scene, Sky, display_scale, write_image,
};
use gravography_sky::{BlackbodyLut, bake_sky_cached};

#[derive(Parser)]
#[command(
    name = "gravography",
    version,
    about = "Real-time Kerr black-hole ray tracer",
    args_conflicts_with_subcommands = true
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
    #[command(flatten)]
    view: ViewArgs,
}

/// Options of the interactive viewer (no subcommand).
#[derive(clap::Args)]
struct ViewArgs {
    /// Scene file to open (default: the built-in default scene, a static observer at r = 30 M).
    #[arg(long)]
    scene: Option<PathBuf>,
    /// Directory for screenshots (key P).
    #[arg(long, default_value = "screenshots")]
    screenshot_dir: PathBuf,
    /// Scripted check: render, screenshot, resize the window, render, screenshot, fall through the horizon,
    /// screenshot, exit.
    #[arg(long)]
    smoke_test: bool,
    /// Do not read or write the baked-sky cache.
    #[arg(long)]
    no_cache: bool,
    /// Start from this bookmark (RON, saved from the viewer); its sky replaces the scene's.
    #[arg(long)]
    bookmark: Option<PathBuf>,
    /// Directory for bookmarks.
    #[arg(long, default_value = "bookmarks")]
    bookmark_dir: PathBuf,
    /// Directory for camera paths.
    #[arg(long, default_value = "paths")]
    path_dir: PathBuf,
}

#[derive(Subcommand)]
enum Command {
    /// Render a scene file to an image without opening a window.
    Render(RenderArgs),
    /// Measure GPU frame times (NF-1).
    Bench(bench::BenchArgs),
}

#[derive(Clone, Copy, PartialEq, Eq, ValueEnum)]
enum Backend {
    /// f64 reference renderer (slow, exact).
    Cpu,
    /// wgpu compute renderer (f32, fast).
    Gpu,
}

/// FR-R6 debug views (GPU backend).
#[derive(Clone, Copy, PartialEq, Eq, ValueEnum)]
enum Debug {
    /// Ray status: escaped blue, captured black, singular yellow, max-steps red, error magenta.
    Status,
    /// Frequency shift g: red (redshift) → white (g = 1) → blue (blueshift), log scale over ±2 stops.
    Shift,
    /// Integration steps, logarithmic heat map.
    Steps,
}

#[derive(clap::Args)]
struct RenderArgs {
    /// Scene file (TOML, see README § "Scene files").
    #[arg(long)]
    scene: PathBuf,
    /// Output image: `.png` (tone-mapped sRGB) or `.exr` (linear HDR).
    #[arg(long)]
    out: PathBuf,
    /// Override the scene's image width.
    #[arg(long)]
    width: Option<u32>,
    /// Override the scene's image height.
    #[arg(long)]
    height: Option<u32>,
    /// Override the scene's samples per pixel.
    #[arg(long)]
    samples: Option<u32>,
    /// Rendering backend.
    #[arg(long, value_enum, default_value_t = Backend::Cpu)]
    backend: Backend,
    /// Do not read or write the baked-sky cache.
    #[arg(long)]
    no_cache: bool,
    /// Write a debug view instead of the image (GPU backend; PNG output).
    #[arg(long, value_enum)]
    debug: Option<Debug>,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Some(Command::Render(args)) => render(&args),
        Some(Command::Bench(args)) => bench::bench(&args),
        None => view(&cli.view),
    }
}

/// Bakes (or loads from the cache) the sky of `scene`, printing where it came from.
fn load_sky(scene: &Scene, no_cache: bool) -> Result<Sky> {
    let t0 = Instant::now();
    let lut = BlackbodyLut::human_vision();
    if let Some(tex) = &scene.sky.texture {
        // Texture sky (PRD §6.5, approximate): each pixel becomes a blackbody fitted to its chromaticity.
        let img = gravography_render::output::load_equirect(Path::new(tex))?;
        let cube = gravography_sky::cube_from_equirect(&img, scene.sky.face_size, &lut);
        println!(
            "sky: texture {tex} ({}×{}) → {}² cube in {:.2} s (approximate colours: blackbody fit per pixel)",
            img.width,
            img.height,
            scene.sky.face_size,
            t0.elapsed().as_secs_f64()
        );
        return Ok(Sky::new(cube, scene.sky.band.lut()));
    }
    let cache = if no_cache { None } else { cache_dir() };
    let (cube, hit) = bake_sky_cached(&scene.sky_params()?, &lut, cache.as_deref())?;
    println!(
        "sky: {} ({:.2} s)",
        if hit { "loaded from cache" } else { "baked" },
        t0.elapsed().as_secs_f64()
    );
    // The star field is defined in human vision (flux → solid angle); the band only changes the shading table.
    Ok(Sky::new(cube, scene.sky.band.lut()))
}

fn view(args: &ViewArgs) -> Result<()> {
    let mut scene = match &args.scene {
        Some(p) => Scene::load(p).with_context(|| format!("loading {}", p.display()))?,
        None => Scene::default(),
    };
    let bookmark: Option<viewer::persist::Bookmark> = match &args.bookmark {
        Some(p) => Some(viewer::persist::load_ron(p)?),
        None => None,
    };
    if let Some(b) = &bookmark {
        // Reproduce the bookmark's star field.
        scene.sky = b.sky.clone();
    }
    // Validate the camera before opening a window.
    scene.frame()?;
    let sky = load_sky(&scene, args.no_cache)?;
    viewer::run(viewer::ViewerOptions {
        scene,
        sky,
        screenshot_dir: args.screenshot_dir.clone(),
        smoke_test: args.smoke_test,
        bookmark,
        // The smoke test keeps its files next to its screenshots, out of the user's folders.
        bookmark_dir: if args.smoke_test {
            args.screenshot_dir.join("bookmarks")
        } else {
            args.bookmark_dir.clone()
        },
        path_dir: if args.smoke_test {
            args.screenshot_dir.join("paths")
        } else {
            args.path_dir.clone()
        },
    })
}

/// Platform cache directory for baked skies: `~/Library/Caches/gravography` on macOS, otherwise
/// `$XDG_CACHE_HOME/gravography` or `~/.cache/gravography`.
fn cache_dir() -> Option<PathBuf> {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let base = if cfg!(target_os = "macos") {
        home.map(|h| h.join("Library/Caches"))
    } else {
        std::env::var_os("XDG_CACHE_HOME")
            .map(PathBuf::from)
            .or_else(|| home.map(|h| h.join(".cache")))
    };
    base.map(|b| b.join("gravography"))
}

fn render(args: &RenderArgs) -> Result<()> {
    let mut scene = Scene::load(&args.scene).with_context(|| format!("loading {}", args.scene.display()))?;
    if let Some(w) = args.width {
        scene.camera.width = w;
    }
    if let Some(h) = args.height {
        scene.camera.height = h;
    }
    if let Some(s) = args.samples {
        scene.camera.samples = s;
    }
    let frame = scene.frame()?;
    let settings = scene.cpu_settings()?;
    let format = OutputFormat::from_path(&args.out)?;
    if args.debug.is_some() && (args.backend != Backend::Gpu || format != OutputFormat::Png) {
        bail!("--debug needs --backend gpu and a .png output");
    }
    // Create the GPU device before the (possibly slow) sky bake so a missing adapter fails fast.
    let gpu = match args.backend {
        Backend::Gpu => {
            Some(GpuContext::new_headless()?.context("no GPU adapter available; use --backend cpu")?)
        }
        Backend::Cpu => None,
    };

    let sky = load_sky(&scene, args.no_cache)?;

    let t1 = Instant::now();
    let (stats, scale) = if let Some(ctx) = &gpu {
        println!("GPU: {}", ctx.describe());
        render_gpu(ctx, args, &frame, &sky, format)?
    } else {
        let (image, stats) = CpuRenderer::new(&frame, &sky, settings)?.render();
        let scale = display_scale(&frame, &image, &sky);
        write_image(&args.out, &image, scale, frame.tone_map)
            .with_context(|| format!("writing {}", args.out.display()))?;
        (stats, scale)
    };
    let secs = t1.elapsed().as_secs_f64();
    report(&frame, &stats, scale, secs, &args.out, gpu.is_some());
    Ok(())
}

/// GPU render (or debug view) to `args.out`; returns the last sample's statistics and the exposure scale.
fn render_gpu(
    ctx: &GpuContext,
    args: &RenderArgs,
    frame: &FrameParams,
    sky: &Sky,
    format: OutputFormat,
) -> Result<(RenderStats, f64)> {
    let renderer = GpuRenderer::new(ctx);
    let gpu_sky = GpuSky::upload(ctx, sky)?;
    let out = renderer.render(ctx, frame, &gpu_sky, &TraceParams::gpu_default())?;
    let stats = renderer.read_stats(ctx, &out)?;
    if let Some(d) = args.debug {
        let view = match d {
            Debug::Status => DebugView::Status,
            Debug::Shift => DebugView::Shift,
            Debug::Steps => DebugView::Steps,
        };
        let bytes = renderer.debug_view(ctx, &out, view)?;
        write_rgb8_png(&args.out, frame.width, frame.height, bytes)?;
        return Ok((stats, 1.0));
    }
    let image: HdrImage = renderer.read_hdr(ctx, &out)?;
    let scale = display_scale(frame, &image, sky);
    match format {
        OutputFormat::Png => {
            let bytes = renderer.tonemap(ctx, &out, scale, frame.tone_map)?;
            write_rgb8_png(&args.out, frame.width, frame.height, bytes)?;
        }
        OutputFormat::Exr => write_image(&args.out, &image, scale, frame.tone_map)?,
    }
    Ok((stats, scale))
}

fn report(frame: &FrameParams, stats: &RenderStats, scale: f64, secs: f64, out: &Path, gpu: bool) {
    let n = stats.samples().max(1) as f64;
    let pct = |s: RayStatus| 100.0 * stats.status[status_index(s)] as f64 / n;
    println!(
        "rendered {}×{} × {} spp in {secs:.2} s → {}",
        frame.width,
        frame.height,
        frame.samples,
        display(out)
    );
    println!(
        "rays{}: escaped {:.2}%, captured {:.2}%, max-steps {:.3}%, singular {:.3}%, error {:.3}%",
        if gpu { " (last sample)" } else { "" },
        pct(RayStatus::Escaped),
        pct(RayStatus::Captured),
        pct(RayStatus::MaxSteps),
        pct(RayStatus::Singular),
        pct(RayStatus::Error)
    );
    println!(
        "steps: mean {:.0} per ray, max {}; LUT clamped {}; footprint fallbacks {} partial / {} full; \
         exposure scale {scale:.4e}",
        stats.steps as f64 / stats.rays.max(1) as f64,
        stats.max_steps,
        stats.lut_clamped,
        stats.footprint_partial,
        stats.footprint_fallback
    );
}

fn display(p: &Path) -> String {
    p.display().to_string()
}
