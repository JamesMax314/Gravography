//! Interactive viewer (`docs/PRD.md` WP-10): winit window, wgpu surface, progressive GPU rendering of a fixed
//! camera, egui settings panel, resize handling and FR-X1 screenshots.
//!
//! Each redraw adds one sample to the accumulation buffer until the target is reached; the present pass
//! tone-maps the accumulation buffer onto the window and egui draws on top. Exposure and the tone map are applied
//! when presenting, so changing them does not restart accumulation.

pub mod screenshot;
pub mod state;
pub mod ui;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, anyhow};
use gravography_physics::TraceParams;
use gravography_render::gpu::{GpuContext, GpuFrame, GpuRenderer, GpuSky, Presenter};
use gravography_render::output::write_rgb8_png;
use gravography_render::tonemap::log_average_luminance;
use gravography_render::{FrameParams, Scene, Sky, write_image};
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalSize, PhysicalSize};
use winit::event::{ElementState, KeyEvent, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{Key, NamedKey};
use winit::window::{Window, WindowId};

use self::screenshot::screenshot_paths;
use self::state::{Accumulation, ViewSettings, internal_size};
use self::ui::{Status, settings_panel};

/// Viewer start-up options.
pub struct ViewerOptions {
    /// Scene providing the camera, sky and initial display settings.
    pub scene: Scene,
    /// Baked sky for the scene.
    pub sky: Sky,
    /// Where screenshots are written.
    pub screenshot_dir: PathBuf,
    /// Scripted check: converge, screenshot, resize, converge, screenshot, exit.
    pub smoke_test: bool,
}

/// Opens the viewer window and runs until it is closed (or the smoke test finishes).
///
/// # Errors
/// Event-loop, GPU or screenshot failures; a failed smoke test.
pub fn run(opts: ViewerOptions) -> Result<()> {
    let event_loop = EventLoop::new().context("creating the event loop")?;
    event_loop.set_control_flow(ControlFlow::Wait);
    let mut app = App {
        opts,
        gfx: None,
        error: None,
        smoke: SmokeStage::Converge1,
        started: Instant::now(),
    };
    event_loop.run_app(&mut app).context("running the event loop")?;
    match app.error {
        Some(e) => Err(e),
        None => Ok(()),
    }
}

/// Smoke-test progress.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SmokeStage {
    /// Wait for convergence at the initial size, then screenshot and request a resize.
    Converge1,
    /// Wait until the surface size differs from `from`.
    Resizing { from: (u32, u32) },
    /// Wait for convergence at the new size, then screenshot and exit.
    Converge2,
    /// Finished.
    Done,
}

/// Longest the smoke test may run.
const SMOKE_TIMEOUT: Duration = Duration::from_secs(120);

struct App {
    opts: ViewerOptions,
    gfx: Option<Gfx>,
    error: Option<anyhow::Error>,
    smoke: SmokeStage,
    started: Instant,
}

/// Everything that exists once the window does.
struct Gfx {
    window: Arc<Window>,
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    ctx: GpuContext,
    renderer: GpuRenderer,
    presenter: Presenter,
    sky: GpuSky,
    reference_luminance: f64,
    egui_ctx: egui::Context,
    egui_state: egui_winit::State,
    egui_renderer: egui_wgpu::Renderer,
    target: GpuFrame,
    accum: Accumulation,
    settings: ViewSettings,
    /// Log-average luminance of the last metered image (auto exposure).
    log_average: Option<f64>,
    status: Status,
    last_frame: Instant,
    screenshot_counter: u32,
    screenshot_requested: bool,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.gfx.is_some() {
            return;
        }
        match Gfx::new(event_loop, &self.opts) {
            Ok(g) => {
                g.window.request_redraw();
                self.gfx = Some(g);
            }
            Err(e) => self.fail(event_loop, e),
        }
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        let Some(gfx) = self.gfx.as_mut() else {
            return;
        };
        let response = gfx.egui_state.on_window_event(&gfx.window, &event);
        if response.repaint {
            gfx.window.request_redraw();
        }
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                gfx.resize(size);
                gfx.window.request_redraw();
            }
            WindowEvent::KeyboardInput {
                event:
                    KeyEvent {
                        logical_key,
                        state: ElementState::Pressed,
                        repeat: false,
                        ..
                    },
                ..
            } if !response.consumed => match logical_key {
                Key::Character(c) if c.eq_ignore_ascii_case("p") => {
                    gfx.screenshot_requested = true;
                    gfx.window.request_redraw();
                }
                Key::Named(NamedKey::Escape) => event_loop.exit(),
                _ => {}
            },
            WindowEvent::RedrawRequested => {
                if let Err(e) = gfx.redraw(&self.opts) {
                    self.fail(event_loop, e);
                    return;
                }
                if self.opts.smoke_test {
                    self.smoke_step(event_loop);
                }
            }
            _ => {}
        }
    }
}

