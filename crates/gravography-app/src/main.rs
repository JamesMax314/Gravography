//! `gravography`: interactive black-hole viewer (WP-10) and headless `render` CLI (`docs/PRD.md` FR-X2).

use std::path::{Path, PathBuf};
use std::time::Instant;

use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand, ValueEnum};
use gravography_physics::RayStatus;
use gravography_render::cpu::status_index;
use gravography_render::{CpuRenderer, Scene, Sky, display_scale, write_image};
use gravography_sky::{BlackbodyLut, bake_sky_cached};

#[derive(Parser)]
#[command(name = "gravography", version, about = "Real-time Kerr black-hole ray tracer")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Render a scene file to an image without opening a window.
    Render(RenderArgs),
}

#[derive(Clone, Copy, PartialEq, Eq, ValueEnum)]
enum Backend {
    /// f64 reference renderer (slow, exact).
    Cpu,
    /// wgpu compute renderer (WP-09).
    Gpu,
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
}

fn main() -> Result<()> {
    match Cli::parse().command {
        Some(Command::Render(args)) => render(&args),
        None => {
            println!(
                "gravography {}: the interactive viewer arrives in WP-10. Use `gravography render --help`.",
                env!("CARGO_PKG_VERSION")
            );
            Ok(())
        }
    }
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
    if args.backend == Backend::Gpu {
        bail!("the GPU backend is not implemented yet (WP-09); use --backend cpu");
    }
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
    gravography_render::OutputFormat::from_path(&args.out)?;

    let t0 = Instant::now();
    let lut = BlackbodyLut::human_vision();
    let cache = if args.no_cache { None } else { cache_dir() };
    let (cube, hit) = bake_sky_cached(&scene.sky_params()?, &lut, cache.as_deref())?;
    let sky = Sky::new(cube, lut);
    println!(
        "sky: {} ({:.2} s)",
        if hit { "loaded from cache" } else { "baked" },
        t0.elapsed().as_secs_f64()
    );

    let t1 = Instant::now();
    let (image, stats) = CpuRenderer::new(&frame, &sky, settings)?.render();
    let secs = t1.elapsed().as_secs_f64();
    let scale = display_scale(&frame, &image, &sky);
    write_image(&args.out, &image, scale, frame.tone_map)
        .with_context(|| format!("writing {}", args.out.display()))?;

    let n = stats.samples().max(1) as f64;
    let pct = |s: RayStatus| 100.0 * stats.status[status_index(s)] as f64 / n;
    println!(
        "rendered {}×{} × {} spp in {secs:.2} s → {}",
        frame.width,
        frame.height,
        frame.samples,
        display(&args.out)
    );
    println!(
        "rays: escaped {:.2}%, captured {:.2}%, max-steps {:.3}%, singular {:.3}%, error {:.3}%",
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
    Ok(())
}

fn display(p: &Path) -> String {
    p.display().to_string()
}
