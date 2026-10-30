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

/// Errors and stop conditions of the observer simulation (`docs/PRD.md` §5.8). The messages are meant for the
/// user interface.
#[derive(Debug, Error, Clone, PartialEq)]
pub enum ObserverError {
    /// The requested observer state or preset does not exist at this place (e.g. hovering inside the
    /// ergosphere, a circular orbit below the photon orbit).
    #[error("{0}")]
    InvalidState(&'static str),
    /// The observer came too close to the singularity (Schwarzschild) or the inner horizon (Kerr).
    #[error(
        "the observer reached r = {r:.4} M, below the safe limit r_min = {r_min:.4} M \
         (too close to the singularity or the inner horizon); the simulation stopped"
    )]
    InnerLimit {
        /// Radius reached (units of `M`).
        r: f64,
        /// Limit `r_min = r_− + margin` (units of `M`).
        r_min: f64,
    },
    /// The observer moved beyond the supported distance from the hole.
    #[error("the observer reached r = {r:.1} M, beyond the supported range r_max = {r_max:.1} M")]
    OuterLimit {
        /// Radius reached (units of `M`).
        r: f64,
        /// Limit (units of `M`).
        r_max: f64,
    },
    /// The worldline integration failed (non-finite values, degenerate tetrad, step budget exhausted).
    #[error("observer integration failed: {0}")]
    IntegrationFailure(&'static str),
}