impl App {
    fn fail(&mut self, event_loop: &ActiveEventLoop, e: anyhow::Error) {
        eprintln!("error: {e:#}");
        self.error = Some(e);
        event_loop.exit();
    }

    /// Advances the scripted smoke test after a redraw.
    fn smoke_step(&mut self, event_loop: &ActiveEventLoop) {
        if self.started.elapsed() > SMOKE_TIMEOUT {
            self.fail(
                event_loop,
                anyhow!("smoke test timed out in stage {:?}", self.smoke),
            );
            return;
        }
        let Some(gfx) = self.gfx.as_mut() else {
            return;
        };
        let size = (gfx.config.width, gfx.config.height);
        let result = match self.smoke {
            SmokeStage::Converge1 if gfx.accum.converged() => gfx.smoke_screenshot(&self.opts).map(|_| {
                // A clearly different size; the window manager may adjust it, any change counts.
                let logical = gfx
                    .window
                    .inner_size()
                    .to_logical::<f64>(gfx.window.scale_factor());
                let _ = gfx
                    .window
                    .request_inner_size(LogicalSize::new(logical.width * 0.75, logical.height * 0.8));
                self.smoke = SmokeStage::Resizing { from: size };
            }),
            SmokeStage::Resizing { from } => {
                let now = gfx.window.inner_size();
                if (now.width, now.height) != from {
                    gfx.resize(now);
                    self.smoke = SmokeStage::Converge2;
                }
                Ok(())
            }
            SmokeStage::Converge2 if gfx.accum.converged() => gfx.smoke_screenshot(&self.opts).map(|_| {
                println!("smoke test passed: rendered, resized and wrote screenshots at two sizes");
                self.smoke = SmokeStage::Done;
                event_loop.exit();
            }),
            _ => Ok(()),
        };
        if let Err(e) = result {
            self.fail(event_loop, e);
            return;
        }
        if let Some(gfx) = self.gfx.as_ref() {
            gfx.window.request_redraw();
        }
    }
}

impl Gfx {
    fn new(event_loop: &ActiveEventLoop, opts: &ViewerOptions) -> Result<Self> {
        let attrs = Window::default_attributes()
            .with_title("Gravography")
            .with_inner_size(LogicalSize::new(1280.0, 720.0));
        let window = Arc::new(event_loop.create_window(attrs).context("creating the window")?);
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
        let surface = instance
            .create_surface(window.clone())
            .context("creating the window surface")?;
        let ctx = GpuContext::for_surface(&instance, &surface)?;
        let size = window.inner_size();
        let caps = surface.get_capabilities(&ctx.adapter);
        let format = egui_wgpu::preferred_framebuffer_format(&caps.formats)
            .ok()
            .or_else(|| caps.formats.first().copied())
            .ok_or_else(|| anyhow!("the surface supports no formats"))?;
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            color_space: wgpu::SurfaceColorSpace::Auto,
            width: size.width.max(1),
            height: size.height.max(1),
            desired_maximum_frame_latency: 2,
            present_mode: wgpu::PresentMode::AutoVsync,
            alpha_mode: caps
                .alpha_modes
                .first()
                .copied()
                .unwrap_or(wgpu::CompositeAlphaMode::Auto),
            view_formats: vec![],
        };
        surface.configure(&ctx.device, &config);

        let renderer = GpuRenderer::new(&ctx);
        let presenter = Presenter::new(&ctx, format);
        let sky = GpuSky::upload(&ctx, &opts.sky)?;
        let egui_ctx = egui::Context::default();
        let egui_state = egui_winit::State::new(
            egui_ctx.clone(),
            egui::ViewportId::ROOT,
            &window,
            Some(window.scale_factor() as f32),
            None,
            Some(ctx.device.limits().max_texture_dimension_2d as usize),
        );
        let egui_renderer =
            egui_wgpu::Renderer::new(&ctx.device, format, egui_wgpu::RendererOptions::default());

