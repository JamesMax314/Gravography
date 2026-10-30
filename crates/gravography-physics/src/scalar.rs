//! Scalar abstraction: the physics is generic over the floating-point type so the same code runs in f64
//! (CPU ground truth), f32 (to reproduce GPU precision on the CPU) and later dual numbers
//! (`docs/PRD.md` §4.2, §5.9).

use core::fmt::Debug;

use num_traits::{Float, FloatConst};

/// Real scalar type used throughout the physics crate.
///
/// Implemented for `f32` and `f64`.
pub trait Real: Float + FloatConst + Debug + Send + Sync + 'static {
    /// Converts an `f64` literal to this type (rounding to nearest for `f32`).
    fn lit(v: f64) -> Self;

    /// Converts to `f64` (exact for `f32` and `f64`). Used for diagnostics and error messages.
    fn to_f64_lossy(self) -> f64;
}

impl Real for f64 {
    #[inline]
    fn lit(v: f64) -> Self {
        v
    }

    #[inline]
    fn to_f64_lossy(self) -> f64 {
        self
    }
}

impl Real for f32 {
    #[inline]
    fn lit(v: f64) -> Self {
        v as f32
    }

    #[inline]
    fn to_f64_lossy(self) -> f64 {
        f64::from(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_relative_eq;

    #[test]
    fn lit_round_trips() {
        assert_relative_eq!(f64::lit(0.1), 0.1_f64);
        assert_relative_eq!(f32::lit(0.1), 0.1_f32);
        assert_relative_eq!(f32::lit(0.1).to_f64_lossy(), f64::from(0.1_f32));
    }
}
