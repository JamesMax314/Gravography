//! Keyboard and mouse mapping for flight (`docs/PRD.md` FR-O1, FR-O3).
//!
//! Physical key codes are used, so the WASD layout stays in place on non-QWERTY keyboards. Thrust keys are *held*:
//! the viewer tracks press and release in [`ThrustKeys`]; every other action fires once per press.

use winit::keyboard::KeyCode;

/// One of the six held thrust keys, as a direction in the view frame (forward, left, up).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ThrustKey {
    /// W: forward.
    Forward,
    /// S: backward.
    Back,
    /// A: left.
    Left,
    /// D: right.
    Right,
    /// R: up.
    Up,
    /// F: down.
    Down,
}

/// What a key does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeyAction {
    /// Thrust while held.
    Thrust(ThrustKey),
    /// Space: cut all thrust, including automatic station keeping (switches to free fall).
    CutThrust,
    /// Enter: pause or resume the simulation.
    TogglePause,
    /// Period: advance one step while paused.
    Step,
    /// `[`: halve the time warp.
    WarpSlower,
    /// `]`: double the time warp.
    WarpFaster,
    /// `-`: halve the thrust magnitude.
    ThrustWeaker,
    /// `=`: double the thrust magnitude.
    ThrustStronger,
    /// P: screenshot.
    Screenshot,
    /// Esc: quit.
    Quit,
}

/// Key binding of the viewer.
pub fn key_action(code: KeyCode) -> Option<KeyAction> {
    use KeyAction as A;
    Some(match code {
        KeyCode::KeyW => A::Thrust(ThrustKey::Forward),
        KeyCode::KeyS => A::Thrust(ThrustKey::Back),
        KeyCode::KeyA => A::Thrust(ThrustKey::Left),
        KeyCode::KeyD => A::Thrust(ThrustKey::Right),
        KeyCode::KeyR => A::Thrust(ThrustKey::Up),
        KeyCode::KeyF => A::Thrust(ThrustKey::Down),
        KeyCode::Space => A::CutThrust,
        KeyCode::Enter | KeyCode::NumpadEnter => A::TogglePause,
        KeyCode::Period => A::Step,
        KeyCode::BracketLeft => A::WarpSlower,
        KeyCode::BracketRight => A::WarpFaster,
        KeyCode::Minus | KeyCode::NumpadSubtract => A::ThrustWeaker,
        KeyCode::Equal | KeyCode::NumpadAdd => A::ThrustStronger,
        KeyCode::KeyP => A::Screenshot,
        KeyCode::Escape => A::Quit,
        _ => return None,
    })
}

/// Which thrust keys are held.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ThrustKeys {
    held: [bool; 6],
}

impl ThrustKeys {
    fn index(key: ThrustKey) -> usize {
        match key {
            ThrustKey::Forward => 0,
            ThrustKey::Back => 1,
            ThrustKey::Left => 2,
            ThrustKey::Right => 3,
            ThrustKey::Up => 4,
            ThrustKey::Down => 5,
        }
    }

    /// Records a press (`pressed = true`) or release of `key`.
    pub fn set(&mut self, key: ThrustKey, pressed: bool) {
        self.held[Self::index(key)] = pressed;
    }

    /// Releases every key (Space, focus loss).
    pub fn clear(&mut self) {
        self.held = [false; 6];
    }

    /// Whether any thrust key is held.
    pub fn any(&self) -> bool {
        self.held.iter().any(|&h| h)
    }

    /// Unit thrust direction in the view frame `(forward, left, up)`, or zero when no key is held or opposite keys
    /// cancel. Diagonals are normalised so that the thrust magnitude does not depend on how many keys are held.
    pub fn direction(&self) -> [f64; 3] {
        let axis = |plus: ThrustKey, minus: ThrustKey| {
            f64::from(u8::from(self.held[Self::index(plus)]))
                - f64::from(u8::from(self.held[Self::index(minus)]))
        };
        let d = [
            axis(ThrustKey::Forward, ThrustKey::Back),
            axis(ThrustKey::Left, ThrustKey::Right),
            axis(ThrustKey::Up, ThrustKey::Down),
        ];
        let n = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt();
        if n > 0.0 { d.map(|c| c / n) } else { [0.0; 3] }
    }
}

