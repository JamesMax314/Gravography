//! Kerr / Schwarzschild spacetime physics for Gravography.
//!
//! Pure CPU code with no GPU dependencies (`docs/PRD.md` §4.2). Conventions (normative, `docs/PRD.md` §2):
//! units `G = c = M = 1`, signature `(−, +, +, +)`, ingoing Cartesian Kerr–Schild coordinates `(t, x, y, z)`
//! with index 0 = `t`, rays evolved with covariant momentum `p_μ` and traced backward in time.
//!
//! All functions are generic over [`Real`] (`f64` for the ground truth, `f32` to reproduce GPU precision).
//! Derivations are in `docs/physics.md`.

pub mod boyer_lindquist;
pub mod error;
pub mod kerr;
pub mod kerr_schild;
pub mod linalg;
pub mod scalar;

pub use error::PhysicsError;
pub use kerr::Kerr;
pub use kerr_schild::{KsFields, ks_fields, ks_g_lo, ks_g_up, ks_radius};
pub use linalg::{Mat4, Vec4};
pub use scalar::Real;
