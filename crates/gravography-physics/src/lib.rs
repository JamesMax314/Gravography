//! Kerr / Schwarzschild spacetime physics for Gravography.
//!
//! Pure CPU code with no GPU dependencies (`docs/PRD.md` §4.2). Conventions (normative, `docs/PRD.md` §2):
//! units `G = c = M = 1`, signature `(−, +, +, +)`, ingoing Cartesian Kerr–Schild coordinates `(t, x, y, z)`
//! with index 0 = `t`, rays evolved with covariant momentum `p_μ` and traced backward in time.
//!
//! All functions are generic over [`Real`] (`f64` for the ground truth, `f32` to reproduce GPU precision).
//! Derivations are in `docs/physics.md`.

pub mod boyer_lindquist;
pub mod connection;
pub mod distant;
pub mod error;
pub mod geodesic;
pub mod invariants;
pub mod kerr;
pub mod kerr_schild;
pub mod linalg;
pub mod ode;
pub mod ray;
pub mod scalar;
#[cfg(test)]
mod test_util;
pub mod tetrad;

pub use connection::{Christoffel, ks_christoffel};
pub use error::PhysicsError;
pub use geodesic::{ks_hamiltonian, ks_hamiltonian_rhs};
pub use kerr::Kerr;
pub use kerr_schild::{
    KsFieldDerivs, KsFields, MetricDerivs, ks_dg_lo, ks_dg_up, ks_field_derivatives, ks_fields, ks_g_lo,
    ks_g_up, ks_radius,
};
pub use linalg::{Mat4, Vec4};
pub use ray::{RayResult, RayStatus, TraceParams, camera_ray_momentum, sky_frequency_shift, trace_ray};
pub use scalar::Real;
pub use tetrad::Tetrad;
