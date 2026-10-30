//! Sky shading shared by the backends (`docs/PRD.md` §6.3; `docs/physics.md` § "Rendering").
//!
//! A ray that escapes in direction `d` with frequency shift `g`, whose sky footprint covers `Ω` steradians,
//! sees the cube-map average `(w, w·T)` over that footprint and receives `XYZ = w · response(g · T)` with
//! `T = (w·T)/w`. The footprint picks the mip level, `lod = ½ log₂(Ω / Ω_texel(d))`.

use gravography_sky::cubemap::dir_to_face_uv;
use gravography_sky::{BlackbodyLut, CubeMap};

use crate::frame::Effects;

/// Effective photospheric temperature of the Sun (K; IAU 2015 Resolution B3 nominal value), used as the manual
/// exposure reference.
pub const SOLAR_TEMPERATURE: f64 = 5772.0;

/// The baked sky and the band response table.
#[derive(Clone, Debug)]
pub struct Sky {
    /// Mip-mapped `(w, w·T)` cube map.
    pub cube: CubeMap,
    /// Band response table (human vision: `T ↦ XYZ`).
    pub lut: BlackbodyLut,
}

/// Radiance of one sky sample.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SkySample {
    /// Band response (human vision: CIE `XYZ`).
    pub xyz: [f64; 3],
    /// `g · T` fell outside the LUT range and was clamped.
    pub lut_clamped: bool,
}

impl Sky {
    /// Wraps a baked cube map and a LUT.
    pub fn new(cube: CubeMap, lut: BlackbodyLut) -> Self {
        Self { cube, lut }
    }

    /// Luminance of a blackbody at the band's reference temperature filling the pixel (`w = 1`, `g = 1`): the
    /// manual-exposure reference ([`crate::tonemap::Exposure::scale`]). Human vision: `Y` at [`SOLAR_TEMPERATURE`].
    pub fn reference_luminance(&self) -> f64 {
        self.lut.reference_luminance()
    }

    /// Converts a band response to linear sRGB with the band's display mapping (human vision: XYZ → sRGB).
    pub fn to_rgb(&self, response: &[f64; 3]) -> [f64; 3] {
        gravography_sky::colour::mat3_vec(&self.lut.display, response)
    }

    /// Solid angle of a level-0 texel at unit direction `d`: on a face with coordinates `s, t ∈ [−1, 1]`,
    /// `dΩ = ds dt / (1 + s² + t²)^{3/2} = ds dt · |d_major|³`, and a texel spans `ds = dt = 2/N`.
    pub fn texel_solid_angle(&self, d: &[f64; 3]) -> f64 {
        let major = d[0].abs().max(d[1].abs()).max(d[2].abs());
        let pitch = 2.0 / self.cube.base_size() as f64;
        pitch * pitch * major * major * major
    }

    /// Level of detail for a footprint of `solid_angle` steradians around `d`: `½ log₂(Ω / Ω_texel(d))`, so one
    /// texel of the chosen level covers about the footprint (isotropic, area-preserving choice).
    pub fn lod(&self, d: &[f64; 3], solid_angle: f64) -> f64 {
        0.5 * (solid_angle / self.texel_solid_angle(d)).log2()
    }

    /// Radiance seen along escape direction `d` with frequency shift `g` and footprint `solid_angle` (sr).
    pub fn shade(&self, d: &[f64; 3], solid_angle: f64, g: f64, effects: &Effects) -> SkySample {
        let [w, wt] = self.cube.sample_trilinear(d, self.lod(d, solid_angle));
        if w.is_nan() || w <= 0.0 {
            return SkySample {
                xyz: [0.0; 3],
                lut_clamped: false,
            };
        }
        let g = if effects.frequency_shift { g } else { 1.0 };
        let s = self.lut.lookup(g * wt / w);
        let scale = if effects.beaming { w } else { w / g.powi(4) };
        SkySample {
            xyz: s.response.map(|c| c * scale),
            lut_clamped: s.clamped,
        }
    }
}

/// Face of `d` (exposed for tests and debug views).
pub fn face_of(d: &[f64; 3]) -> usize {
    dir_to_face_uv(d).0
}

