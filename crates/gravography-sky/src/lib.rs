//! Sky model for Gravography: CIE colour matching, blackbody spectra, procedural star field and cube-map
//! baking (`docs/PRD.md` §6; derivations in `docs/physics.md` § "Colour pipeline and sky").
//!
//! The sky is a set of blackbody sources at rest at infinity. A baked cube map stores, per texel, the pair
//! `(w, w·T)`: `w` is the solid-angle filling factor (specific intensity `I_ν = w B_ν(T)`), and `T` the
//! (flux-weighted) temperature. A ray with frequency shift `g` sees `w · response(g T)` ([`BlackbodyLut`]).

pub mod bake;
pub mod band;
pub mod cache;
pub mod catalogue;
pub mod cmf;
pub mod colour;
pub mod cubemap;
pub mod diffuse;
pub mod error;
pub mod planck;
pub mod rng;

pub use bake::{SkyParams, bake, bake_sky};
pub use band::{Band, BlackbodyLut, LutSample};
pub use cache::bake_sky_cached;
pub use catalogue::{Star, StarParams};
pub use cmf::Cmf;
pub use cubemap::{CubeLevel, CubeMap};
pub use diffuse::DiffuseBandParams;
pub use error::SkyError;
