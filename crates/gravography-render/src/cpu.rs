//! CPU f64 reference renderer (rayon): the ground truth for GPU parity tests and reference images
//! (`docs/PRD.md` §4.2; `docs/physics.md` § "Rendering").
//!
//! Per sample: build the past-directed camera ray, trace it ([`trace_ray`]), and for escaped rays estimate the sky
//! footprint by **ray differentials**: two extra rays offset by `ε` pixels in `x` and `y` give the Jacobian
//! `J = ∂d/∂(x, y)` of the escape direction, and the footprint of one pixel is `|J_x × J_y|` steradians. The sky is
//! sampled with that footprint ([`Sky::shade`]), converted to linear sRGB and averaged over the samples of a
//! pixel. Pixels are independent, so the image does not depend on thread scheduling (NF-3).

use gravography_physics::linalg::Mat4;
use gravography_physics::ray::escape_direction;
use gravography_physics::{RayStatus, TraceParams, camera_ray_momentum, trace_ray};
use gravography_sky::colour::{mat3_vec, xyz_to_linear_srgb_matrix};
use rayon::prelude::*;

use crate::frame::FrameParams;
use crate::sky::Sky;
use crate::{HdrImage, RenderError};

/// CPU renderer settings.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct CpuSettings {
    /// Ray-tracing parameters (default: [`TraceParams::cpu_reference`], `rtol = 1e-10`).
    pub trace: TraceParams<f64>,
    /// Offset `ε` of the differential rays, in pixels. Small enough to be local (the Jacobian varies on the scale
    /// of a pixel near the critical curve), large enough that integration noise (`~rtol`) is negligible against
    /// the ray separation (`ε` × pixel angle ≳ 1e-5 rad for typical fields of view).
    pub differential_offset: f64,
}

impl Default for CpuSettings {
    fn default() -> Self {
        Self {
            trace: TraceParams::cpu_reference(),
            differential_offset: 0.25,
        }
    }
}

/// Outcome of casting one camera ray.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hit {
    /// Classification (always `Escaped` with lensing off).
    pub status: RayStatus,
    /// Unit escape direction (KS Cartesian axes), for escaped rays.
    pub dir: Option<[f64; 3]>,
    /// Frequency shift `g = 1/p_t` of a source at rest at infinity (`docs/PRD.md` §5.7).
    pub g: f64,
    /// Integration steps taken (accepted + rejected).
    pub steps: u32,
}

/// Per-image diagnostics.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RenderStats {
    /// Primary samples per status: escaped, captured, singular, max steps, error.
    pub status: [u64; 5],
    /// Escaped samples whose `g T` was outside the LUT range (colour clamped).
    pub lut_clamped: u64,
    /// Escaped samples where a differential ray did not escape on one axis (isotropic footprint from the other).
    pub footprint_partial: u64,
    /// Escaped samples where no differential ray escaped (unlensed pixel footprint used).
    pub footprint_fallback: u64,
    /// Rays cast (primary and differential).
    pub rays: u64,
    /// Integration steps over all rays (primary and differential).
    pub steps: u64,
    /// Largest step count of a single ray.
    pub max_steps: u32,
}

impl RenderStats {
    fn add(&mut self, o: &Self) {
        for k in 0..5 {
            self.status[k] += o.status[k];
        }
        self.lut_clamped += o.lut_clamped;
        self.footprint_partial += o.footprint_partial;
        self.footprint_fallback += o.footprint_fallback;
        self.rays += o.rays;
        self.steps += o.steps;
        self.max_steps = self.max_steps.max(o.max_steps);
    }

    fn count_ray(&mut self, hit: &Hit) {
        self.rays += 1;
        self.steps += u64::from(hit.steps);
        self.max_steps = self.max_steps.max(hit.steps);
    }

    /// Total primary samples.
    pub fn samples(&self) -> u64 {
        self.status.iter().sum()
    }
}

/// Index of a status in [`RenderStats::status`].
pub fn status_index(s: RayStatus) -> usize {
    match s {
        RayStatus::Escaped => 0,
        RayStatus::Captured => 1,
        RayStatus::Singular => 2,
        RayStatus::MaxSteps => 3,
        RayStatus::Error => 4,
    }
}

