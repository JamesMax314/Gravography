//! Camera projection: pixel coordinates → unit viewing directions in the camera frame (`docs/PRD.md` FR-R2;
//! `docs/physics.md` § "Rendering").
//!
//! Directions are tetrad components `n = (n_forward, n_left, n_up)`. Pixel coordinates are continuous: pixel
//! `(i, j)` covers `[i, i + 1) × [j, j + 1)`, its centre is `(i + ½, j + ½)`, `x` grows to the right and `y`
//! downward (row 0 is the top of the image).

use serde::{Deserialize, Serialize};

/// Projection from the camera frame onto the image.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Projection {
    /// Pinhole camera with vertical field of view `fov_deg` (degrees, `0 < fov < 180`); square pixels.
    Rectilinear {
        /// Vertical field of view in degrees.
        fov_deg: f64,
    },
    /// Full-sphere equirectangular (latitude–longitude) map: the centre looks forward, `x` spans longitude
    /// `+180° … −180°` (left to right), `y` latitude `+90° … −90°` (top to bottom). Use a 2:1 image for square
    /// pixels.
    Equirectangular,
}

impl Default for Projection {
    /// Rectilinear, 60° vertical field of view.
    fn default() -> Self {
        Self::Rectilinear { fov_deg: 60.0 }
    }
}

impl Projection {
    /// Checks the parameters, returning a message for invalid ones.
    pub fn validate(&self) -> Result<(), String> {
        match *self {
            Self::Rectilinear { fov_deg } if !(fov_deg > 0.0 && fov_deg < 180.0) => {
                Err(format!("rectilinear fov_deg must be in (0, 180), got {fov_deg}"))
            }
            Self::Rectilinear { .. } | Self::Equirectangular => Ok(()),
        }
    }

    /// Unit viewing direction `(forward, left, up)` through the continuous pixel coordinate `(x, y)` of a
    /// `width × height` image.
    ///
    /// Rectilinear: the image plane sits at unit distance along forward with half-height `t = tan(fov/2)`; the
    /// point is `(1, −(2x/W − 1) t W/H, (1 − 2y/H) t)`, normalised.
    ///
    /// Equirectangular: longitude `λ = −π (2x/W − 1)`, latitude `φ = π (½ − y/H)`, direction
    /// `(cos φ cos λ, cos φ sin λ, sin φ)`.
    pub fn direction(&self, width: u32, height: u32, x: f64, y: f64) -> [f64; 3] {
        let (w, h) = (f64::from(width), f64::from(height));
        match *self {
            Self::Rectilinear { fov_deg } => {
                let t = (0.5 * fov_deg.to_radians()).tan();
                let v = [1.0, -(2.0 * x / w - 1.0) * t * w / h, (1.0 - 2.0 * y / h) * t];
                normalise(&v)
            }
            Self::Equirectangular => {
                let lon = -core::f64::consts::PI * (2.0 * x / w - 1.0);
                let lat = core::f64::consts::PI * (0.5 - y / h);
                let (sl, cl) = lon.sin_cos();
                let (sp, cp) = lat.sin_cos();
                [cp * cl, cp * sl, sp]
            }
        }
    }

    /// Solid angle subtended by a unit pixel area around continuous pixel coordinate `(x, y)` in the camera
    /// frame (sr per pixel², i.e. the Jacobian `|dΩ/dx dy|`).
    ///
    /// Rectilinear: with pixel pitch `Δ = 2 tan(fov/2)/H` on the unit-distance image plane, `dΩ = Δ² cos³θ`, where
    /// `θ` is the angle from the optical axis (`cos θ = n_forward`).
    ///
    /// Equirectangular: `dΩ = cos φ dλ dφ = cos φ (2π/W)(π/H)`.
    pub fn pixel_solid_angle(&self, width: u32, height: u32, x: f64, y: f64) -> f64 {
        match *self {
            Self::Rectilinear { fov_deg } => {
                let pitch = 2.0 * (0.5 * fov_deg.to_radians()).tan() / f64::from(height);
                let c = self.direction(width, height, x, y)[0];
                pitch * pitch * c * c * c
            }
            Self::Equirectangular => {
                let lat = core::f64::consts::PI * (0.5 - y / f64::from(height));
                let pi = core::f64::consts::PI;
                lat.cos() * (2.0 * pi / f64::from(width)) * (pi / f64::from(height))
            }
        }
    }

    /// Degrees of view per pixel at the image centre (mouse-look sensitivity). Rectilinear: the pinhole scale
    /// `2 tan(fov/2)/H` rad; equirectangular: `180°/H` (= `360°/W` for a 2:1 image).
    pub fn degrees_per_pixel(&self, height: u32) -> f64 {
        let h = f64::from(height.max(1));
        match *self {
            Self::Rectilinear { fov_deg } => (2.0 * (0.5 * fov_deg.to_radians()).tan() / h).to_degrees(),
            Self::Equirectangular => 180.0 / h,
        }
    }
}

