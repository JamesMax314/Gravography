//! Shared helpers for unit tests (compiled only under `cfg(test)`).

use proptest::prelude::*;

use crate::kerr_schild::ks_radius;
use crate::linalg::{Mat4, Vec4};

/// Largest absolute entry-wise difference between two matrices.
pub fn max_abs_diff(a: &Mat4<f64>, b: &Mat4<f64>) -> f64 {
    let mut d = 0.0_f64;
    for (ra, rb) in a.iter().zip(b) {
        for (x, y) in ra.iter().zip(rb) {
            d = d.max((x - y).abs());
        }
    }
    d
}

/// Largest absolute entry of a matrix.
pub fn max_abs(a: &Mat4<f64>) -> f64 {
    a.iter().flatten().fold(0.0_f64, |m, x| m.max(x.abs()))
}

/// Largest absolute component of a 4-vector.
pub fn max_abs4(v: &Vec4<f64>) -> f64 {
    v.iter().fold(0.0_f64, |m, x| m.max(x.abs()))
}

/// Random spin `a ∈ [0, 0.999)` and KS event `[t, x, y, z]` away from the ring singularity (`r > 0.3`),
/// with extra weight near the axis, the equatorial plane and the hole.
pub fn event() -> impl Strategy<Value = (f64, Vec4<f64>)> {
    let spin = 0.0..0.999_f64;
    let coord = -30.0..30.0_f64;
    let tiny = -1e-6..1e-6_f64;
    let generic = (coord.clone(), coord.clone(), coord.clone()).prop_map(|(x, y, z)| [x, y, z]);
    let near_axis = (tiny.clone(), tiny.clone(), coord.clone()).prop_map(|(x, y, z)| [x, y, z]);
    let near_equator = (coord.clone(), coord, tiny).prop_map(|(x, y, z)| [x, y, z]);
    let near_hole = (-3.0..3.0_f64, -3.0..3.0_f64, -3.0..3.0_f64).prop_map(|(x, y, z)| [x, y, z]);
    (
        spin,
        prop_oneof![generic, near_axis, near_equator, near_hole],
        -100.0..100.0_f64,
    )
        .prop_filter("stay away from the ring singularity", |(a, [x, y, z], _)| {
            ks_radius(*a, *x, *y, *z) > 0.3
        })
        .prop_map(|(a, [x, y, z], t)| (a, [t, x, y, z]))
}

/// Random covariant momentum with components in `[−2, 2]` (not necessarily null).
pub fn momentum() -> impl Strategy<Value = Vec4<f64>> {
    proptest::array::uniform4(-2.0..2.0_f64)
}

/// Derivative of `f` at `s = 0` by Richardson-extrapolated central differences:
/// `(4 D(h/2) − D(h)) / 3` with `D(h) = (f(h) − f(−h)) / 2h`. Truncation error `O(h⁴)`, rounding `O(ε/h)`.
pub fn richardson<F: Fn(f64) -> f64>(f: F, h: f64) -> f64 {
    let d = |h: f64| (f(h) - f(-h)) / (2.0 * h);
    (4.0 * d(0.5 * h) - d(h)) / 3.0
}
