//! Interactive viewer (`docs/PRD.md` WP-10, WP-11): winit window, wgpu surface, progressive GPU rendering, egui
//! panel and HUD, flight controls, resize handling and FR-X1 screenshots.
//!
//! Each redraw first applies panel changes and advances the observer ([`flight::Flight::update`]). If the camera
//! moved, accumulation restarts; otherwise one more sample is added until the target is reached. The present pass
//! tone-maps the accumulation buffer onto the window and egui draws on top. Exposure and the tone map are applied
//! when presenting, so changing them does not restart accumulation.

pub mod capture;
pub mod flight;
pub mod input;
pub mod screenshot;
pub mod state;
pub mod ui;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, anyhow};
use gravography_physics::TraceParams;
use gravography_render::camera::Projection;
use gravography_render::gpu::{GpuContext, GpuFrame, GpuRenderer, GpuSky, Presenter};
use gravography_render::output::write_rgb8_png;
use gravography_render::scene::{ObserverSpec, PresetKind};
use gravography_render::tonemap::log_average_luminance;
use gravography_render::{FrameParams, Scene, Sky, write_image};
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalSize, PhysicalSize};
use winit::event::{DeviceEvent, DeviceId, ElementState, KeyEvent, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::PhysicalKey;
use winit::window::{CursorGrabMode, Window, WindowId};

use self::flight::{Camera, Flight, Hud, check_preset};
use self::input::{KeyAction, LookButton, MouseLook, ThrustKeys, degrees_per_pixel, key_action};
use self::screenshot::screenshot_paths;
use self::state::{Accumulation, ViewSettings, internal_size};
use self::ui::{FlightPanel, Status, hud_lines, hud_window, settings_panel};

/// Viewer start-up options.
pub struct ViewerOptions {
    /// Scene providing the camera, sky and initial display settings.
    pub scene: Scene,
    /// Baked sky for the scene.
    pub sky: Sky,
    /// Where screenshots are written.
    pub screenshot_dir: PathBuf,
    /// Scripted check: converge, screenshot, resize, converge, screenshot, fall through the horizon, converge,
    /// screenshot, exit.
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
    /// Wait for convergence at the new size, then screenshot, change the spin and start a radial infall.
    Converge2,
    /// Falling until inside the horizon, then pause.
    Falling,
    /// Wait for convergence inside the horizon, then screenshot.
    Converge3,
    /// One more redraw for the last window capture, then exit.
    Finish,
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
    /// The observer and its controls.
    flight: Flight,
    /// Preset being edited in the panel.
    editor: ObserverSpec,
    /// Held thrust keys.
    keys: ThrustKeys,
    /// Mouse look (drag or captured cursor).
    mouse: MouseLook,
    /// Camera of the accumulated image.
    rendered: Option<Camera>,
    /// When exposure and ray statistics were last read back.
    last_meter: Option<Instant>,
    /// Wall-clock time of the last flight update.
    last_update: Instant,
    /// Capture the next presented window frame (panel and HUD included) to this PNG.
    capture_to: Option<PathBuf>,
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
                        physical_key: PhysicalKey::Code(code),
                        state,
                        repeat,
                        ..
                    },
                ..
            } => {
                if let Some(action) = key_action(code) {
                    let pressed = state == ElementState::Pressed;
                    if gfx.key(action, pressed, repeat, response.consumed) {
                        event_loop.exit();
                    }
                    gfx.window.request_redraw();
                }
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let button = match button {
                    MouseButton::Left => Some(LookButton::Left),
                    MouseButton::Right => Some(LookButton::Right),
                    _ => None,
                };
                if let Some(b) = button {
                    gfx.mouse_button(b, state == ElementState::Pressed, response.consumed);
                }
            }
            WindowEvent::CursorMoved { position, .. } => {
                gfx.cursor_moved((position.x, position.y), &self.opts);
            }
            WindowEvent::Focused(false) => {
                // Key releases are lost while unfocused: drop held keys so thrust does not stick.
                gfx.keys.clear();
                if gfx.mouse.release() {
                    gfx.set_capture(false);
                }
            }
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

    fn device_event(&mut self, _event_loop: &ActiveEventLoop, _id: DeviceId, event: DeviceEvent) {
        // Raw motion drives the captured mouse look (the cursor itself is locked and hidden).
        if let (Some(gfx), DeviceEvent::MouseMotion { delta }) = (self.gfx.as_mut(), event) {
            let deg = gfx.degrees_per_pixel(&self.opts);
            if let Some((dy, dp)) = gfx.mouse.raw_motion(delta, deg) {
                gfx.flight.look_by(dy, dp);
                gfx.window.request_redraw();
            }
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
            SmokeStage::Converge2 if gfx.accum.converged() => {
                gfx.smoke_screenshot(&self.opts).and_then(|_| {
                    // Drag the view through the viewer's own mouse handlers (OS events cannot be injected here): the
                    // look must turn left and up ("grab the sky": drag right and down).
                    let (cx, cy) = (
                        f64::from(gfx.config.width) * 0.6,
                        f64::from(gfx.config.height) * 0.5,
                    );
                    gfx.cursor_moved((cx, cy), &self.opts);
                    gfx.mouse_button(LookButton::Left, true, false);
                    anyhow::ensure!(
                        gfx.mouse.dragging(),
                        "a left press over the 3D view did not start a drag"
                    );
                    gfx.cursor_moved((cx + 200.0, cy + 100.0), &self.opts);
                    gfx.mouse_button(LookButton::Left, false, false);
                    let look = gfx.flight.look;
                    anyhow::ensure!(
                        look.yaw_deg > 1.0 && look.pitch_deg > 0.5,
                        "drag did not turn the view: {look:?}"
                    );
                    println!(
                        "smoke test: drag turned the view by yaw {:.2}°, pitch {:.2}°",
                        look.yaw_deg, look.pitch_deg
                    );
                    gfx.flight.reset_look();
                    // Spin up while hovering (keeps the state), then fall in from r = 8 at 4 M/s.
                    if let Some(m) = gfx.flight.set_spin(0.5) {
                        anyhow::bail!("spin change while hovering at r = 30 reset the observer: {m}");
                    }
                    let infall = ObserverSpec {
                        preset: PresetKind::RadialInfall,
                        r: 8.0,
                        ..flight::safe_preset()
                    };
                    gfx.flight.apply_preset(&infall).map_err(|e| anyhow!(e))?;
                    gfx.flight.settings.time_warp = 4.0;
                    gfx.flight.settings.paused = false;
                    self.smoke = SmokeStage::Falling;
                    Ok(())
                })
            }
            SmokeStage::Falling => {
                let hud = Hud::new(&gfx.flight, &gfx.keys);
                if gfx.flight.settings.paused {
                    Err(anyhow!("the infall was reset: {}", gfx.status.message))
                } else if hud.r >= 1.5 {
                    Ok(())
                } else if hud.region != gravography_physics::Region::InsideHorizon {
                    Err(anyhow!("r = {} but region {:?}", hud.r, hud.region))
                } else {
                    gfx.flight.settings.paused = true;
                    println!(
                        "smoke test: inside the horizon at r = {:.3} M, τ = {:.3} M",
                        hud.r, hud.tau
                    );
                    self.smoke = SmokeStage::Converge3;
                    Ok(())
                }
            }
            SmokeStage::Converge3 if gfx.accum.converged() => gfx.smoke_screenshot(&self.opts).map(|_| {
                self.smoke = SmokeStage::Finish;
            }),
            SmokeStage::Finish if gfx.capture_to.is_none() => {
                println!(
                    "smoke test passed: rendered, resized, flew through the horizon and wrote screenshots at each stage"
                );
                self.smoke = SmokeStage::Done;
                event_loop.exit();
                Ok(())
            }
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
        // COPY_SRC (where supported) lets the smoke test capture the composed window frame.
        let copy_src = caps.usages & wgpu::TextureUsages::COPY_SRC;
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | copy_src,
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

        let flight = Flight::new(opts.scene.kerr()?, &opts.scene.observer).map_err(|e| anyhow!(e))?;
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
            editor: *flight.preset(),
            flight,
            keys: ThrustKeys::default(),
            mouse: MouseLook::default(),
            rendered: None,
            last_meter: None,
            last_update: Instant::now(),
            capture_to: None,
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
        self.restart(true);
    }

    /// Handles a mapped key. Thrust keys are tracked while held (releases always count, so keys cannot stick
    /// when egui takes focus); other actions fire once per press when egui does not want the keyboard. Returns
    /// `true` to quit.
    fn key(&mut self, action: KeyAction, pressed: bool, repeat: bool, consumed: bool) -> bool {
        if let KeyAction::Thrust(k) = action {
            if !pressed || !consumed {
                self.keys.set(k, pressed);
            }
            return false;
        }
        if !pressed || repeat || consumed {
            return false;
        }
        let s = &mut self.flight.settings;
        match action {
            KeyAction::Thrust(_) => {}
            KeyAction::CutThrust => {
                self.keys.clear();
                if let Some(m) = self.flight.cut_thrust() {
                    self.status.message = m;
                }
            }
            KeyAction::TogglePause => s.paused = !s.paused,
            KeyAction::Step => self.flight.request_step(),
            KeyAction::WarpSlower => s.time_warp *= 0.5,
            KeyAction::WarpFaster => s.time_warp *= 2.0,
            KeyAction::ThrustWeaker => s.thrust *= 0.5,
            KeyAction::ThrustStronger => s.thrust *= 2.0,
            KeyAction::Screenshot => self.screenshot_requested = true,
            // Esc first releases a captured cursor; otherwise it quits.
            KeyAction::Quit => {
                if !self.mouse.release() {
                    return true;
                }
                self.set_capture(false);
            }
        }
        self.flight.settings = self.flight.settings.clamped();
        false
    }

    /// Mouse-look sensitivity for the current window (degrees per physical pixel at the image centre).
    fn degrees_per_pixel(&self, opts: &ViewerOptions) -> f64 {
        let Projection::Rectilinear { fov_deg } = opts.scene.camera.projection;
        degrees_per_pixel(fov_deg, self.config.height)
    }

    /// A mouse button press or release (`consumed`: egui wants it).
    fn mouse_button(&mut self, button: LookButton, pressed: bool, consumed: bool) {
        if let Some(captured) = self.mouse.button(button, pressed, consumed) {
            self.set_capture(captured);
        }
    }

    /// Cursor moved: turns the view while dragging.
    fn cursor_moved(&mut self, pos: (f64, f64), opts: &ViewerOptions) {
        let deg = self.degrees_per_pixel(opts);
        if let Some((dy, dp)) = self.mouse.cursor_moved(pos, deg) {
            self.flight.look_by(dy, dp);
            self.window.request_redraw();
        }
    }

    /// Locks and hides the cursor for captured mouse look, or releases it.
    fn set_capture(&mut self, on: bool) {
        let grab = if on {
            self.window
                .set_cursor_grab(CursorGrabMode::Locked)
                .or_else(|_| self.window.set_cursor_grab(CursorGrabMode::Confined))
        } else {
            self.window.set_cursor_grab(CursorGrabMode::None)
        };
        self.window.set_cursor_visible(!on);
        self.status.message = match (on, grab) {
            (true, Ok(())) => {
                "mouse look captured: move the mouse to look; right-click or Esc releases".into()
            }
            (true, Err(e)) => format!(
                "mouse look on, but the cursor could not be locked ({e}); right-click or Esc releases"
            ),
            (false, _) => "mouse look released".into(),
        };
        self.window.request_redraw();
    }

    /// Restarts accumulation (reallocating the target if the internal size changed). `hard` (settings, size)
    /// re-meters exposure on the next sample; a camera move keeps the throttled metering of a moving view.
    fn restart(&mut self, hard: bool) {
        let (w, h) = internal_size(
            (self.config.width, self.config.height),
            self.settings.render_scale,
        );
        if (w, h) != (self.target.width, self.target.height) {
            self.target = GpuFrame::new(&self.ctx, w, h, self.target.max_steps);
        }
        self.accum = Accumulation::new(self.settings.samples);
        self.rendered = Some(self.flight.camera());
        if hard {
            self.last_meter = None;
        }
    }

    /// Frame parameters for a `width × height` render of the current camera with the current settings.
    fn frame_params(&self, opts: &ViewerOptions, width: u32, height: u32) -> Result<FrameParams> {
        let cam = self.flight.camera();
        let frame = FrameParams {
            kerr: cam.kerr,
            position: cam.position,
            tetrad: cam.tetrad,
            width,
            height,
            projection: opts.scene.camera.projection,
            samples: self.settings.samples,
            effects: self.settings.effects,
            exposure: self.settings.exposure,
            tone_map: self.settings.tone_map,
        };
        frame.validate()?;
        Ok(frame)
    }

    /// Applies the panel's flight changes and actions.
    fn apply_panel(&mut self, panel: FlightPanel, actions: ui::Actions) {
        self.flight.settings = panel.settings.clamped();
        self.editor = panel.editor;
        // No-op unless the slider moved.
        if let Some(m) = self.flight.set_spin(panel.spin) {
            self.status.message = m;
        }
        if actions.apply_preset {
            self.status.message = match self.flight.apply_preset(&self.editor) {
                Ok(Some(w)) => format!("preset applied; {w}"),
                Ok(None) => "preset applied".into(),
                Err(e) => e,
            };
        }
        if actions.release {
            self.keys.clear();
            if let Some(m) = self.flight.cut_thrust() {
                self.status.message = m;
            }
        }
        if actions.step {
            self.flight.request_step();
        }
        if actions.reset_look {
            self.flight.reset_look();
        }
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
        self.status.mouse_captured = self.mouse.captured();
        let status = self.status.clone();
        let mut panel = FlightPanel {
            settings: self.flight.settings,
            spin: self.flight.kerr.spin(),
            editor: self.editor,
        };
        let check = check_preset(&self.flight.kerr, &self.editor, self.flight.params());
        let hud = hud_lines(
            &Hud::new(&self.flight, &self.keys),
            &status,
            &self.flight.settings,
            self.settings.render_scale,
        );
        let out = self.egui_ctx.run_ui(raw, |ui| {
            egui::Panel::left("settings").show(ui, |ui| {
                actions = settings_panel(ui, &mut settings, &mut panel, &check, &status);
            });
            hud_window(ui.ctx(), &hud);
        });
        self.egui_state
            .handle_platform_output(&self.window, out.platform_output);
        let reset = self.settings.resets_accumulation(&settings);
        self.settings = settings;
        if reset {
            self.restart(true);
        }
        self.apply_panel(panel, actions);

        // Advance the observer; restart accumulation if the camera moved.
        let now = Instant::now();
        let dt = now.duration_since(self.last_update).as_secs_f64();
        self.last_update = now;
        let update = self.flight.update(dt, &self.keys);
        if let Some(m) = update.message {
            self.status.message = m;
        }
        let cam = self.flight.camera();
        if self.rendered.is_none_or(|r| cam.differs(&r)) {
            self.restart(false);
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
            if self
                .accum
                .meter_exposure_now(self.last_meter.map(|t| t.elapsed()))
            {
                if self.settings.exposure.auto {
                    let hdr = self.renderer.read_hdr(&self.ctx, &self.target)?;
                    if let Some(l) = log_average_luminance(&hdr.pixels) {
                        self.log_average = Some(l);
                    }
                }
                let stats = self.renderer.read_stats(&self.ctx, &self.target)?;
                let n = stats.status.iter().sum::<u64>().max(1) as f64;
                self.status.ray_rates = Some(stats.status.map(|c| c as f64 / n));
                self.last_meter = Some(Instant::now());
            }
        }

        // Upload egui texture changes before anything can skip the frame: egui sends each delta (including the
        // font atlas on the first frame) only once, and a lost atlas makes every egui shape invisible.
        for (id, deltas) in &out.textures_delta.set {
            for delta in deltas {
                self.egui_renderer
                    .update_texture(&self.ctx.device, &self.ctx.queue, *id, delta);
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
        if let Some(path) = self.capture_to.take() {
            capture::capture_texture(&self.ctx, &surface_texture.texture, &path)?;
            println!("window capture: {}", path.display());
        }
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

        // Keep rendering while refining or while the simulation runs; otherwise wait for input.
        let egui_wants_repaint = out
            .viewport_output
            .get(&egui::ViewportId::ROOT)
            .is_some_and(|v| v.repaint_delay.is_zero());
        if !self.accum.converged() || egui_wants_repaint || !self.flight.settings.paused {
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
        if self.config.usage.contains(wgpu::TextureUsages::COPY_SRC) {
            let name = png
                .file_name()
                .map(|n| format!("window-{}", n.to_string_lossy()))
                .unwrap_or_else(|| "window.png".into());
            self.capture_to = Some(png.with_file_name(name));
        }
        let hud = Hud::new(&self.flight, &self.keys);
        for line in hud_lines(
            &hud,
            &self.status,
            &self.flight.settings,
            self.settings.render_scale,
        ) {
            println!("  hud: {line}");
        }
        println!("  exposure scale {:.4e}", self.exposure_scale());
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