/// Inverse of [`status_index`] (`None` for codes ≥ 5). Used for GPU status codes.
pub fn status_from_index(i: u32) -> Option<RayStatus> {
    Some(match i {
        0 => RayStatus::Escaped,
        1 => RayStatus::Captured,
        2 => RayStatus::Singular,
        3 => RayStatus::MaxSteps,
        4 => RayStatus::Error,
        _ => return None,
    })
}

/// Sub-pixel offset of sample `s`: the R2 low-discrepancy sequence (M. Roberts, "The unreasonable effectiveness
/// of quasirandom sequences", 2018) started at the pixel centre, `frac(½ + s (1/φ₂, 1/φ₂²))` with `φ₂` the
/// plastic number. Sample 0 is the centre, so one sample per pixel samples centres.
pub fn sample_offset(s: u32) -> (f64, f64) {
    const PHI2: f64 = 1.324_717_957_244_746;
    let (a1, a2) = (1.0 / PHI2, 1.0 / (PHI2 * PHI2));
    let s = f64::from(s);
    ((0.5 + s * a1).fract(), (0.5 + s * a2).fract())
}

/// Reference renderer for one frame.
pub struct CpuRenderer<'a> {
    frame: &'a FrameParams,
    sky: &'a Sky,
    settings: CpuSettings,
    g_lo: Mat4<f64>,
}

impl<'a> CpuRenderer<'a> {
    /// Prepares a frame.
    ///
    /// # Errors
    /// [`RenderError::InvalidScene`] if the frame parameters are invalid.
    pub fn new(frame: &'a FrameParams, sky: &'a Sky, settings: CpuSettings) -> Result<Self, RenderError> {
        frame.validate()?;
        Ok(Self {
            frame,
            sky,
            settings,
            g_lo: frame.kerr.g_lo(&frame.position),
        })
    }

    /// Casts the ray with camera-frame unit direction `n` (forward, left, up).
    pub fn cast(&self, n: &[f64; 3]) -> Hit {
        let f = self.frame;
        let p0 = camera_ray_momentum(&self.g_lo, &f.tetrad, n);
        let g = if f.effects.frequency_shift {
            1.0 / p0[0]
        } else {
            1.0
        };
        if !f.effects.lensing {
            // Straight ray along the spatial part of k = −e_(0) + nⁱ e_(i): keeps aberration, removes bending.
            let k = f.tetrad.to_coordinate(&[-1.0, n[0], n[1], n[2]]);
            let ok = g.is_finite() && g > 0.0;
            return Hit {
                status: if ok { RayStatus::Escaped } else { RayStatus::Error },
                dir: ok.then(|| escape_direction(&k)),
                g,
                steps: 0,
            };
        }
        let r = trace_ray(&f.kerr, &f.position, &p0, &self.settings.trace);
        Hit {
            status: r.status,
            dir: r.escape_dir,
            g,
            steps: r.steps + r.rejected,
        }
    }

    /// Casts the ray through continuous pixel coordinate `(x, y)`.
    pub fn cast_pixel(&self, x: f64, y: f64) -> Hit {
        let f = self.frame;
        self.cast(&f.projection.direction(f.width, f.height, x, y))
    }

    /// Sky solid angle covered by one pixel (sr per pixel²) around `(x, y)`, whose primary ray escaped in
    /// direction `d`, by ray differentials. Returns the solid angle and how many axes fell back (0, 1 or 2).
    /// `stats` receives the step counts of the differential rays.
    pub fn footprint(&self, x: f64, y: f64, d: &[f64; 3], stats: &mut RenderStats) -> (f64, u8) {
        let eps = self.settings.differential_offset;
        let mut axis = |dx: f64, dy: f64| -> Option<[f64; 3]> {
            for sign in [1.0, -1.0] {
                let h = self.cast_pixel(x + sign * eps * dx, y + sign * eps * dy);
                stats.count_ray(&h);
                if let Some(e) = h.dir {
                    return Some(core::array::from_fn(|i| (e[i] - d[i]) / (sign * eps)));
                }
            }
            None
        };
        let (jx, jy) = (axis(1.0, 0.0), axis(0.0, 1.0));
        let len2 = |v: &[f64; 3]| v[0] * v[0] + v[1] * v[1] + v[2] * v[2];
        match (jx, jy) {
            (Some(a), Some(b)) => (len2(&cross(&a, &b)).sqrt(), 0),
            (Some(a), None) | (None, Some(a)) => (len2(&a), 1),
            (None, None) => {
                let f = self.frame;
                (f.projection.pixel_solid_angle(f.width, f.height, x, y), 2)
            }
        }
    }

