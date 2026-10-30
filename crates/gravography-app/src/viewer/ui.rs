//! The egui settings panel (scaffolding for WP-11): render settings, exposure, status and the screenshot button.

use gravography_render::ToneMap;

use super::state::{MAX_SAMPLE_TARGET, RENDER_SCALE_RANGE, ViewSettings};

/// Read-only status shown in the panel.
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
    /// Last message (screenshot saved, errors).
    pub message: String,
}

/// Actions requested from the panel.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Actions {
    /// Take a screenshot.
    pub screenshot: bool,
}

/// Draws the panel, editing `settings` in place.
pub fn settings_panel(ui: &mut egui::Ui, settings: &mut ViewSettings, status: &Status) -> Actions {
    let mut actions = Actions::default();
    ui.heading("Gravography");
    ui.label(&status.gpu);
    ui.separator();

    ui.label("Render");
    ui.add(egui::Slider::new(&mut settings.samples, 1..=MAX_SAMPLE_TARGET).text("samples").logarithmic(true));
    ui.add(egui::Slider::new(&mut settings.render_scale, RENDER_SCALE_RANGE.0..=RENDER_SCALE_RANGE.1).text("render scale"));
    ui.separator();

    ui.label("Exposure");
    ui.checkbox(&mut settings.exposure.auto, "auto exposure");
    let label = if settings.exposure.auto { "EV compensation" } else { "EV" };
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
    ui.separator();

    if ui.button("Screenshot (P)").clicked() {
        actions.screenshot = true;
    }
    ui.separator();
    ui.label(format!("{:.1} fps", status.fps));
    ui.label(format!("render {}×{}", status.render_size.0, status.render_size.1));
    ui.label(format!("samples {}/{}", status.samples.0, status.samples.1));
    ui.label(format!("exposure scale {:.3e}", status.exposure_scale));
    if !status.message.is_empty() {
        ui.separator();
        ui.label(&status.message);
    }
    actions
}
