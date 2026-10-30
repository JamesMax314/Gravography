//! GPU ↔ CPU parity harness (`docs/PRD.md` §9 tests 15–16; skeleton in WP-07).
//!
//! - [`parity_cameras`]: a fixed set of cameras (Schwarzschild and Kerr `a ∈ {0.5, 0.9, 0.998}`, outside and
//!   inside the horizon, moving observers).
//! - [`camera_rays`]: a grid of camera rays `(x, p)` in f64, convertible to f32 for the GPU and the CPU f32 path.
//! - [`RayOutcome`] and [`compare`]: status agreement and escape-direction angular errors between two tracers.
//!
//! Backend-independent: the GPU side converts its results into [`RayOutcome`]s.

use gravography_physics::linalg::Vec4;
use gravography_physics::{
    Kerr, Observer, ObserverParams, RayResult, RayStatus, Real, Tetrad, camera_ray_momentum,
};

use crate::camera::Projection;
use crate::cpu::{status_from_index, status_index};

/// A camera of the parity set.
#[derive(Clone, Debug)]
pub struct ParityCamera {
    /// Short identifier, e.g. `kerr0.9-zamo-r10`.
    pub name: String,
    /// The hole.
    pub kerr: Kerr<f64>,
    /// Camera event.
    pub position: Vec4<f64>,
    /// Camera tetrad (forward toward the hole unless noted).
    pub tetrad: Tetrad<f64>,
}

fn camera(name: &str, kerr: Kerr<f64>, obs: Observer<f64>, boost: [f64; 3]) -> ParityCamera {
    ParityCamera {
        name: name.into(),
        kerr,
        position: obs.pos,
        tetrad: obs
            .tetrad
            .boosted(&boost)
            .expect("parity camera boosts are below c"),
    }
}

/// The fixed parity camera set (PRD §9 test 15).
///
/// # Panics
/// Never for the built-in parameters (all presets are valid); a panic means a preset regressed.
pub fn parity_cameras() -> Vec<ParityCamera> {
    let p = ObserverParams::default();
    let deg = |d: f64| d.to_radians();
    let schw = Kerr::schwarzschild();
    let k05 = Kerr::new(0.5).expect("valid spin");
    let k09 = Kerr::new(0.9).expect("valid spin");
    let k0998 = Kerr::new(0.998).expect("valid spin");
    let ok = "parity camera presets are valid";
    vec![
        camera(
            "schw-hover-r30",
            schw,
            Observer::hover(&schw, 30.0, deg(90.0), 0.0, &p).expect(ok),
            [0.0; 3],
        ),
        camera(
            "schw-inside-r1.2",
            schw,
            Observer::radial_infall(&schw, 1.2, deg(90.0), 0.0, &p).expect(ok),
            [0.0; 3],
        ),
        camera(
            "schw-fast-r10-b0.9",
            schw,
            Observer::hover(&schw, 10.0, deg(90.0), 0.0, &p).expect(ok),
            [0.0, 0.9, 0.0],
        ),
        camera(
            "kerr0.5-hover-r15-th60",
            k05,
            Observer::hover(&k05, 15.0, deg(60.0), deg(30.0), &p).expect(ok),
            [0.0; 3],
        ),
        camera(
            "kerr0.5-inside-r1.2",
            k05,
            Observer::radial_infall(&k05, 1.2, deg(45.0), 0.0, &p).expect(ok),
            [0.0; 3],
        ),
        camera(
            "kerr0.9-zamo-r10",
            k09,
            Observer::zamo(&k09, 10.0, deg(90.0), 0.0, &p).expect(ok),
            [0.0; 3],
        ),
        camera(
            "kerr0.9-orbit-r4-pro",
            k09,
            Observer::circular_orbit(&k09, 4.0, true, 0.0, &p).expect(ok),
            [0.0; 3],
        ),
        camera(
            "kerr0.9-inside-r1.0",
            k09,
            Observer::radial_infall(&k09, 1.0, deg(60.0), 0.0, &p).expect(ok),
            [0.0; 3],
        ),
        camera(
            "kerr0.998-hover-r20",
            k0998,
            Observer::hover(&k0998, 20.0, deg(90.0), 0.0, &p).expect(ok),
            [0.0; 3],
        ),
        camera(
            "kerr0.998-inside-r1.02-axis",
            k0998,
            Observer::radial_infall(&k0998, 1.02, 0.0, 0.0, &p).expect(ok),
            [0.0; 3],
        ),
    ]
}

/// Initial states `(x, p)` of a `side × side` grid of pixel-centre rays through a rectilinear view of vertical
/// field of view `fov_deg` (f64; [`camera_ray_momentum`]).
pub fn camera_rays(cam: &ParityCamera, side: u32, fov_deg: f64) -> Vec<(Vec4<f64>, Vec4<f64>)> {
    let proj = Projection::Rectilinear { fov_deg };
    let g = cam.kerr.g_lo(&cam.position);
    let mut out = Vec::with_capacity((side * side) as usize);
    for j in 0..side {
        for i in 0..side {
            let n = proj.direction(side, side, f64::from(i) + 0.5, f64::from(j) + 0.5);
            out.push((cam.position, camera_ray_momentum(&g, &cam.tetrad, &n)));
        }
    }
    out
}

