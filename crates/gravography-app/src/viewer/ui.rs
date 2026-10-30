//! The egui panels: the left settings panel (flight, presets, spacetime, effects, render, exposure) and the HUD
//! (`docs/PRD.md` FR-O2–FR-O5, §6.4). The panel edits plain copies of the state; the viewer applies the changes.

use gravography_physics::{Motion, Region};
use gravography_render::ToneMap;
use gravography_render::scene::{MAX_SPIN, ObserverSpec, PresetKind};

use super::flight::{FlightSettings, Hud, PresetCheck, THRUST_RANGE, WARP_RANGE, preset_name};
use super::state::{FOV_RANGE, MAX_SAMPLE_TARGET, RENDER_SCALE_RANGE, ViewSettings};
use gravography_render::Projection;

/// Read-only status shown in the panel and HUD.
#[derive(Clone, Debug, Default)]
pub struct Status {
    /// GPU description.
    pub gpu: String,
    /// Frames per second (smoothed).
    pub fps: f64,
    /// Internal render size.
    pub render_size: (u32, u32),
    /// Samples accumulated / target.
    pub samples: (u32, u32),
    /// Exposure scale in use.
    pub exposure_scale: f64,
    /// Fractions of rays per status (escaped, captured, singular, max steps, error) from the last statistics
    /// readback.
    pub ray_rates: Option<[f64; 5]>,
    /// Last message (screenshot saved, mode switches, resets, errors).
    pub message: String,
    /// Mouse-look state: the cursor is captured.
    pub mouse_captured: bool,
    /// GPU time of the last timed sample, milliseconds.
    pub gpu_ms: Option<f64>,
}

/// Editable copies of the flight controls.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FlightPanel {
    /// Time warp, pause, thrust, mass.
    pub settings: FlightSettings,
    /// Spin slider value.
    pub spin: f64,
    /// Preset being edited (applied with the button).
    pub editor: ObserverSpec,
}

/// Actions requested from the panel.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Actions {
    /// Take a screenshot.
    pub screenshot: bool,
    /// Apply the edited preset.
    pub apply_preset: bool,
    /// Cut thrust / release station keeping.
    pub release: bool,
    /// Single step (while paused).
    pub step: bool,
    /// Reset the look rotation.
    pub reset_look: bool,
    /// Save a bookmark under [`PersistPanel::name`].
    pub save_bookmark: bool,
    /// Load the bookmark with this index in [`PersistPanel::bookmarks`].
    pub load_bookmark: Option<usize>,
    /// Start recording a camera path.
    pub start_recording: bool,
    /// Stop recording and save the path under [`PersistPanel::name`].
    pub stop_recording: bool,
    /// Play the path with this index in [`PersistPanel::paths`].
    pub play_path: Option<usize>,
    /// Stop playback.
    pub stop_playback: bool,
    /// Re-read the bookmark and path directories.
    pub refresh_files: bool,
}

/// Bookmarks and camera paths panel state.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PersistPanel {
    /// Name for the next bookmark or path file.
    pub name: String,
    /// Bookmark files (names) in the bookmark directory.
    pub bookmarks: Vec<String>,
    /// Path files (names) in the path directory.
    pub paths: Vec<String>,
    /// Proper time between recorded samples, `M`.
    pub interval: f64,
    /// Playback rate, `M` of proper time per second.
    pub speed: f64,
    /// Loop playback.
    pub looping: bool,
    /// Samples recorded so far, while recording.
    pub recording: Option<usize>,
    /// Current and total samples, while playing.
    pub playing: Option<(usize, usize)>,
}

