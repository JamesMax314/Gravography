//! Error type of the physics crate.

use thiserror::Error;

/// Errors returned by the physics crate.
#[derive(Debug, Error, Clone, PartialEq)]
pub enum PhysicsError {
    /// Spacetime parameters outside the supported range (`M > 0`, `|a| < M`).
    #[error("invalid Kerr parameters (M = {mass}, a = {spin}): {reason}")]
    InvalidParameters {
        /// Mass `M` that was requested.
        mass: f64,
        /// Spin parameter `a` that was requested.
        spin: f64,
        /// Human-readable explanation.
        reason: &'static str,
    },
    /// A coordinate transform was evaluated where its Jacobian is singular
    /// (e.g. on the spin axis or a horizon in Boyer–Lindquist coordinates).
    #[error("coordinate transform is singular at this point: {0}")]
    CoordinateSingularity(&'static str),
    /// No ray exists with the requested parameters at the requested place.
    #[error("no such ray: {0}")]
    NoSuchRay(&'static str),
}