pub(crate) fn normalise(v: &[f64; 3]) -> [f64; 3] {
    let n = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
    v.map(|c| c / n)
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_relative_eq;

    #[test]
    fn centre_and_edges() {
        // Image centre looks forward; the top edge midpoint is fov/2 above the axis (definition of the vertical
        // field of view); the right edge is to the right (negative "left" component).
        let p = Projection::Rectilinear { fov_deg: 60.0 };
        let (w, h) = (400, 200);
        let c = p.direction(w, h, 200.0, 100.0);
        assert_relative_eq!(c[0], 1.0, epsilon = 1e-15);
        assert_relative_eq!(c[1], 0.0, epsilon = 1e-15);
        let top = p.direction(w, h, 200.0, 0.0);
        assert_relative_eq!(top[2].atan2(top[0]), 30f64.to_radians(), epsilon = 1e-14);
        assert_relative_eq!(top[1], 0.0, epsilon = 1e-15);
        let right = p.direction(w, h, 400.0, 100.0);
        assert!(right[1] < 0.0);
        // Horizontal half-angle: atan(tan(30°) · W/H) for square pixels.
        let expect = (30f64.to_radians().tan() * 2.0).atan();
        assert_relative_eq!((-right[1]).atan2(right[0]), expect, epsilon = 1e-14);
        for d in [c, top, right] {
            assert_relative_eq!(d.iter().map(|x| x * x).sum::<f64>(), 1.0, epsilon = 1e-15);
        }
    }

    #[test]
    fn pixel_solid_angles_sum_to_frustum() {
        // The solid angle of a rectangular pyramid with half-angles α, β is 4 asin(sin α sin β) (e.g. Mathar,
        // "Solid angle of a rectangular plate", 2015). Summing the per-pixel Jacobian at pixel centres (midpoint
        // rule, 400 × 300 pixels) converges to it with relative error O(pixel²) ≈ 1e-5.
        let p = Projection::Rectilinear { fov_deg: 90.0 };
        let (w, h) = (400_u32, 300_u32);
        let mut total = 0.0;
        for j in 0..h {
            for i in 0..w {
                total += p.pixel_solid_angle(w, h, f64::from(i) + 0.5, f64::from(j) + 0.5);
            }
        }
        let tb = 45f64.to_radians().tan();
        let (alpha, beta) = ((tb * f64::from(w) / f64::from(h)).atan(), tb.atan());
        let exact = 4.0 * (alpha.sin() * beta.sin()).asin();
        assert_relative_eq!(total, exact, max_relative = 1e-5);
    }

    #[test]
    fn equirectangular_covers_the_sphere() {
        // Centre forward, left/right edges backward, top/bottom rows at the poles; the quarter points look left
        // (x = W/4) and right (x = 3W/4); the per-pixel solid angles sum to 4π (midpoint rule in latitude:
        // relative error ≈ (π/H)²/24 ≈ 3e-6 at H = 300; bound 1e-5).
        let p = Projection::Equirectangular;
        let (w, h) = (600_u32, 300_u32);
        let d = |x: f64, y: f64| p.direction(w, h, x, y);
        let close = |a: [f64; 3], b: [f64; 3]| {
            for k in 0..3 {
                assert_relative_eq!(a[k], b[k], epsilon = 1e-14);
            }
        };
        close(d(300.0, 150.0), [1.0, 0.0, 0.0]);
        close(d(150.0, 150.0), [0.0, 1.0, 0.0]);
        close(d(450.0, 150.0), [0.0, -1.0, 0.0]);
        close(d(0.0, 150.0), [-1.0, 0.0, 0.0]);
        close(d(300.0, 0.0), [0.0, 0.0, 1.0]);
        close(d(300.0, 300.0), [0.0, 0.0, -1.0]);
        let mut total = 0.0;
        for j in 0..h {
            for i in 0..w {
                total += p.pixel_solid_angle(w, h, f64::from(i) + 0.5, f64::from(j) + 0.5);
            }
        }
        assert_relative_eq!(total, 4.0 * core::f64::consts::PI, max_relative = 1e-5);
        assert_relative_eq!(p.degrees_per_pixel(300), 0.6, epsilon = 1e-15);
        // Rectilinear sensitivity: the pinhole pitch at the centre (90° on 1000 px: 2e-3 rad).
        let r = Projection::Rectilinear { fov_deg: 90.0 };
        assert_relative_eq!(
            r.degrees_per_pixel(1000),
            2e-3_f64.to_degrees(),
            max_relative = 1e-14
        );
        assert!(p.validate().is_ok());
    }

    #[test]
    fn validation() {
        assert!(Projection::Rectilinear { fov_deg: 0.0 }.validate().is_err());
        assert!(Projection::Rectilinear { fov_deg: 180.0 }.validate().is_err());
        assert!(Projection::Rectilinear { fov_deg: f64::NAN }.validate().is_err());
        assert!(Projection::default().validate().is_ok());
    }
}