/// Draws the left panel, editing `settings` and `flight` in place. `check` is the validity of `flight.editor`.
pub fn settings_panel(
    ui: &mut egui::Ui,
    settings: &mut ViewSettings,
    flight: &mut FlightPanel,
    persist: &mut PersistPanel,
    check: &PresetCheck,
    status: &Status,
) -> Actions {
    let mut actions = Actions::default();
    egui::ScrollArea::vertical().show(ui, |ui| {
        ui.heading("Gravography");
        ui.label(&status.gpu);
        ui.separator();
        flight_section(ui, flight, &mut actions);
        ui.small(if status.mouse_captured {
            "Mouse look captured: move the mouse to look. Right-click or Esc releases."
        } else {
            "Look around: drag with the left button, or right-click for mouse look."
        });
        preset_section(ui, &mut flight.editor, check, &mut actions);
        ui.collapsing("Spacetime", |ui| {
            ui.add(
                egui::Slider::new(&mut flight.spin, 0.0..=MAX_SPIN)
                    .text("spin a/M")
                    .fixed_decimals(3),
            );
            ui.small("Changing spin keeps the position and velocity; invalid states reset.");
        });
        egui::ComboBox::from_label("band")
            .selected_text(settings.band.label())
            .show_ui(ui, |ui| {
                for b in gravography_render::scene::BandKind::ALL {
                    ui.selectable_value(&mut settings.band, b, b.label());
                }
            });
        ui.collapsing("Effects", |ui| {
            ui.checkbox(&mut settings.effects.lensing, "gravitational lensing");
            ui.checkbox(&mut settings.effects.frequency_shift, "frequency shift");
            ui.checkbox(&mut settings.effects.beaming, "intensity beaming");
        });
        render_section(ui, settings, &mut actions);
        persist_section(ui, persist, &mut actions);
        ui.separator();
        ui.collapsing("Keys", |ui| {
            for line in [
                "left-drag: look around",
                "right-click: captured mouse look (Esc releases)",
                "W/S A/D R/F: thrust fwd/back, left/right, up/down",
                "Space: cut thrust (free fall)",
                "Enter: pause   .: step   [ ]: time warp",
                "- =: thrust   P: screenshot   Esc: quit (or release mouse)",
            ] {
                ui.small(line);
            }
        });
        if !status.message.is_empty() {
            ui.separator();
            ui.label(&status.message);
        }
    });
    actions
}

fn flight_section(ui: &mut egui::Ui, flight: &mut FlightPanel, actions: &mut Actions) {
    egui::CollapsingHeader::new("Flight")
        .default_open(true)
        .show(ui, |ui| {
            let s = &mut flight.settings;
            ui.horizontal(|ui| {
                let label = if s.paused { "Resume" } else { "Pause" };
                if ui.button(label).clicked() {
                    s.paused = !s.paused;
                }
                if ui.add_enabled(s.paused, egui::Button::new("Step")).clicked() {
                    actions.step = true;
                }
            });
            ui.add(
                egui::Slider::new(&mut s.time_warp, WARP_RANGE.0..=WARP_RANGE.1)
                    .logarithmic(true)
                    .text("time warp (M/s)"),
            );
            ui.add(
                egui::Slider::new(&mut s.thrust, THRUST_RANGE.0..=THRUST_RANGE.1)
                    .logarithmic(true)
                    .text("thrust (c²/M)"),
            );
            ui.add(
                egui::Slider::new(&mut s.mass_solar, 1.0..=1e10)
                    .logarithmic(true)
                    .text("mass (M_sun)"),
            );
            ui.horizontal(|ui| {
                if ui.button("Cut thrust").clicked() {
                    actions.release = true;
                }
                if ui.button("Reset look").clicked() {
                    actions.reset_look = true;
                }
            });
        });
}

fn preset_section(ui: &mut egui::Ui, spec: &mut ObserverSpec, check: &PresetCheck, actions: &mut Actions) {
    egui::CollapsingHeader::new("Presets")
        .default_open(true)
        .show(ui, |ui| {
            egui::ComboBox::from_label("preset")
                .selected_text(preset_name(spec.preset))
                .show_ui(ui, |ui| {
                    for kind in [
                        PresetKind::Hover,
                        PresetKind::Zamo,
                        PresetKind::CircularOrbit,
                        PresetKind::RadialInfall,
                    ] {
                        ui.selectable_value(&mut spec.preset, kind, preset_name(kind));
                    }
                });
            ui.add(
                egui::Slider::new(&mut spec.r, 0.05..=200.0)
                    .logarithmic(true)
                    .text("r (M)"),
            );
            if spec.preset == PresetKind::CircularOrbit {
                ui.checkbox(&mut spec.prograde, "prograde");
            } else {
                ui.add(egui::Slider::new(&mut spec.theta_deg, 0.0..=180.0).text("θ (°)"));
            }
            ui.add(egui::Slider::new(&mut spec.phi_deg, -180.0..=180.0).text("φ (°)"));
            if ui
                .add_enabled(check.error.is_none(), egui::Button::new("Apply preset"))
                .clicked()
            {
                actions.apply_preset = true;
            }
            if let Some(e) = &check.error {
                ui.colored_label(ui.visuals().error_fg_color, e);
            } else if let Some(w) = &check.warning {
                ui.colored_label(ui.visuals().warn_fg_color, w);
            }
        });
}

