//! Sky model for Gravography: CIE colour matching, blackbody spectra, procedural star field and cube-map
//! baking (`docs/PRD.md` §6; derivations in `docs/physics.md` § "Colour pipeline and sky").
//!
//! The sky is a set of blackbody sources at rest at infinity. A baked cube map stores, per texel, the pair
//! `(w, w·T)`: `w` is the solid-angle filling factor (specific intensity `I_ν = w B_ν(T)`), and `T` the
//! (flux-weighted) temperature. A ray with frequency shift `g` sees `w · response(g T)` ([`BlackbodyLut`]).

pub mod band;
pub mod cmf;
pub mod colour;
pub mod error;
pub mod planck;
pub mod rng;

pub use band::{Band, BlackbodyLut, LutSample};
pub use cmf::Cmf;
pub use error::SkyError;