        let mut settings = ViewSettings::from_scene(&opts.scene);
        if opts.smoke_test {
            // Few samples so the scripted check converges quickly.
            settings.samples = 4;
        }
        let (w, h) = internal_size((config.width, config.height), settings.render_scale);
        let target = GpuFrame::new(&ctx, w, h, TraceParams::<f32>::gpu_default().max_steps);
        let status = Status {
            gpu: ctx.describe(),
            ..Status::default()
        };
        Ok(Self {
            window,
            surface,
            config,
            renderer,
            presenter,
            sky,
            reference_luminance: opts.sky.reference_luminance(),
            egui_ctx,
            egui_state,
            egui_renderer,
            target,
            accum: Accumulation::new(settings.samples),
            settings,
            log_average: None,
            status,
            last_frame: Instant::now(),
            screenshot_counter: 0,
            screenshot_requested: false,
            ctx,
        })
    }

    /// Reconfigures the surface and restarts accumulation at the new internal size.
    fn resize(&mut self, size: PhysicalSize<u32>) {
        if size.width == 0 || size.height == 0 {
            return;
        }
        if (size.width, size.height) == (self.config.width, self.config.height) {
            return;
        }
        self.config.width = size.width;
        self.config.height = size.height;
        self.surface.configure(&self.ctx.device, &self.config);
        self.restart();
    }

    /// Restarts accumulation (reallocating the target if the internal size changed).
    fn restart(&mut self) {
        let (w, h) = internal_size(
            (self.config.width, self.config.height),
            self.settings.render_scale,
        );
        if (w, h) != (self.target.width, self.target.height) {
            self.target = GpuFrame::new(&self.ctx, w, h, self.target.max_steps);
        }
        self.accum = Accumulation::new(self.settings.samples);
        self.log_average = None;
    }

    /// Frame parameters for a `width × height` render of the scene with the current settings.
    fn frame_params(&self, opts: &ViewerOptions, width: u32, height: u32) -> Result<FrameParams> {
        let mut scene = opts.scene.clone();
        scene.camera.width = width;
        scene.camera.height = height;
        scene.camera.samples = self.settings.samples;
        scene.render.exposure = self.settings.exposure;
        scene.render.tone_map = self.settings.tone_map;
        Ok(scene.frame()?)
    }

    fn exposure_scale(&self) -> f64 {
        self.settings
            .exposure
            .scale(self.log_average, self.reference_luminance)
    }

    fn redraw(&mut self, opts: &ViewerOptions) -> Result<()> {
        // UI first: settings changes apply to this frame.
        let raw = self.egui_state.take_egui_input(&self.window);
        let mut settings = self.settings;
        let mut actions = ui::Actions::default();
        let status = self.status.clone();
        let out = self.egui_ctx.run_ui(raw, |ui| {
            egui::Panel::left("settings").show(ui, |ui| {
                actions = settings_panel(ui, &mut settings, &status);
            });
        });
        self.egui_state
            .handle_platform_output(&self.window, out.platform_output);
        let reset = self.settings.resets_accumulation(&settings);
        self.settings = settings;
        if reset {
            self.restart();
        }
        if actions.screenshot || std::mem::take(&mut self.screenshot_requested) {
            match self.screenshot(opts) {
                Ok((png, exr)) => {
                    self.status.message = format!("saved {} and {}", png.display(), exr.display())
                }
                Err(e) => self.status.message = format!("screenshot failed: {e:#}"),
            }
        }

        // One progressive sample per frame until converged.
        if let Some(s) = self.accum.next_sample() {
            let frame = self.frame_params(opts, self.target.width, self.target.height)?;
            self.renderer.render_sample(
                &self.ctx,
                &frame,
                &self.sky,
                &TraceParams::gpu_default(),
                &self.target,
                s,
            )?;
            self.accum.advance();
            if self.settings.exposure.auto && self.accum.meter_exposure_now() {
                let hdr = self.renderer.read_hdr(&self.ctx, &self.target)?;
                self.log_average = log_average_luminance(&hdr.pixels);
            }
        }

        let surface_texture = match self.surface.get_current_texture() {
            wgpu::CurrentSurfaceTexture::Success(t) | wgpu::CurrentSurfaceTexture::Suboptimal(t) => t,
            wgpu::CurrentSurfaceTexture::Outdated | wgpu::CurrentSurfaceTexture::Lost => {
                self.surface.configure(&self.ctx.device, &self.config);
                self.window.request_redraw();
                return Ok(());
            }
            _ => {
                // Timeout, occluded or validation: skip this frame.
                self.window.request_redraw();
                return Ok(());
            }
        };
        let view = surface_texture
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let paint_jobs = self.egui_ctx.tessellate(out.shapes, out.pixels_per_point);
        for (id, deltas) in &out.textures_delta.set {
            for delta in deltas {
                self.egui_renderer
                    .update_texture(&self.ctx.device, &self.ctx.queue, *id, delta);
            }
        }
        let screen = egui_wgpu::ScreenDescriptor {
            size_in_pixels: [self.config.width, self.config.height],
            pixels_per_point: out.pixels_per_point,
        };
        let mut encoder = self
            .ctx
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("frame") });
        let egui_cmds = self.egui_renderer.update_buffers(
            &self.ctx.device,
            &self.ctx.queue,
            &mut encoder,
            &paint_jobs,
            &screen,
        );
        {
            let mut pass = encoder
                .begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("present"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                })
                .forget_lifetime();
            let dst = [0.0, 0.0, self.config.width as f32, self.config.height as f32];
            self.presenter.draw(
                &self.ctx,
                &mut pass,
                &self.target,
                self.exposure_scale(),
                self.settings.tone_map,
                dst,
            );
            self.egui_renderer.render(&mut pass, &paint_jobs, &screen);
        }
        self.ctx
            .queue
            .submit(egui_cmds.into_iter().chain([encoder.finish()]));
        self.window.pre_present_notify();
        self.ctx.queue.present(surface_texture);
        for id in &out.textures_delta.free {
            self.egui_renderer.free_texture(id);
        }

        // Status for the next frame.
        let now = Instant::now();
        let dt = now.duration_since(self.last_frame).as_secs_f64().max(1e-6);
        self.last_frame = now;
        self.status.fps = if self.status.fps > 0.0 {
            0.9 * self.status.fps + 0.1 / dt
        } else {
            1.0 / dt
        };
        self.status.render_size = (self.target.width, self.target.height);
        self.status.samples = (self.accum.done, self.accum.target);
        self.status.exposure_scale = self.exposure_scale();

        // Keep rendering while refining; otherwise wait for input.
        let egui_wants_repaint = out
            .viewport_output
            .get(&egui::ViewportId::ROOT)
            .is_some_and(|v| v.repaint_delay.is_zero());
        if !self.accum.converged() || egui_wants_repaint {
            self.window.request_redraw();
        }
        Ok(())
    }

    /// FR-X1: renders the current view to convergence at the full window resolution and writes a tone-mapped PNG
    /// and a linear HDR EXR. The exposure scale is the one on screen (what you see is what you get).
    fn screenshot(&mut self, opts: &ViewerOptions) -> Result<(PathBuf, PathBuf)> {
        let (w, h) = (self.config.width, self.config.height);
        let frame = self.frame_params(opts, w, h)?;
        let out = self
            .renderer
            .render(&self.ctx, &frame, &self.sky, &TraceParams::gpu_default())?;
        let scale = self.exposure_scale();
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        self.screenshot_counter += 1;
        let (png, exr) = screenshot_paths(&opts.screenshot_dir, stamp, self.screenshot_counter);
        let bytes = self
            .renderer
            .tonemap(&self.ctx, &out, scale, self.settings.tone_map)?;
        write_rgb8_png(&png, w, h, bytes)?;
        let hdr = self.renderer.read_hdr(&self.ctx, &out)?;
        write_image(&exr, &hdr, scale, self.settings.tone_map)?;
        println!("screenshot: {} and {} ({w}×{h})", png.display(), exr.display());
        Ok((png, exr))
    }

    /// Screenshot for the smoke test, checking that both files exist and the PNG has the window's size.
    fn smoke_screenshot(&mut self, opts: &ViewerOptions) -> Result<()> {
        let (png, exr) = self.screenshot(opts)?;
        let dims = image_dimensions(&png)?;
        anyhow::ensure!(
            dims == (self.config.width, self.config.height),
            "screenshot is {dims:?}, window is {}×{}",
            self.config.width,
            self.config.height
        );
        anyhow::ensure!(exr.is_file(), "EXR {} missing", exr.display());
        Ok(())
    }
}

/// Width and height of a PNG from its IHDR chunk (bytes 16..24, big-endian).
fn image_dimensions(path: &std::path::Path) -> Result<(u32, u32)> {
    let bytes = std::fs::read(path).with_context(|| format!("reading {}", path.display()))?;
    anyhow::ensure!(
        bytes.len() >= 24 && bytes[..8] == *b"\x89PNG\r\n\x1a\n",
        "not a PNG"
    );
    let be = |i: usize| u32::from_be_bytes([bytes[i], bytes[i + 1], bytes[i + 2], bytes[i + 3]]);
    Ok((be(16), be(20)))
}