/// Rounds an f64 4-vector to f32 (the GPU input precision).
pub fn to_f32(v: &Vec4<f64>) -> Vec4<f32> {
    v.map(|c| c as f32)
}

/// Tracer-independent outcome of one ray.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct RayOutcome {
    /// Classification.
    pub status: RayStatus,
    /// Unit escape direction (escaped rays only).
    pub dir: Option<[f64; 3]>,
    /// Accepted steps.
    pub steps: u32,
}

impl<T: Real> From<&RayResult<T>> for RayOutcome {
    fn from(r: &RayResult<T>) -> Self {
        Self {
            status: r.status,
            dir: r.escape_dir.map(|d| d.map(Real::to_f64_lossy)),
            steps: r.steps,
        }
    }
}

impl RayOutcome {
    /// Outcome from a GPU status code, direction and step count; unknown codes map to `Error`.
    pub fn from_gpu(status: u32, dir: [f32; 4], steps: u32) -> Self {
        let status = status_from_index(status).unwrap_or(RayStatus::Error);
        Self {
            status,
            dir: (status == RayStatus::Escaped).then(|| [dir[0], dir[1], dir[2]].map(f64::from)),
            steps,
        }
    }
}

/// Angle between two unit vectors, `atan2(|a × b|, a · b)` (accurate for tiny and large angles).
pub fn angle_between(a: &[f64; 3], b: &[f64; 3]) -> f64 {
    let c = [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ];
    let cross = (c[0] * c[0] + c[1] * c[1] + c[2] * c[2]).sqrt();
    let dot = a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
    cross.atan2(dot)
}

/// Comparison of two tracers on the same rays.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Comparison {
    /// Number of rays.
    pub rays: usize,
    /// `confusion[i][j]`: rays with status index `i` in A and `j` in B.
    pub confusion: [[u64; 5]; 5],
    /// Angular errors (rad) of rays escaped in both, sorted ascending.
    pub angle_errors: Vec<f64>,
    /// Sum of `|steps_A − steps_B|` over all rays.
    pub step_difference: u64,
}

/// Compares outcomes `a` and `b` ray by ray.
///
/// # Panics
/// If the slices differ in length (they must describe the same rays).
pub fn compare(a: &[RayOutcome], b: &[RayOutcome]) -> Comparison {
    assert_eq!(a.len(), b.len(), "compared tracers must see the same rays");
    let mut c = Comparison {
        rays: a.len(),
        ..Comparison::default()
    };
    for (x, y) in a.iter().zip(b) {
        c.confusion[status_index(x.status)][status_index(y.status)] += 1;
        if let (Some(d1), Some(d2)) = (x.dir, y.dir) {
            c.angle_errors.push(angle_between(&d1, &d2));
        }
        c.step_difference += u64::from(x.steps.abs_diff(y.steps));
    }
    c.angle_errors.sort_by(f64::total_cmp);
    c
}

impl Comparison {
    /// Rays with the same status in both.
    pub fn status_agree(&self) -> u64 {
        (0..5).map(|i| self.confusion[i][i]).sum()
    }

    /// Fraction of rays with the same status (1 for an empty comparison).
    pub fn status_agreement(&self) -> f64 {
        if self.rays == 0 {
            return 1.0;
        }
        self.status_agree() as f64 / self.rays as f64
    }

    /// Angular-error quantile `q ∈ [0, 1]` (nearest rank) over rays escaped in both, or `None` if there are none.
    pub fn angle_quantile(&self, q: f64) -> Option<f64> {
        let n = self.angle_errors.len();
        if n == 0 {
            return None;
        }
        let idx = ((q.clamp(0.0, 1.0) * n as f64).ceil() as usize).clamp(1, n) - 1;
        Some(self.angle_errors[idx])
    }

    /// Fraction of both-escaped rays with angular error `≤ tol` (1 if there are none).
    pub fn fraction_within(&self, tol: f64) -> f64 {
        let n = self.angle_errors.len();
        if n == 0 {
            return 1.0;
        }
        self.angle_errors.partition_point(|&e| e <= tol) as f64 / n as f64
    }

    /// Mean `|Δsteps|` per ray.
    pub fn mean_step_difference(&self) -> f64 {
        self.step_difference as f64 / self.rays.max(1) as f64
    }

    /// Merges another comparison (e.g. per-camera into overall).
    pub fn merge(&mut self, o: &Comparison) {
        self.rays += o.rays;
        for i in 0..5 {
            for j in 0..5 {
                self.confusion[i][j] += o.confusion[i][j];
            }
        }
        self.angle_errors.extend_from_slice(&o.angle_errors);
        self.angle_errors.sort_by(f64::total_cmp);
        self.step_difference += o.step_difference;
    }