fn render_section(ui: &mut egui::Ui, settings: &mut ViewSettings, actions: &mut Actions) {
    ui.collapsing("Render", |ui| {
        let rectilinear = matches!(settings.projection, Projection::Rectilinear { .. });
        egui::ComboBox::from_label("projection")
            .selected_text(if rectilinear {
                "rectilinear"
            } else {
                "equirectangular 360°"
            })
            .show_ui(ui, |ui| {
                if ui.selectable_label(rectilinear, "rectilinear").clicked() && !rectilinear {
                    settings.projection = Projection::Rectilinear { fov_deg: 60.0 };
                }
                if ui
                    .selectable_label(!rectilinear, "equirectangular 360°")
                    .clicked()
                {
                    settings.projection = Projection::Equirectangular;
                }
            });
        if let Projection::Rectilinear { fov_deg } = &mut settings.projection {
            ui.add(egui::Slider::new(fov_deg, FOV_RANGE.0..=FOV_RANGE.1).text("vertical FOV (°)"));
        }
        ui.add(
            egui::Slider::new(&mut settings.samples, 1..=MAX_SAMPLE_TARGET)
                .text("samples")
                .logarithmic(true),
        );
        ui.add(
            egui::Slider::new(
                &mut settings.render_scale,
                RENDER_SCALE_RANGE.0..=RENDER_SCALE_RANGE.1,
            )
            .text("still scale"),
        );
        ui.checkbox(
            &mut settings.dynamic_resolution,
            "dynamic resolution while moving",
        );
        ui.add_enabled(
            settings.dynamic_resolution,
            egui::Slider::new(&mut settings.target_fps, 30.0..=120.0).text("target fps"),
        );
        ui.checkbox(&mut settings.exposure.auto, "auto exposure");
        let label = if settings.exposure.auto {
            "EV compensation"
        } else {
            "EV"
        };
        ui.add(egui::Slider::new(&mut settings.exposure.ev, -12.0..=12.0).text(label));
        egui::ComboBox::from_label("tone map")
            .selected_text(match settings.tone_map {
                ToneMap::Aces => "ACES",
                ToneMap::Reinhard => "Reinhard",
            })
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut settings.tone_map, ToneMap::Aces, "ACES");
                ui.selectable_value(&mut settings.tone_map, ToneMap::Reinhard, "Reinhard");
            });
        if ui.button("Screenshot (P)").clicked() {
            actions.screenshot = true;
        }
    });
}

fn persist_section(ui: &mut egui::Ui, p: &mut PersistPanel, actions: &mut Actions) {
    ui.collapsing("Bookmarks & camera paths", |ui| {
        ui.horizontal(|ui| {
            ui.label("name");
            ui.text_edit_singleline(&mut p.name);
        });
        if ui.button("Save bookmark").clicked() {
            actions.save_bookmark = true;
        }
        for (i, b) in p.bookmarks.iter().enumerate() {
            if ui.button(format!("load {b}")).clicked() {
                actions.load_bookmark = Some(i);
            }
        }
        ui.separator();
        ui.add(
            egui::Slider::new(&mut p.interval, 0.05..=20.0)
                .logarithmic(true)
                .text("record every (M)"),
        );
        match p.recording {
            Some(n) => {
                ui.label(format!("recording: {n} samples"));
                if ui.button("Stop & save path").clicked() {
                    actions.stop_recording = true;
                }
            }
            None => {
                if ui
                    .add_enabled(p.playing.is_none(), egui::Button::new("Record path"))
                    .clicked()
                {
                    actions.start_recording = true;
                }
            }
        }
        ui.add(
            egui::Slider::new(&mut p.speed, 0.01..=1000.0)
                .logarithmic(true)
                .text("playback (M/s)"),
        );
        ui.checkbox(&mut p.looping, "loop");
        if let Some((k, n)) = p.playing {
            ui.label(format!("playing sample {}/{n}", k + 1));
            if ui.button("Stop playback").clicked() {
                actions.stop_playback = true;
            }
        } else {
            for (i, name) in p.paths.iter().enumerate() {
                if ui
                    .add_enabled(p.recording.is_none(), egui::Button::new(format!("play {name}")))
                    .clicked()
                {
                    actions.play_path = Some(i);
                }
            }
        }
        if ui.small_button("refresh lists").clicked() {
            actions.refresh_files = true;
        }
    });
}

