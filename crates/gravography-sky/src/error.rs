//! Error type of the sky crate.

use thiserror::Error;

/// Errors returned by the sky crate.
#[derive(Debug, Error)]
pub enum SkyError {
    /// A bundled or supplied data table could not be parsed.
    #[error("invalid data table: {0}")]
    InvalidTable(String),
    /// Invalid parameters (e.g. a zero cube-map size or an empty temperature range).
    #[error("invalid parameters: {0}")]
    InvalidParameters(&'static str),
    /// Reading or writing the sky cache failed.
    #[error("sky cache I/O error: {0}")]
    Io(#[from] std::io::Error),
}