#[cfg(test)]
mod tests {
    use super::*;
    use approx::assert_relative_eq;
    use gravography_sky::cubemap::{CubeLevel, face_uv_to_dir, texel_solid_angle};

    fn uniform_sky(size: usize, w: f32, t: f32) -> Sky {
        let mut base = CubeLevel::zeros(size);
        base.data.fill([w, w * t]);
        Sky::new(CubeMap::with_mips(base), BlackbodyLut::human_vision())
    }

    #[test]
    fn texel_solid_angle_matches_exact_at_centres() {
        // Midpoint approximation of the exact texel solid angle; relative error O((2/N)²) ≈ 4e-4 at N = 64
        // (curvature of 1/(1+s²+t²)^{3/2} across a texel).
        let sky = uniform_sky(64, 1.0, 5000.0);
        for (c, r) in [(0, 0), (31, 31), (5, 50), (63, 10)] {
            for face in 0..6 {
                let d = face_uv_to_dir(face, (c as f64 + 0.5) / 64.0, (r as f64 + 0.5) / 64.0);
                assert_eq!(face_of(&d), face);
                assert_relative_eq!(
                    sky.texel_solid_angle(&d),
                    texel_solid_angle(64, c, r),
                    max_relative = 1e-3
                );
            }
        }
    }

    #[test]
    fn uniform_sky_shading_and_toggles() {
        // Uniform (w, T) sky: any direction and footprint returns w · XYZ(gT) (the blackbody of temperature T
        // seen with shift g is a blackbody of temperature gT, docs/PRD.md §5.7). Without beaming the bolometric
        // brightness (∝ T⁴ for a blackbody) is restored to the g = 1 value: XYZ / g⁴.
        let sky = uniform_sky(16, 0.25, 6000.0);
        let on = Effects::default();
        let d = [0.3, -0.5, 0.8].map(|c: f64| c / (0.98_f64).sqrt());
        for lod_sa in [1e-8, 1e-2, 10.0] {
            let s = sky.shade(&d, lod_sa, 1.5, &on);
            let expect = sky.lut.lookup(9000.0).response.map(|c| 0.25 * c);
            for k in 0..3 {
                // f32 storage of w·T and w: relative error ≲ 1e-7 in T, amplified by d ln XYZ / d ln T ≲ 10.
                assert_relative_eq!(s.xyz[k], expect[k], max_relative = 1e-5);
            }
        }
        let no_shift = sky.shade(
            &d,
            1e-3,
            1.5,
            &Effects {
                frequency_shift: false,
                ..on
            },
        );
        let base = sky.lut.lookup(6000.0).response.map(|c| 0.25 * c);
        assert_relative_eq!(no_shift.xyz[1], base[1], max_relative = 1e-5);
        let no_beam = sky.shade(&d, 1e-3, 1.5, &Effects { beaming: false, ..on });
        let expect = sky
            .lut
            .lookup(9000.0)
            .response
            .map(|c| 0.25 * c / 1.5_f64.powi(4));
        assert_relative_eq!(no_beam.xyz[1], expect[1], max_relative = 1e-5);
        assert!(!no_beam.lut_clamped);
        assert!(sky.shade(&d, 1e-3, 1e5, &on).lut_clamped);
    }

    #[test]
    fn lod_selects_matching_level() {
        // A footprint of 4^k texels selects level k (each level halves the face size).
        let sky = uniform_sky(64, 1.0, 5000.0);
        let d = [0.0, 0.0, 1.0];
        let t = sky.texel_solid_angle(&d);
        for k in 0..4 {
            assert_relative_eq!(sky.lod(&d, t * 4f64.powi(k)), f64::from(k), epsilon = 1e-12);
        }
    }

    #[test]
    fn empty_texels_are_black() {
        let sky = uniform_sky(4, 0.0, 0.0);
        let s = sky.shade(&[1.0, 0.0, 0.0], 1e-3, 1.0, &Effects::default());
        assert_eq!(s.xyz, [0.0; 3]);
    }

    #[test]
    fn reference_luminance_is_solar_blackbody() {
        let sky = uniform_sky(1, 0.0, 0.0);
        assert_relative_eq!(sky.reference_luminance(), sky.lut.lookup(5772.0).response[1]);
        assert!(sky.reference_luminance() > 0.0);
    }
}
