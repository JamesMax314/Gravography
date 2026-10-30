//! Error type of the render crate.

use gravography_physics::{ObserverError, PhysicsError};
use gravography_sky::SkyError;

/// Errors from scene loading, frame setup, rendering and image output.
#[derive(Debug, thiserror::Error)]
pub enum RenderError {
    /// The scene file is not valid TOML or does not match the schema.
    #[error("scene file: {0}")]
    SceneParse(#[from] toml::de::Error),
    /// The scene could not be serialised.
    #[error("scene serialisation: {0}")]
    SceneWrite(#[from] toml::ser::Error),
    /// A scene value is out of range.
    #[error("invalid scene: {0}")]
    InvalidScene(String),
    /// Invalid spacetime parameters.
    #[error(transparent)]
    Physics(#[from] PhysicsError),
    /// The observer cannot be placed as requested.
    #[error("observer: {0}")]
    Observer(#[from] ObserverError),
    /// Sky baking failed.
    #[error("sky: {0}")]
    Sky(#[from] SkyError),
    /// Unsupported output format (the extension is not `.png` or `.exr`).
    #[error("unsupported output format {0:?} (use .png or .exr)")]
    OutputFormat(String),
    /// Image encoding failed.
    #[error("image output: {0}")]
    Image(#[from] image::ImageError),
    /// GPU device, shader or readback failure.
    #[error("GPU: {0}")]
    Gpu(String),
    /// File-system error.
    #[error("I/O: {0}")]
    Io(#[from] std::io::Error),
}