    /// Shades one sample at `(x, y)`; returns linear sRGB.
    fn sample(&self, x: f64, y: f64, stats: &mut RenderStats) -> [f64; 3] {
        let hit = self.cast_pixel(x, y);
        stats.count_ray(&hit);
        stats.status[status_index(hit.status)] += 1;
        let Some(d) = hit.dir else {
            return [0.0; 3];
        };
        let (pixel_sr, fallback) = self.footprint(x, y, &d, stats);
        match fallback {
            1 => stats.footprint_partial += 1,
            2 => stats.footprint_fallback += 1,
            _ => {}
        }
        // Each of the N samples represents 1/N of the pixel area.
        let sr = pixel_sr / f64::from(self.frame.samples);
        let s = self.sky.shade(&d, sr, hit.g, &self.frame.effects);
        stats.lut_clamped += u64::from(s.lut_clamped);
        mat3_vec(&xyz_to_linear_srgb_matrix(), &s.xyz)
    }

    /// Renders one row.
    fn row(&self, j: u32, out: &mut [[f32; 3]]) -> RenderStats {
        let mut stats = RenderStats::default();
        let n = self.frame.samples;
        for (i, px) in out.iter_mut().enumerate() {
            let mut acc = [0.0_f64; 3];
            for s in 0..n {
                let (ox, oy) = sample_offset(s);
                let c = self.sample(i as f64 + ox, f64::from(j) + oy, &mut stats);
                for k in 0..3 {
                    acc[k] += c[k];
                }
            }
            *px = acc.map(|c| (c / f64::from(n)) as f32);
        }
        stats
    }

    /// Renders the frame (rows in parallel).
    pub fn render(&self) -> (HdrImage, RenderStats) {
        let (w, h) = (self.frame.width as usize, self.frame.height as usize);
        let mut pixels = vec![[0.0_f32; 3]; w * h];
        let rows: Vec<RenderStats> = pixels
            .par_chunks_mut(w)
            .enumerate()
            .map(|(j, row)| self.row(j as u32, row))
            .collect();
        let mut stats = RenderStats::default();
        for r in &rows {
            stats.add(r);
        }
        (
            HdrImage {
                width: self.frame.width,
                height: self.frame.height,
                pixels,
            },
            stats,
        )
    }
}

