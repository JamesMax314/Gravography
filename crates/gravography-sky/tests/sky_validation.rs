//! Sky validation at production size (`docs/PRD.md` §6, WP-05 acceptance).

use std::time::Instant;

use gravography_sky::{BlackbodyLut, SkyParams, bake_sky};

/// Full default bake (2048² faces, 2·10⁵ stars, diffuse band): the star flux plus the band is conserved through the
/// whole mip chain, and the timing is printed for `docs/progress_report.md`. Slow in debug builds (~270 MB of
/// texels), so ignored; run with `cargo test --release -p gravography-sky -- --ignored --nocapture`.
#[test]
#[ignore = "production-size bake: slow in debug builds"]
fn default_sky_bake_conserves_flux() {
    let lut = BlackbodyLut::human_vision();
    let params = SkyParams::default();
    let start = Instant::now();
    let (stars, cube) = bake_sky(&params, &lut).expect("default parameters are valid");
    let elapsed = start.elapsed();
    println!(
        "default bake: {} stars, {}² faces, {} levels in {elapsed:.2?}",
        stars.len(),
        cube.base_size(),
        cube.levels.len()
    );
    let base = cube.levels[0].totals();
    for l in &cube.levels {
        let t = l.totals();
        // f32 texels, ~25M of them summed in f64 per level; the mip averaging adds rounding per level. 1e-5 relative.
        assert!(
            (t[0] / base[0] - 1.0).abs() < 1e-5,
            "size {}: {:?} vs {:?}",
            l.size,
            t,
            base
        );
        assert!((t[1] / base[1] - 1.0).abs() < 1e-5);
    }
    let star_flux: f64 = stars.iter().map(|s| s.solid_angle).sum();
    // The band adds flux on top of the stars.
    assert!(base[0] > star_flux);
    println!(
        "Σ Ω_* = {star_flux:.4e} sr, Σ w Ω = {:.4e} sr (band {:.1} %)",
        base[0],
        100.0 * (1.0 - star_flux / base[0])
    );
}