/// Look-rotation change `(Δyaw, Δpitch)` in degrees for a drag of `(dx, dy)` physical pixels ("grab the sky":
/// dragging right turns the view left, i.e. positive yaw; dragging down looks up, i.e. positive pitch, since
/// window `y` grows downward).
pub fn drag_to_look(dx: f64, dy: f64, deg_per_px: f64) -> (f64, f64) {
    (dx * deg_per_px, dy * deg_per_px)
}

/// Mouse buttons that matter for looking.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LookButton {
    /// Left: drag to look ("grab the sky").
    Left,
    /// Right: toggle captured mouse look.
    Right,
}

/// Mouse-look state (`docs/PRD.md` FR-O1 "drag or captured cursor"; both are supported):
/// - **drag:** hold the left button over the 3D view and move; the sky follows the cursor;
/// - **captured:** a right click over the 3D view hides and locks the cursor, and mouse motion turns the view like
///   a first-person game (mouse right turns right). Another right click, Esc or focus loss releases it.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MouseLook {
    dragging: bool,
    last: Option<(f64, f64)>,
    captured: bool,
}

impl MouseLook {
    /// Whether the cursor is captured.
    pub fn captured(&self) -> bool {
        self.captured
    }

    /// Whether a drag is in progress.
    pub fn dragging(&self) -> bool {
        self.dragging
    }

    /// A button press or release. `consumed`: egui wants this click (the pointer is over the panel or the HUD).
    /// Returns `Some(captured)` when the capture state changed (the viewer then grabs or releases the cursor).
    pub fn button(&mut self, button: LookButton, pressed: bool, consumed: bool) -> Option<bool> {
        match button {
            LookButton::Left => {
                // Start a drag only over the 3D view; always end it.
                self.dragging = pressed && !consumed && !self.captured;
                None
            }
            LookButton::Right if pressed && (!consumed || self.captured) => {
                self.dragging = false;
                self.captured = !self.captured;
                Some(self.captured)
            }
            LookButton::Right => None,
        }
    }

    /// Cursor moved to `pos` (physical pixels). Returns the look change `(Δyaw, Δpitch)` in degrees while dragging.
    pub fn cursor_moved(&mut self, pos: (f64, f64), deg_per_px: f64) -> Option<(f64, f64)> {
        let last = self.last.replace(pos);
        match (self.dragging, last) {
            (true, Some(l)) => Some(drag_to_look(pos.0 - l.0, pos.1 - l.1, deg_per_px)),
            _ => None,
        }
    }

    /// Raw mouse motion `(dx, dy)` (device units, about a pixel each). Returns the look change while captured:
    /// first-person convention, the opposite sign of a drag (mouse right turns right, i.e. negative yaw; mouse
    /// down looks down).
    pub fn raw_motion(&self, delta: (f64, f64), deg_per_px: f64) -> Option<(f64, f64)> {
        self.captured
            .then(|| drag_to_look(-delta.0, -delta.1, deg_per_px))
    }

