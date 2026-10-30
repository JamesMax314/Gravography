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

/// Mouse-look sensitivity: degrees of yaw/pitch per physical pixel of drag, chosen so that the sky follows the
/// cursor at the centre of a rectilinear view of vertical field of view `fov_deg` on a window `height` pixels tall.
pub fn degrees_per_pixel(fov_deg: f64, height: u32) -> f64 {
    let h = f64::from(height.max(1));
    // Centre of a pinhole image: one pixel subtends 2 tan(fov/2) / h radians.
    (2.0 * (0.5 * fov_deg.to_radians()).tan() / h).to_degrees()
}

/// Look-rotation change `(Δyaw, Δpitch)` in degrees for a drag of `(dx, dy)` physical pixels ("grab the sky":
/// dragging right turns the view left, i.e. positive yaw; dragging down looks up, i.e. positive pitch, since
/// window `y` grows downward).
pub fn drag_to_look(dx: f64, dy: f64, deg_per_px: f64) -> (f64, f64) {
    (dx * deg_per_px, dy * deg_per_px)
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
    fn mouse_sensitivity_matches_pinhole_centre() {
        // 90° fov on 1000 px: one pixel at the centre subtends 2 tan(45°)/1000 = 2e-3 rad.
        assert_relative_eq!(
            degrees_per_pixel(90.0, 1000),
            2e-3_f64.to_degrees(),
            max_relative = 1e-14
        );
        assert!(degrees_per_pixel(60.0, 0).is_finite());
        assert_eq!(drag_to_look(10.0, -4.0, 0.5), (5.0, -2.0));
    }
}