    /// One-line report: rays, status agreement, angular error median/p99/max, mean step difference.
    pub fn summary(&self) -> String {
        let q = |x: Option<f64>| x.map_or("—".to_string(), |v| format!("{v:.2e}"));
        format!(
            "{} rays, status {:.3}% ({} differ), angle p50 {} p99 {} max {} rad, |Δsteps| {:.2}",
            self.rays,
            100.0 * self.status_agreement(),
            self.rays as u64 - self.status_agree(),
            q(self.angle_quantile(0.5)),
            q(self.angle_quantile(0.99)),
            q(self.angle_quantile(1.0)),
            self.mean_step_difference()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_relative_eq;
    use gravography_physics::ray::Integrator;
    use gravography_physics::{TraceParams, trace_ray};

    #[test]
    fn angle_between_is_accurate() {
        // atan2 form: exact for tiny angles where acos(dot) would lose all precision.
        let t: f64 = 1e-9;
        let a = [1.0, 0.0, 0.0];
        let b = [t.cos(), t.sin(), 0.0];
        assert_relative_eq!(angle_between(&a, &b), t, max_relative = 1e-12);
        assert_relative_eq!(angle_between(&a, &[-1.0, 0.0, 0.0]), std::f64::consts::PI);
        assert_relative_eq!(angle_between(&a, &[0.0, 0.0, 1.0]), std::f64::consts::FRAC_PI_2);
    }

    #[test]
    fn comparison_statistics() {
        let esc = |x: f64, steps| RayOutcome {
            status: RayStatus::Escaped,
            dir: Some([x.cos(), x.sin(), 0.0]),
            steps,
        };
        let cap = RayOutcome {
            status: RayStatus::Captured,
            dir: None,
            steps: 5,
        };
        let a = [esc(0.0, 10), esc(0.0, 10), esc(0.0, 10), cap];
        let b = [esc(1e-3, 12), esc(2e-3, 10), cap, cap];
        let c = compare(&a, &b);
        assert_eq!(c.rays, 4);
        assert_eq!(c.status_agree(), 3);
        assert_eq!(c.confusion[0][1], 1);
        assert_relative_eq!(c.status_agreement(), 0.75);
        assert_eq!(c.angle_errors.len(), 2);
        assert_relative_eq!(c.angle_quantile(0.5).unwrap(), 1e-3, max_relative = 1e-9);
        assert_relative_eq!(c.angle_quantile(1.0).unwrap(), 2e-3, max_relative = 1e-9);
        assert_relative_eq!(c.fraction_within(1.5e-3), 0.5);
        assert_eq!(c.step_difference, 2 + 5);
        let mut m = c.clone();
        m.merge(&c);
        assert_eq!(m.rays, 8);
        assert_eq!(m.angle_errors.len(), 4);
        assert!(c.summary().contains("75.000%"));
        assert_eq!(compare(&[], &[]).angle_quantile(0.5), None);
    }

    #[test]
    fn gpu_outcome_conversion() {
        let o = RayOutcome::from_gpu(0, [0.0, 1.0, 0.0, 9.0], 3);
        assert_eq!(o.status, RayStatus::Escaped);
        assert_eq!(o.dir, Some([0.0, 1.0, 0.0]));
        assert_eq!(RayOutcome::from_gpu(1, [0.0; 4], 3).dir, None);
        assert_eq!(RayOutcome::from_gpu(99, [0.0; 4], 3).status, RayStatus::Error);
    }

    #[test]
    fn cameras_and_rays_are_valid() {
        // Every parity camera is a valid observer, its rays are null to f64 rounding (H ≈ 0 relative to |p|²),
        // and the CPU f32 vs f64 RK4 comparison runs (the GPU-free part of the harness).
        let cams = parity_cameras();
        assert_eq!(cams.len(), 10);
        let mut rk4 = TraceParams::<f64>::cpu_reference();
        rk4.integrator = Integrator::Rk4;
        let mut rk4_f32 = TraceParams::<f32>::cpu_reference();
        rk4_f32.integrator = Integrator::Rk4;
        for cam in &cams {
            let g = cam.kerr.g_lo(&cam.position);
            assert!(cam.tetrad.orthonormality_error(&g) < 1e-12, "{}", cam.name);
            let rays = camera_rays(cam, 4, 90.0);
            assert_eq!(rays.len(), 16);
            let k32 = Kerr::new(cam.kerr.spin() as f32).unwrap();
            let (mut a, mut b) = (Vec::new(), Vec::new());
            for (x, p) in &rays {
                let pn = p.iter().fold(0.0_f64, |m, c| m.max(c.abs()));
                assert!(cam.kerr.hamiltonian(x, p).abs() < 1e-12 * pn * pn, "{}", cam.name);
                a.push(RayOutcome::from(&trace_ray(&cam.kerr, x, p, &rk4)));
                b.push(RayOutcome::from(&trace_ray(
                    &k32,
                    &to_f32(x),
                    &to_f32(p),
                    &rk4_f32,
                )));
            }
            let c = compare(&a, &b);
            assert_eq!(c.rays, 16);
        }
    }
}