    /// Releases drag and capture (Esc, focus loss). Returns `true` if the cursor was captured.
    pub fn release(&mut self) -> bool {
        self.dragging = false;
        std::mem::take(&mut self.captured)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_relative_eq;

    #[test]
    fn wasd_rf_map_to_view_axes() {
        let cases = [
            (KeyCode::KeyW, [1.0, 0.0, 0.0]),
            (KeyCode::KeyS, [-1.0, 0.0, 0.0]),
            (KeyCode::KeyA, [0.0, 1.0, 0.0]),
            (KeyCode::KeyD, [0.0, -1.0, 0.0]),
            (KeyCode::KeyR, [0.0, 0.0, 1.0]),
            (KeyCode::KeyF, [0.0, 0.0, -1.0]),
        ];
        for (code, expect) in cases {
            let Some(KeyAction::Thrust(k)) = key_action(code) else {
                panic!("{code:?} is not a thrust key");
            };
            let mut keys = ThrustKeys::default();
            keys.set(k, true);
            assert!(keys.any());
            assert_eq!(keys.direction(), expect, "{code:?}");
            keys.set(k, false);
            assert!(!keys.any());
            assert_eq!(keys.direction(), [0.0; 3]);
        }
        assert_eq!(key_action(KeyCode::Space), Some(KeyAction::CutThrust));
        assert_eq!(key_action(KeyCode::KeyP), Some(KeyAction::Screenshot));
        assert_eq!(key_action(KeyCode::KeyQ), None);
    }

    #[test]
    fn diagonals_are_unit_and_opposites_cancel() {
        let mut keys = ThrustKeys::default();
        keys.set(ThrustKey::Forward, true);
        keys.set(ThrustKey::Left, true);
        keys.set(ThrustKey::Up, true);
        let d = keys.direction();
        let s = 1.0 / 3.0_f64.sqrt();
        for c in d {
            // Exact up to one rounding of 1/√3.
            assert_relative_eq!(c, s, epsilon = 1e-15);
        }
        keys.set(ThrustKey::Back, true);
        keys.set(ThrustKey::Right, true);
        keys.set(ThrustKey::Down, true);
        assert!(keys.any());
        assert_eq!(keys.direction(), [0.0; 3]);
        keys.clear();
        assert!(!keys.any());
    }

    #[test]
    fn drag_scales_with_sensitivity() {
        // Sensitivity itself: Projection::degrees_per_pixel (render crate, tested there).
        assert_eq!(drag_to_look(10.0, -4.0, 0.5), (5.0, -2.0));
    }

    #[test]
    fn drag_looks_only_from_the_3d_view() {
        let mut m = MouseLook::default();
        // Moving without a button does nothing (but records the position).
        assert_eq!(m.cursor_moved((100.0, 100.0), 0.5), None);
        // A press on the panel (consumed) does not start a drag.
        m.button(LookButton::Left, true, true);
        assert!(!m.dragging());
        assert_eq!(m.cursor_moved((110.0, 100.0), 0.5), None);
        // A press over the 3D view does; the sky follows the cursor.
        m.button(LookButton::Left, true, false);
        assert!(m.dragging());
        assert_eq!(m.cursor_moved((130.0, 90.0), 0.5), Some((10.0, -5.0)));
        // Release always ends it, even if egui claims the release.
        m.button(LookButton::Left, false, true);
        assert_eq!(m.cursor_moved((200.0, 90.0), 0.5), None);
    }

    #[test]
    fn right_click_toggles_captured_look() {
        let mut m = MouseLook::default();
        assert_eq!(m.raw_motion((5.0, 0.0), 1.0), None);
        assert_eq!(m.button(LookButton::Right, true, true), None, "not on the panel");
        assert_eq!(m.button(LookButton::Right, true, false), Some(true));
        assert!(m.captured());
        // First-person: mouse right turns right (negative yaw), mouse down looks down.
        assert_eq!(m.raw_motion((4.0, 2.0), 0.5), Some((-2.0, -1.0)));
        // No drag while captured; releasing the right button changes nothing.
        m.button(LookButton::Left, true, false);
        assert!(!m.dragging());
        assert_eq!(m.button(LookButton::Right, false, false), None);
        // A second right click releases, even over the (now irrelevant) panel area.
        assert_eq!(m.button(LookButton::Right, true, true), Some(false));
        m.button(LookButton::Right, true, false);
        assert!(m.release());
        assert!(!m.captured() && !m.release());
    }
}