/// Human-readable region name (FR-O4).
pub fn region_name(region: Region) -> &'static str {
    match region {
        Region::OutsideErgosphere => "outside the ergosphere",
        Region::Ergosphere => "in the ergosphere",
        Region::InsideHorizon => "inside the horizon",
    }
}

/// Human-readable motion mode.
pub fn motion_name(motion: &Motion<f64>) -> &'static str {
    match motion {
        Motion::Free => "free",
        Motion::Hover { .. } => "hovering (auto thrust)",
        Motion::Zamo { .. } => "ZAMO (auto thrust)",
    }
}

/// HUD text lines (FR-O4), separated from drawing so they can be tested.
pub fn hud_lines(hud: &Hud, status: &Status, settings: &FlightSettings, render_scale: f32) -> Vec<String> {
    let mut lines = vec![
        format!("τ = {:.3} M   t = {:.3} M (KS)", hud.tau, hud.t_ks),
        format!(
            "r = {:.4} M   θ = {:.2}°   φ = {}",
            hud.r,
            hud.theta_deg,
            hud.phi_deg
                .map_or_else(|| "—".to_string(), |p| format!("{p:.2}°"))
        ),
        match hud.zamo_speed {
            Some((beta, gamma)) => format!("v vs ZAMO = {beta:.5} c   γ = {gamma:.5}"),
            None => "v vs ZAMO: — (no ZAMOs inside the horizon)".to_string(),
        },
        format!(
            "acceleration {:.3e} c²/M = {:.3e} g (M = {} M_sun)",
            hud.acceleration, hud.acceleration_g, settings.mass_solar
        ),
        format!("{}, {}", region_name(hud.region), motion_name(&hud.motion)),
        format!(
            "a = {:.3}   warp {:.3} M/s{}",
            hud.spin,
            settings.time_warp,
            if settings.paused { "   PAUSED" } else { "" }
        ),
        format!(
            "{:.0} fps   GPU {}   scale {:.2} ({}×{})   samples {}/{}",
            status.fps,
            status
                .gpu_ms
                .map_or_else(|| "—".to_string(), |m| format!("{m:.1} ms")),
            render_scale,
            status.render_size.0,
            status.render_size.1,
            status.samples.0,
            status.samples.1
        ),
    ];
    if let Some(r) = status.ray_rates {
        lines.push(format!(
            "rays: escaped {:.1}%  captured {:.1}%  max-steps {:.2}%  singular {:.2}%  error {:.2}%",
            100.0 * r[0],
            100.0 * r[1],
            100.0 * r[3],
            100.0 * r[2],
            100.0 * r[4]
        ));
    }
    lines
}

/// Draws the HUD in the top-right corner.
pub fn hud_window(ctx: &egui::Context, lines: &[String]) {
    egui::Window::new("HUD")
        .anchor(egui::Align2::RIGHT_TOP, [-8.0, 8.0])
        .resizable(false)
        .collapsible(true)
        .show(ctx, |ui| {
            for l in lines {
                ui.monospace(l);
            }
        });
}

#[cfg(test)]
mod tests {
    use super::super::flight::{Flight, safe_preset};
    use super::super::input::ThrustKeys;
    use super::*;
    use gravography_physics::Kerr;

    #[test]
    fn hud_shows_every_fr_o4_item() {
        let f = Flight::new(Kerr::new(0.5).unwrap(), &safe_preset()).unwrap();
        let hud = Hud::new(&f, &ThrustKeys::default());
        let status = Status {
            fps: 60.0,
            render_size: (640, 360),
            samples: (3, 16),
            ray_rates: Some([0.9, 0.1, 0.0, 0.0, 0.0]),
            ..Status::default()
        };
        let text = hud_lines(&hud, &status, &f.settings, 0.5).join("\n");
        for needle in [
            "τ =",
            "t =",
            "r = 30.0000",
            "θ = 90.00°",
            "φ =",
            "v vs ZAMO",
            "γ =",
            " g (M = 10 M_sun)",
            "outside the ergosphere",
            "hovering",
            "a = 0.500",
            "60 fps",
            "scale 0.50",
            "escaped 90.0%",
        ] {
            assert!(text.contains(needle), "missing {needle:?} in\n{text}");
        }
    }
}