fn cross(a: &[f64; 3], b: &[f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

/// Renders `frame` with default [`CpuSettings`] (convenience).
///
/// # Errors
/// [`RenderError::InvalidScene`] if the frame parameters are invalid.
pub fn render(frame: &FrameParams, sky: &Sky) -> Result<(HdrImage, RenderStats), RenderError> {
    Ok(CpuRenderer::new(frame, sky, CpuSettings::default())?.render())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::camera::Projection;
    use crate::frame::Effects;
    use crate::tonemap::{Exposure, ToneMap};
    use approx::assert_relative_eq;
    use gravography_physics::{Kerr, Tetrad};
    use gravography_sky::cubemap::CubeLevel;
    use gravography_sky::{BlackbodyLut, CubeMap};

    fn uniform_sky() -> Sky {
        let mut base = CubeLevel::zeros(8);
        base.data.fill([0.5, 0.5 * 5000.0]);
        Sky::new(CubeMap::with_mips(base), BlackbodyLut::human_vision())
    }

    /// Static observer at distance `r` on the +x axis looking at the hole (forward −x, left −y, up +z).
    fn frame_at(r: f64, width: u32, height: u32, fov_deg: f64, effects: Effects) -> FrameParams {
        let kerr = Kerr::schwarzschild();
        let position = [0.0, r, 0.0, 0.0];
        let g = kerr.g_lo(&position);
        let seeds = [
            [1.0, 0.0, 0.0, 0.0],
            [0.0, -1.0, 0.0, 0.0],
            [0.0, 0.0, -1.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        ];
        FrameParams {
            kerr,
            position,
            tetrad: Tetrad::gram_schmidt(&g, &seeds).unwrap(),
            width,
            height,
            projection: Projection::Rectilinear { fov_deg },
            samples: 1,
            effects,
            exposure: Exposure::default(),
            tone_map: ToneMap::default(),
        }
    }

    #[test]
    fn status_codes_round_trip() {
        for s in [
            RayStatus::Escaped,
            RayStatus::Captured,
            RayStatus::Singular,
            RayStatus::MaxSteps,
            RayStatus::Error,
        ] {
            assert_eq!(status_from_index(status_index(s) as u32), Some(s));
        }
        assert_eq!(status_from_index(5), None);
    }

    #[test]
    fn sample_offsets() {
        // Sample 0 is the pixel centre; offsets stay inside the pixel and are distinct (low discrepancy).
        assert_eq!(sample_offset(0), (0.5, 0.5));
        let pts: Vec<_> = (0..64).map(sample_offset).collect();
        for (i, p) in pts.iter().enumerate() {
            assert!((0.0..1.0).contains(&p.0) && (0.0..1.0).contains(&p.1));
            for q in &pts[..i] {
                assert!((p.0 - q.0).abs() + (p.1 - q.1).abs() > 1e-3);
            }
        }
    }

    #[test]
    fn unlensed_footprint_is_the_pixel_solid_angle() {
        // Lensing off, static observer at r = 1e7 (metric = η to ~2M/r = 2e-7): the escape direction is the
        // pinhole direction, so the ray-differential footprint must equal the analytic dΩ = Δ² cos³θ. Forward
        // differences with ε = 0.25 px have relative error ~ε · (pixel angle) ≈ 0.25 · 2e-3 = 5e-4; allow 1e-3.
        let effects = Effects {
            lensing: false,
            ..Effects::default()
        };
        let frame = frame_at(1e7, 640, 480, 60.0, effects);
        let sky = uniform_sky();
        let r = CpuRenderer::new(&frame, &sky, CpuSettings::default()).unwrap();
        for (x, y) in [(320.5, 240.5), (10.5, 20.5), (600.5, 470.5)] {
            let hit = r.cast_pixel(x, y);
            assert_eq!(hit.status, RayStatus::Escaped);
            // Static observer: g = (1 − 2/r)^{-1/2} (PRD §9 test 10).
            assert_relative_eq!(hit.g, (1.0 - 2.0 / 1e7_f64).powf(-0.5), epsilon = 1e-12);
            let (sr, fb) = r.footprint(x, y, &hit.dir.unwrap(), &mut RenderStats::default());
            assert_eq!(fb, 0);
            let exact = frame.projection.pixel_solid_angle(640, 480, x, y);
            assert_relative_eq!(sr, exact, max_relative = 1e-3);
        }
    }

    #[test]
    fn small_schwarzschild_render() {
        // Static observer at r = 30, 40° field: the shadow (angular radius asin(3√3 √(1 − 2/r)/r) ≈ 9.6°) covers
        // the centre; the corners see the sky. A uniform sky stays uniform outside the shadow (surface brightness
        // conservation, Liouville) with the uniform blueshift g = (1 − 2/30)^{-1/2} (PRD §9 test 10).
        let frame = frame_at(30.0, 24, 16, 40.0, Effects::default());
        let sky = uniform_sky();
        let (img, stats) = render(&frame, &sky).unwrap();
        assert_eq!(stats.samples(), 24 * 16);
        assert!(stats.status[status_index(RayStatus::Captured)] > 0);
        assert_eq!(stats.status[status_index(RayStatus::Error)], 0);
        assert_eq!(img.pixel(12, 8), [0.0; 3]);
        let g = (1.0 - 2.0 / 30.0_f64).powf(-0.5);
        let xyz = sky.lut.lookup(5000.0 * g).response.map(|c| 0.5 * c);
        let expect = mat3_vec(&xyz_to_linear_srgb_matrix(), &xyz);
        for (i, j) in [(0, 0), (23, 0), (0, 15), (23, 15)] {
            let p = img.pixel(i, j);
            for k in 0..3 {
                // f32 storage (≈ 6e-8) and f32 cube texels; 1e-5 relative is ample.
                assert_relative_eq!(f64::from(p[k]), expect[k], max_relative = 1e-5);
            }
        }
        // Determinism (NF-3): a second render is bit-identical.
        let (again, stats2) = render(&frame, &sky).unwrap();
        assert_eq!(again.pixels, img.pixels);
        assert_eq!(stats, stats2);
    }
}
