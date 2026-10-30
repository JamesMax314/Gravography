# Gravography — Progress Report

Hand-off log between agent sessions. Format and rules: `CLAUDE.md` §7. Newest log entry first.

---

## Current state

- WP-00 to WP-06 done.
  - `gravography-physics`: Kerr–Schild metric and derivatives, Hamiltonian ray tracing (DOPRI5/RK4, termination,
    escape direction, `g = 1/p_t`), BL ↔ KS, radii, invariants, observer dynamics, tetrad boosts (WP-06).
  - `gravography-sky`: CIE 1931 table, Planck spectra, blackbody LUT, XYZ → sRGB, deterministic star catalogue,
    diffuse band, flux-conserving mip-mapped `(w, w·T)` cube map, on-disk cache.
  - `gravography-render` (WP-06): TOML scene files, `FrameParams`, CPU f64 reference renderer (rayon,
    ray-differential footprints, cube-map shading, effect toggles), auto/manual exposure, ACES/Reinhard tone mapping,
    PNG/EXR output.
  - `gravography` binary: `gravography render --scene … --out …png|exr [--width --height --samples --backend
    --no-cache]`. Only the CPU backend exists; the interactive viewer is not built yet.
- PRD §9 tests 1–14 pass, as do FR-X3 (scene round trip) and a renderer lensing validation against the exact
  Schwarzschild lens map.
- **Next work package:** WP-07 — GPU foundation.
- Toolchain: rustup, Rust **1.99.1** (`rust-toolchain.toml`).
- The user approved shipping the CC BY-SA 4.0 CIE table (2026-10-30; see `THIRD_PARTY.md`).
- Open questions for the user:
  - The example scenes use auto exposure with −2 EV (−4 EV inside the horizon), chosen by eye. Adjust to taste.

---

## Work package status

| WP | Title | Status | Tag |
|---|---|---|---|
| WP-00 | Repository bootstrap | done | `wp-00` |
| WP-01 | Metric core | done | `wp-01` |
| WP-02 | Derivatives, Hamiltonian, invariants | done | `wp-02` |
| WP-03 | CPU ray tracing | done | `wp-03` |
| WP-04 | Observer | done | `wp-04` |
| WP-05 | Colour & sky | done | `wp-05` |
| WP-06 | CPU reference renderer + headless CLI | done | `wp-06` |
| WP-07 | GPU foundation | not started | |
| WP-08 | GPU ray tracer | not started | |
| WP-09 | GPU shading & headless GPU render | not started | |
| WP-10 | Viewer shell | not started | |
| WP-11 | Interactive flight | not started | |
| WP-12 | Performance | not started | |
| WP-13 | v1 polish & release | not started | |

---

## Log

<!--
Template for each entry (copy above the previous entry):

## WP-XX — <title> — <YYYY-MM-DD> — COMPLETE | INCOMPLETE

**Summary**
-

**Key files/modules**
-

**Decisions & deviations**
-

**Tests**
-

**Visual checks**
-

**Performance**
-

**Known issues / tech debt**
-

**Notes for the next agent**
-
-->

## WP-06 — CPU reference renderer + headless CLI — 2026-10-30 — COMPLETE

**Summary**
- `gravography-render`:
  - scene files (`Scene`, TOML via serde, `deny_unknown_fields`, every field defaulted except `[observer]`);
  - the backend-agnostic `FrameParams` (Kerr, camera event, tetrad with boost and look rotation, size,
    projection, samples, effects, exposure, tone map);
  - the f64 CPU reference renderer (`CpuRenderer`: rayon over rows, R2 jittered samples, ray-differential
    footprints, cube-map shading `w · XYZ(g T)`, §6.4 effect toggles);
  - auto/manual exposure, ACES-fitted and extended-Reinhard tone mapping, PNG (8-bit sRGB) and EXR (linear) output.
- `gravography-physics`: `Tetrad::boosted` (pure Lorentz boost in tetrad components), used for the fast-moving
  observer scene.
- `gravography` binary: `render` subcommand (clap). The sky is cached in `~/Library/Caches/gravography`.
  `--backend gpu` exits with a clear error until WP-09.
- Example scenes: `scenes/schwarzschild_far.toml`, `kerr_edge_on.toml` (a = 0.99), `inside_horizon.toml`,
  `fast_observer.toml`.
- README: CLI and full scene-schema documentation. `docs/physics.md` § "Rendering" covers projection, boosts,
  shading, ray differentials, exposure and tone mapping, and determinism.

**Key files/modules**
- `crates/gravography-render/src/scene.rs`: scene schema, validation, `Scene::{frame, sky_params, cpu_settings}`.
- `crates/gravography-render/src/frame.rs`: `FrameParams`, `Effects`.
- `crates/gravography-render/src/camera.rs`: `Projection::Rectilinear` (direction, pixel solid angle).
- `crates/gravography-render/src/cpu.rs`: `CpuRenderer` (`cast`, `cast_pixel`, `footprint`, `render`),
  `RenderStats`, `sample_offset`.
- `crates/gravography-render/src/sky.rs`: `Sky` (cube + LUT), texel solid angle, LOD, `shade`.
- `crates/gravography-render/src/tonemap.rs`: `Exposure`, `ToneMap`, ACES fitted, Reinhard, sRGB8.
- `crates/gravography-render/src/output.rs`: `HdrImage`, `write_image` (PNG/EXR by extension).
- `crates/gravography-render/src/lib.rs`: `display_scale` (exposure for an image).
- `crates/gravography-render/tests/scene_files.rs`: FR-X3 round trip, scenes parse/build/render, README coverage.
- `crates/gravography-render/tests/render_validation.rs`: footprint and escape direction vs the exact lens map.
- `crates/gravography-app/src/main.rs`: CLI.

**Decisions & deviations**
- **Footprint:** forward-difference ray differentials, 2 extra rays per sample, `ε = 0.25` px, retry at `−ε`, then
  isotropic or unlensed fallbacks (counted in the stats). The footprint is isotropic and preserves area
  (`lod = ½ log₂(Ω/Ω_texel(d))`). Anisotropic filtering is not attempted (documented in `physics.md`). With N
  samples, each sample uses 1/N of the pixel's footprint. WP-09 should use the same LOD formula with
  `textureSampleLevel`, not hardware `textureGrad` LOD, to match.
- **Lensing-off toggle:** escape direction = unit spatial part of `k = −e_(0) + nⁱ e_(i)` (aberration kept).
- **Error status:** PRD §5.6 asks for magenta in debug builds. The CPU renderer draws `Error` (and every
  non-escaped status) black in all builds and counts them in `RenderStats`, so debug and release images stay
  identical. The FR-R6 status debug view (WP-09) is where errors should be shown. No errors occurred in any scene.
- **Projection:** only rectilinear (vertical FOV) is implemented. `Projection` is a tagged enum, so WP-13 can add
  equirectangular (FR-R2) without a schema change.
- **Exposure:** auto = log-average luminance of the lit pixels (Y > 0) → 0.18, plus EV compensation. Manual is
  relative to a 5772 K blackbody with w = 1. EXR stores the exposed but not tone-mapped linear sRGB.
- **Scene observer:** a preset plus an optional `boost` (velocity relative to the preset, in its frame) and a
  `look` rotation (yaw, then pitch, then roll). Scenes do not advance the observer in time. Bookmarks (FR-O6,
  RON) are separate and come later.
- **CLI overrides:** `--width/--height/--samples` override the scene.
- New dependencies:
  - `serde` and `toml`: scene files (FR-X2/X3);
  - `image` (only the `png` and `exr` features): output;
  - `rayon`: parallel renderer;
  - `thiserror`: errors;
  - `clap` and `anyhow` (app): CLI;
  - `approx` (dev).
- PRD unchanged.

**Tests**
- `cargo test --workspace`: 177 passed, 1 ignored (the sky production bake, unchanged from WP-05 and not touched).
  New in this WP: 20 render unit tests, 5 render integration tests, 2 physics tests.
- **FR-X3:**
  - a scene with every field non-default round-trips through TOML;
  - every `scenes/*.toml` parses, round-trips, builds a frame and renders at thumbnail size with no
    errors/max-steps/singular rays;
  - every serialised key appears in backticks in README (this caught two undocumented keys during the WP).
- **Lensing validation:** static camera at d = 10⁴ in Schwarzschild against exact orbit quadrature:
  - escape direction error 7e-12 rad (tolerance 1e-8);
  - footprint / pixel solid angle vs the exact inverse magnification `(sin ψ/sin θ)|dψ/dθ|`: ≤ 6.8e-4 relative
    (tolerance 1e-3) at θ/θ_E ∈ {0.7, 0.9, 1.2, 1.5, 2.5}, including the parity-flipped secondary images.
- **Other unit tests:**
  - lensing-off footprint equals the analytic pinhole `Δ² cos³θ` (1e-3);
  - the pixel solid angles sum to the frustum (1e-5);
  - a uniform sky stays uniform with `g = (1 − 2/r)^{-1/2}` around a Schwarzschild shadow (1e-5);
  - the render is bit-identical across runs (NF-3);
  - effect toggles: g⁴ and LUT clamp;
  - LOD level selection;
  - ACES endpoints, grey neutrality and mid-grey value 0.105591; Reinhard white point and hue;
  - auto exposure; sRGB8 encoding; PNG/EXR round trip;
  - scene validation; look-rotation conventions; boost and look application;
  - boost Doppler `sqrt((1+β)/(1−β))` and orthonormality (proptest).

**Visual checks** (960×540, 4 spp, default sky; PNGs in `target/screenshots/`, not committed)
- `schwarzschild_far` (r = 50, 40° FOV):
  - shadow radius 78 px = 5.8° (predicted asin(3√3·sqrt(1 − 2/50)/50) = 5.8°), with a thin photon ring;
  - an Einstein ring of tangentially stretched stars at ≈ 18.5° (weak-field estimate 16°);
  - two secondary images of the galactic band hugging the shadow diagonally.
- `kerr_edge_on` (a = 0.99, r = 20, 45° FOV): D-shaped shadow, flattened on the left (the approaching side, where
  prograde photon orbits lie closest in) and displaced to the right; nested photon-ring arcs on the flat side;
  asymmetric lensing of the band.
- `inside_horizon` (radial infall, r = 1.2, 120° FOV):
  - black disc around the inward direction with radius 50.3° measured, against 50.5° predicted from the critical
    impact parameter for an E = 1 infaller (formula in the scene file);
  - a blue-white glowing rim, g = 5.5 probed just outside the edge (5.6 predicted);
  - the whole sky blueshifted (g ≈ 1.5–2 at the image edges, probed); all stars white.
- `fast_observer` (hover position at r = 10, boost 0.9 c tangential, looking along the motion, 90° FOV):
  - shadow spans 15.7°–41.6° right of centre, against 15.8°–41.6° predicted by aberrating the static-frame edges
    90° ± 27.7°;
  - g = 4.873 at the image centre, against γ(1+β)/sqrt(1−2/r) = 4.87;
  - the forward sky is a blue-white blaze; the corners are slightly reddened (g = 0.81 probed).
- I probed the g values with a throwaway example (not committed). The FR-R6 `g` debug view arrives in WP-09.

**Performance** (M-series Mac, release, rayon on all cores, rtol 1e-10, 960×540 × 4 spp = 2.07 M primary samples
plus 2 differential rays each):
- schwarzschild_far 43 s; kerr_edge_on 75 s; inside_horizon 53–64 s; fast_observer 35 s;
- mean 95–253 steps per ray, max 3876 (Kerr, near-critical);
- sky bake 1.8 s cold, 0.1 s from cache (268 MB file).

**Known issues / tech debt**
- **Auto exposure depends on resolution** for sparse star fields: at low resolution each pixel averages more sky,
  so the log-average rises. At 240×135 the scale was 20× lower than at 960×540 for `inside_horizon`. This is
  inherent to log-average metering of point sources. If it bothers users, consider a percentile or mean-luminance
  meter (WP-12/13); GPU parity (WP-09) should compare at equal resolution, or with manual exposure.
- **Star field is busy:** the default 2·10⁵ stars with α = 1.5 give a dense, mostly orange field. The scene files
  compensate with −2 EV. Sky defaults could be revisited in WP-13.
- **Texel-wide arcs:** near the Einstein ring and photon rings, stars are drawn as texel-width arcs (inherent to
  the 2048² cube map; documented). With 1 spp, a faint concentric moiré appears there; 4 spp mostly removes it.
- **No progress output:** the CPU renderer prints nothing during a long render.

**Notes for the next agent**
- **Matching the reference:** the GPU must reproduce `CpuRenderer::cast`, `CpuRenderer::footprint`, `Sky::shade`,
  `Sky::lod` and `tonemap::*` for parity (WP-08/09). Their structure is deliberately simple: port them by name.
  `RenderStats::status` uses the index order of `cpu::status_index`.
- **Camera setup:** `Scene::frame()` builds everything a backend needs. The tetrad already includes the boost and
  look rotation, and `FrameParams::validate` checks sizes.
- **Probing pixels:** `CpuRenderer::cast_pixel(x, y)` gives status, escape direction, g and steps. It is handy for
  parity harnesses and debugging.
- **Sky cache:** the cache file is keyed by `SkyParams`; tests use `bake_sky` directly (no cache).
- **Renders take time:** a full-size reference render takes ~1 min. For quick looks use
  `--width 480 --height 270 --samples 1` (~3–5 s).

## WP-05 — Colour & sky — 2026-10-30 — COMPLETE

**Summary**
- **Baseline fix first:** `main` was red at the start of the session.
  - `kerr::tests::isco_satisfies_marginal_stability` failed for a random proptest seed (a ≈ 1.3e-5, residual
    1.05e-10 > 1e-10).
  - Cause: `3 − Z₁` in the BPT ISCO formula cancels catastrophically as χ → 0.
  - Fix: compute it as `(p + m)(p − m)²`, with `p − m = 2χ/(p² + pm + m²)`.
  - Regression test `isco_is_accurate_for_tiny_spin`, written first; it reproduced the failure.
  - `.cargo/config.toml` now sets `PROPTEST_RNG_SEED` (CLAUDE.md §4 requires fixed seeds). An existing
    environment variable overrides it.
  - `proptest-regressions/` is gitignored (redundant with fixed seeds).
- `gravography-sky`:
  - `cmf` — bundled CIE table, parsed once;
  - `planck` — `B_λ`, `B_ν`, SI constants;
  - `band` — `Band::human_vision`, `blackbody_response`, `BlackbodyLut`;
  - `colour` — chromaticity, sRGB matrix derivation, transfer function;
  - `rng` — SplitMix64;
  - `catalogue` — `StarParams`, `generate`, galactic frame;
  - `diffuse` — value-noise fBm band;
  - `cubemap` — face convention, texel solid angle, `CubeLevel`/`CubeMap`, mips, sampling;
  - `bake` — `SkyParams`, splatting, `bake_sky`;
  - `cache` — `bake_sky_cached`.
- `examples/sky_preview.rs` renders the baked sky as an equirectangular PPM (no black hole).
- `assets/cie/` (CSV + CIE metadata), `THIRD_PARTY.md`, `docs/physics.md` § "Colour pipeline and sky", README.

**Key files/modules**
- `crates/gravography-sky/src/band.rs`: band response and the LUT; the GPU texture layout is documented on
  `BlackbodyLut`.
- `crates/gravography-sky/src/cubemap.rs`: face/uv convention table (must match WGSL cube sampling in WP-09).
- `crates/gravography-sky/src/bake.rs`, `catalogue.rs`, `diffuse.rs`: the sky content.
- `crates/gravography-sky/tests/sky_validation.rs`: production-size bake (ignored; run in release).
- `crates/gravography-physics/src/kerr.rs`: ISCO fix. `.cargo/config.toml`: proptest seed.

**Decisions & deviations**
- **CIE data licence:** CC BY-SA 4.0, verified from the CIE metadata file; the MD5 matches. It is bundled
  unmodified with attribution. Flagged to the user (Current state).
- **LUT entries are `[ln S, x, y, 0]`, not raw XYZ.** At 100 K XYZ ≈ e^{−360} underflows f32. Interpolating
  `ln S` and chromaticity in `ln T` is smooth. 4096 entries rather than the PRD minimum of 1024, so the error stays
  ≤ 5e-4 at 100 K.
- **Own PRNG (SplitMix64), not `rand`:** catalogues are bit-stable across dependency updates (NF-3). It is checked
  against Vigna's reference outputs.
- **Star brightness is defined by flux in band channel 1 (`Y`).** `Ω_* = F/Y_BB(T)` follows, so a star's
  apparent brightness doesn't depend on its colour. With the defaults, `Σ Ω_* ≈ 0.13 sr` (mean `w ≈ 0.01`).
- **Diffuse band amplitude 5e-3, chosen by eye with the preview** (4e-4 was invisible, 1e-2 dominated). This is
  a tuning knob for WP-06.
- Cube-map texels are RG32F (`[f32; 2]`). A 2048² default is ≈ 270 MB with mips. RG16F would need a rescaled
  `w`; left to WP-12.
- Star splats and mip filtering do not cross face edges (clamped). Flux is exact; there is < 1 texel position
  error at seams.
- Kernel = bilinear tent (2×2 texels), PRD "≈ 1–2 texels".
- The cache directory is chosen by the caller (no `dirs` dependency yet; the app picks the platform cache dir in
  WP-10).
- Tolerance changes in new tests before commit:
  - The Illuminant A reference was corrected to the published (0.44757, 0.40745); I had misremembered x as
    0.44758. Its tolerance went 1e-5 → 2e-5: the published values are rounded to 5 decimals and computed on a 5 nm
    grid. The measured difference is 1.1e-5.
  - A value-noise continuity test started from the wrong point; that was a test bug.
- New dependencies: `rayon` (parallel diffuse-band evaluation in the bake). Dev: `approx`, `proptest`.
  `thiserror` for `SkyError`.

**Tests**
- 150 passed, 1 ignored (93 physics unit + 7 observer + 8 ray validation + 42 sky). The ignored production bake
  passes in release.
- Test 13: Planckian locus vs Kim et al. (US 7024034) at 10 temperatures 1700–25000 K, worst 1.5e-3 (bound 2e-3).
  Illuminant A to 1.1e-5.
- Test 14:
  - Wien peak to 1e-7;
  - Stefan–Boltzmann to 1e-9 (100 K–10⁷ K);
  - `g³B_ν(ν/g, T) = B_ν(ν, gT)` and `g⁵B_λ(gλ, T) = B_λ(λ, gT)` (property test, 1e-12);
  - band integral of the shifted spectrum equals the response at `gT` (1e-12);
  - RJ and Wien limits.
- LUT vs direct integration: Y < 5e-4, xy < 1e-5 over 100 K–10⁷ K. Clamping is flagged.
- sRGB: D65 → (1, 1, 1), primaries, IEC matrix to < 5e-4, transfer function.
- Catalogue: bit-identical for a seed; class fractions, flux power law and galactic concentration within 5σ;
  unit directions.
- Bake: **flux sum test** `Σ w Ω = Σ Ω_*` (and `Σ wTΩ`) to 1e-5 at every mip level (64² and 2048²/2·10⁵ stars);
  diffuse band adds exactly its integral; a single star keeps its T and position; deterministic.
- Cube map: Σ texel Ω = 4π; face/uv round trip; GL orientation spot checks; trilinear sampling; mips conserve flux.
- Cache: round trip and hit; key changes with parameters; mismatched key or truncated file is a miss.

**Visual checks**
- `sky_preview` (1024² faces, equirectangular 2048×1024): orange K/M-dominated star field with sparse white/blue
  hot stars. The galactic plane, tilted 60° from the spin axis, appears as a sinusoid. With amplitude 5e-3 the
  diffuse band is a soft glow along it. The streaks at the image top and bottom are equirectangular pole
  stretching, not a bake artefact.

**Performance**
- Default bake (2048² faces, 2·10⁵ stars, band, 12 mip levels): 1.46 s in release (M-series Mac). Cache reload
  avoids it (NF-6 < 3 s).

**Known issues / tech debt**
- Equirectangular texture-sky mode (§6.5) and false-colour bands are WP-13; the `Band` struct is the extension
  point (a display mapping still has to be added).
- Mip filtering is the solid-angle-weighted mean; GPU hardware trilinear filtering of a level is a plain
  bilinear mean. The difference is small and only near face corners.

**Notes for the next agent**
- WP-06 shading per pixel:
  `[w, wT] = cube.sample_trilinear(escape_dir, lod)`; `T = wT/w`; `XYZ = w · lut.lookup(g·T).response`;
  `rgb = colour::xyz_to_linear_srgb(XYZ)`; then exposure and tone map. `lod` comes from the ray-differential
  footprint: `lod = ½ log2(Ω_pixel_on_sky / Ω_texel)`, `Ω_texel ≈ 4π/(6 n²)`. See `examples/sky_preview.rs` for
  the whole chain without lensing.
- Use a smaller `face_size` (e.g. 512–1024) for scenes/tests; the default 2048 costs ≈ 270 MB.
- The escape direction from `trace_ray` is in KS Cartesian axes, the same axes as the cube map.

## WP-04 — Observer — 2026-10-30 — COMPLETE

**Summary**
- New module `observer`:
  - `Observer { pos, tetrad, tau, motion }` with `advance(kerr, dτ, thrust_body, params)`: DOPRI5 on the
    20-component state `(x, e_0..e_3)` with the Fermi–Walker law, Gram–Schmidt re-orthonormalisation after
    every substep, and an `AdvanceReport` (substeps, rejected, constraint drift before correction);
  - `Motion::{Free, Hover, Zamo}`: free geodesic + body-frame thrust, or station keeping with automatic thrust;
  - presets `hover`, `zamo`, `circular_orbit` and `radial_infall`, plus `Observer::new` (free, any `u`);
  - `reorient`, `release`, `change_spin`, `acceleration`, `zamo_lorentz_factor`;
  - on `Kerr`: `region` (`Region::{OutsideErgosphere, Ergosphere, InsideHorizon}`), `zamo_angular_velocity`,
    `keplerian_angular_velocity`, `stationary_velocity` / `stationary_acceleration` (`u ∝ ∂_t + Ω∂_φ`);
  - helpers `ks_position`, `look_at_hole`, `oriented_tetrad`, `lorentz_factor`, `speed_from_lorentz_factor`,
    and `acceleration_in_g` with the physical constants.
- `ObserverError` (thiserror, user-facing messages): `InvalidState`, `InnerLimit`, `OuterLimit`,
  `IntegrationFailure`.
- `Tetrad::rotated(R)`: user look rotation of the spatial legs.
- Validation suite `crates/gravography-physics/tests/observer_validation.rs` (PRD tests 9–11, geodetic
  precession, frame dragging).
- `docs/physics.md` § "Observer dynamics": transport law, stationary observers, preset derivations (incl. the
  horizon-regular infall covector), limits, spin change, validation table.

**Key files/modules**
- `crates/gravography-physics/src/observer.rs`: everything above, plus unit tests.
- `crates/gravography-physics/src/error.rs`: `ObserverError`.
- `crates/gravography-physics/src/tetrad.rs`: `Tetrad::rotated`.
- `crates/gravography-physics/tests/observer_validation.rs`: analytic validation (≈1.2 s in debug).

**Decisions & deviations**
- **Inner limit `r_min = r_− + 0.05` for all spins.** It equals the PRD's Schwarzschild `0.05` at `a = 0` and is
  "r_− + margin" for Kerr, continuous in `a`. At `a = 0.999` it still leaves 0.04 M between `r_min` and `r_+`.
  There is also an outer limit `r_max = 1000` (not in the PRD; it guards the f32 GPU path, PRD §5.9; the app may
  lower it to ~200). Both are in `ObserverParams`.
- **Radial infall works for any `θ` in Kerr, not only on the axis.** With `E = 1`, `L_z = Q = 0`, `θ` is exactly
  constant (Doran observers), so a general direction is well defined. Off the axis the observer is dragged in
  `φ`, so the fall is not purely "radial"; this is documented. The UI may still default to the axis for Kerr.
- **Station keeping is a separate `Motion` mode, not thrust fed into the free equations.** The worldline is pinned
  (position snapped onto its circle, `u` reset to `N K` every substep), so hover and ZAMO cannot drift, and the
  transported frame still precesses physically. The user's thrust is ignored in these modes; the app should
  call `release()` when the user thrusts.
- **Preset azimuth is the KS `φ̃`, not BL `φ`** (they differ by `Φ(r)`). This avoids the BL singularity at the
  horizon for infall presets and only matters cosmetically.
- **Thrust is given in the body (transported) frame.** The look rotation stays app-side: render with
  `tetrad.rotated(R)`, and convert view-frame thrust with `R · A'`.
- If `advance` fails part-way, the observer is left at the last valid substep (`tau` shows how far it got), as the
  PRD's "stops with a message" requires. `change_spin` leaves the observer unchanged on error.
- PRD §9 test 10 is also checked at escape (`p_t` conserved), and test 9 through the full ray tracer.
- Tolerance changes in new tests, before the first commit: the circular-orbit `Ω` bound went 1e-9 → 5e-8 and the
  geodetic-precession bound 1e-8 → 1e-7. The first values ignored how global error accumulates over many orbits.
  A convergence probe showed the measured errors (Ω 1.1e-8 at a = 0.9 retrograde near its ISCO 8.72; cos Δφ 1e-8
  at r = 6) are DOPRI5 truncation error, falling ~15× per decade of `rtol`. Details in `docs/physics.md`.
- Two mutation checks on the transport law (flipping the sign of the `A u·e` term; dropping the `u A·e` term) fail
  3–4 unit tests each. Pitfall: restoring a file with `mv` from a copy gives it an older mtime, so cargo does not
  rebuild and keeps running the mutated binary. `touch` the file afterwards.
- No new dependencies.

**Tests**
- 107 passed, 0 ignored (92 unit + 8 ray validation + 7 observer validation); 23 new.
- Test 9 (SR limit, β up to 0.99): aberration ≤ 1e-9, Doppler ≤ 1e-12.
- Test 10: static blueshift to 1e-12 in 26 directions at r = 2.2, 3, 6, 20.
- Test 11: circular orbits (r = 10, 10 orbits): radius drift < 1e-7, Ω error ≤ 1.1e-8, constraint drift < 1e-9;
  radial infall: r(τ) and `τ(r₀ → 0) = (2/3)sqrt(r₀³/2)` to 1e-9 (stopped at r_min = 0.05, crossing the horizon).
- Geodetic precession `2π(1 − sqrt(1 − 3/r))`, r = 6, 10, 30, pro/retro, to 1e-7 (sign checked).
- Frame-dragging sign through the observer API (ZAMO `dφ/dt = ω > 0`; L = 0 infall dragged prograde). Kerr axis
  infall (a = 0.9) stops at `r_− + 0.05` with E, L_z conserved to 1e-8.
- Unit: FW transport preserves `g(e_a, e_b)` (property test incl. thrust), Kepler orbits need zero thrust
  (a ≤ 0.999), ZAMO `L_z = 0`, infall covector `E = 1, L = Q = 0` with the right `dr/dτ` (property test,
  across horizons), right-handed presets, invalid presets refused, hover stays put with a constant frame, flat
  hyperbolic motion, spin change, g-force conversion, look rotation.

**Visual checks**
- n/a (first renders in WP-06).

**Performance**
- Not optimised. One orbit at r = 10 takes ~180 substeps at rtol 1e-10 (each substep is 7 Christoffel
  evaluations). Fine for one observer per frame.

**Known issues / tech debt**
- In station-keeping mode each RHS stage evaluates `g_lo` twice (once in the RHS, once in `stationary_velocity`).
  This is negligible on the CPU.
- No serde derives yet. Bookmarks (WP-13) will need a serialisable mirror of `Observer` (`h_next` is private
  step-size memory and can be reset on load).
- The Kerr static-observer gyroscope precession (frame dragging + geodetic) has no quantitative test. Only the
  Schwarzschild cases (no precession for static observers; geodetic for orbits) are checked.

**Notes for the next agent**
- WP-05 does not touch this code. For WP-06/WP-11: build the camera with `Observer::hover(&kerr, r, θ, φ, &params)`
  or another preset, and render with `observer.tetrad` (or `.rotated(look)`) via `camera_ray_momentum`.
- App loop (WP-11): `obs.advance(&kerr, dt_wall * warp, &thrust_body, &params)`. On `Err(e)` show `e.to_string()`
  and reset to a preset. HUD: `kerr.ks_to_bl_position(&obs.pos)`, `obs.region(&kerr)`, `obs.zamo_lorentz_factor`,
  `acceleration_in_g(sqrt(g(A, A)), mass_solar)` with `A = obs.acceleration(&kerr, &thrust)`.
- To warn below the ISCO, compare the preset `r` with `kerr.isco(prograde)`.

## WP-03 — CPU ray tracing — 2026-10-30 — COMPLETE

**Summary**
- `ode`: generic RK4 and DOPRI5 (FSAL, embedded error), scaled RMS `error_norm`, `StepController`.
- `tetrad`: `Tetrad { e_up[a][μ] }` (legs u, forward, left, up), `gram_schmidt` w.r.t. `g` (two passes),
  `to_coordinate`, `orthonormality_error`.
- `ray`: `camera_ray_momentum` (`p = g(−e₀ + nⁱeᵢ)`), `sky_frequency_shift` (`g = 1/p_t`), `TraceParams`
  (`cpu_reference()`), `trace_ray` / `trace_ray_observed` (callback per accepted step), `RayStatus`, `RayResult`,
  `escape_direction`, `PhaseState` pack/unpack.
- `distant`: `Kerr::bardeen_ray` (exact constants of motion from `(α, β)`).
- `PhysicsError::NoSuchRay`.
- Validation suite `crates/gravography-physics/tests/ray_validation.rs` (PRD tests 4–8, 12).
- `docs/physics.md` § "Ray tracing": initial conditions, integrators, step caps, termination, escape direction,
  Bardeen helper, validation table.

**Key files/modules**
- `crates/gravography-physics/src/ode.rs`: integrators (physics-free).
- `crates/gravography-physics/src/tetrad.rs`: tetrads and Gram–Schmidt (WP-04 reuses it).
- `crates/gravography-physics/src/ray.rs`: the tracer. Termination order is documented in the module doc.
- `crates/gravography-physics/src/distant.rs`: Bardeen distant observer.
- `crates/gravography-physics/tests/ray_validation.rs`: analytic validation (≈3.5 s in debug).

**Decisions & deviations**
- **No weak-field escape-direction correction** (PRD §5.6 calls it optional). The standard correction was
  implemented, measured and removed: it increased the error from 6e-6 to 9e-4 rad. In ingoing KS the coordinate
  direction already converges ≈ R⁻⁴ (see `docs/physics.md` § "Escape direction").
- **CPU escape radius `max(16 r_cam, 400)`** instead of the PRD's example `max(2 r_cam, 100)`. The example gives
  errors up to 1.4e-3 rad (sideways rays, camera at r = 30); 16× gives ≤ 3e-7 for ~16 extra steps. The GPU value is
  left to WP-08/WP-12 (≥ 4× recommended, error < 1e-4).
- **Step caps are geometric, not raw `h` bounds.** Coordinate displacement `≤ 0.1 r`, plus *radial*
  displacement `≤ 0.1 (r − r_+)` for exterior inward rays, converted to `λ` with the current rates. A
  total-displacement horizon cap made near-critical a = 0.998 rays hit MaxSteps (≈10⁴ steps per orbit at r ≈ 1.074).
  The radial cap fixed it.
- `dr/dλ > 0` for escape and `< 0` for capture are tested by comparing `r` with the previous accepted step.
- Momentum blow-up threshold is relative: `max|p_i| > 1e8 · max|p(0)|`.
- Steps below `h_min` are accepted anyway (no stalling); MaxSteps counts accepted + rejected steps.
- RK4 uses the heuristic caps only (no error control); `h_max` acts as "no cap" for it.
- Bardeen sign conventions: `β > 0` toward `+z`; prograde photons appear at `α < 0` (consistent with the
  Appendix A formula `α = −ξ/sin i`).
- Test changes during development: RK4-vs-DOPRI5 originally compared at a fixed `r_escape`, where the two
  runs' different stopping radii dominated. It now uses `r_escape = 1e7` and checks 4th-order convergence (ratio
  16.6). The tetrad property test no longer uses `prop_assume`, which hit proptest's global-reject limit at 10⁴ cases.
- No new dependencies.

**Tests**
- 84 passed, 0 ignored (76 unit + 8 validation); unit property suites also pass at `PROPTEST_CASES=10000`.
- Test 4: 85 rays (66 escaped, 19 captured), a = 0.9, camera at r = 20, θ = 60°: drifts `|H|/|p|² ≤ 4.9e-11`,
  `ΔL_z ≤ 4.5e-11`, `ΔQ ≤ 7.4e-11` (relative, tolerance 1e-8), `p_t` bit-identical.
- Test 5: `b_c` error 3.2e-10 (tolerance 1e-5).
- Test 6: relative error vs `4/b + 15π/(4b²)`: 4.3e-5 (b = 500), 1.1e-5, 2.7e-6, 4.3e-7 (b = 5000). These match the
  size of the next series term (tolerance 1e-3).
- Test 7: Δφ vs Gauss–Legendre quadrature of the orbit equation over the same radial range: 2.8e-9 (b = 5.3)
  down to 2e-11 (tolerance 1e-6).
- Test 8: shadow edges on β = 0, a ∈ {0.5, 0.9, 0.998}: relative error ≤ 1.1e-9 (tolerance 1e-3). Camera-pipeline
  version (static observer at r = 1e4): edge ξ to 1e-6, `α ≈ −r sinψ` within 1e-3 (measured finite-distance offsets 2.7e-5 prograde, 1.5e-4 retrograde).
- Test 12: error vs R = (2, 4, 8, 16)·r_cam falls 14–20× per doubling; default error ≤ 2.6e-7 rad (tolerance 1e-6).
- Unit: DOPRI5 tableau consistency, 5th/4th-order convergence, error-estimate scaling, FSAL, adaptive driver;
  Gram–Schmidt orthonormality in Kerr (1e-12); camera momentum null with `−P·u = 1` and static blueshift;
  radial capture/escape; inside-horizon camera escapes; MaxSteps reporting; flat-space straight rays.

**Visual checks**
- n/a (first renders in WP-06).

**Performance**
- Not optimised. Typical CPU ray: ~20–300 accepted steps; near-critical a = 0.998 rays 4–10·10³ steps. The validation
  suite takes 3.4 s in debug.

**Known issues / tech debt**
- `trace_ray` uses `kerr.r()` separately from the RHS evaluation (one extra `r` per step). Fine for the CPU.
- Termination near the inner horizon / for cameras inside `r_−` is not exercised (cameras there are forbidden, PRD
  §5.8). WP-04 must enforce the limit.

**Notes for the next agent**
- WP-04: `Tetrad::gram_schmidt` is the re-orthonormalisation routine (keep `e_(0) = u` by seeding with `u`). The
  KS-normal observer `n^μ = −g^{μt}` is timelike everywhere (also inside the horizon) and is a handy fallback.
- Test 9 (SR limit) can use `Kerr::with_mass(1e-12, 0)`; `Kerr` does not allow `M = 0`, but the free functions do.
- Test 10 (static blueshift) is already checked at the camera in `ray::tests::camera_momentum_is_null_and_normalised`.
  WP-04 should test it through the observer API and several directions.
- `ode` is generic over `[T; N]`: observer integration can use `dopri5_step` with a 4 + 4 + 16 state, or a
  hand-rolled state.
- The PRD's GPU default `max_steps ≲ 1000` is too small for near-critical high-spin rays (see Performance).
  Expect MaxSteps there and track the rate, as the PRD intends.

## WP-02 — Derivatives, Hamiltonian, invariants — 2026-10-30 — COMPLETE

**Summary**
- Analytic `∂_α r`, `∂_α f`, `∂_α l_μ` (`ks_field_derivatives`) and metric derivatives `ks_dg_lo` / `ks_dg_up`
  (`MetricDerivs = [Mat4; 4]`, `d[α][μ][ν]`).
- Christoffel symbols `ks_christoffel` / `Kerr::christoffel` (`Γ[μ][α][β]`) and `Kerr::geodesic_acceleration`.
- Hamiltonian `ks_hamiltonian` and RHS `ks_hamiltonian_rhs` in vector-only form (no rank-3 tensors), with
  `dp_t = 0` exactly.
- Invariants on `Kerr`: `energy`, `angular_momentum_z`, `carter_q`. `carter_q` uses a Cartesian form that is
  regular on the spin axis, and takes `μ² = −2H`, so it works for null and timelike momenta.
- `docs/physics.md`: derivations of all the above, the regular-Q derivation, and validation notes.

**Key files/modules**
- `crates/gravography-physics/src/kerr_schild.rs`: field and metric derivatives (next to `ks_fields`).
- `crates/gravography-physics/src/connection.rs`: Christoffel symbols and geodesic acceleration.
- `crates/gravography-physics/src/geodesic.rs`: Hamiltonian and Hamilton's equations.
- `crates/gravography-physics/src/invariants.rs`: `E`, `L_z`, `Q`.
- `crates/gravography-physics/src/test_util.rs`: shared test helpers (moved from `linalg`/`kerr_schild`):
  `event()` and `momentum()` strategies, `richardson()` finite-difference derivative, matrix norms.

**Decisions & deviations**
- Derivative checks use Richardson-extrapolated central differences (`O(h⁴)`), not dual numbers. PRD test 2
  allows "and/or", and this avoids a `num-dual` dependency and its `Real`/`FloatConst` plumbing. The tests were
  mutation-checked: a planted sign error in `∂l_y` failed all three derivative tests.
- At `r = 0` all field derivatives are returned as zero (consistent with WP-01's `f = 0` there). The disk is a
  branch surface; ray code must stop before reaching it.
- The Hamiltonian RHS returns `(dx, dp)` as two `Vec4`s. The phase-space state type for integrators is left to
  WP-03.
- `carter_q` returns NaN at `r = 0` (documented). There is no public `p_theta` function. The BL route
  (`ks_to_bl_covector(..)[2]`) gives `p_θ` where it is needed.
- Workspace lint `clippy::needless_range_loop = "allow"`: tensor code reads best with explicit index loops that
  mirror the maths and the future WGSL.
- No new dependencies.

**Tests**
- 55 passed, 0 ignored (`gravography-physics`); 18 new. Property suites also pass with `PROPTEST_CASES=10000`
  (all suites) and 10⁶ cases (flow-conservation test); runtime scales with case count, confirming the env var takes effect.
- PRD §9 test 2: `field_derivatives_match_finite_differences` and `metric_derivatives_match_finite_differences`
  agree to 1e-9 of scale (expected ~1e-12). Plus `∂g^{μν} = −g^{μρ}∂g_{ρσ}g^{σν}`, Schwarzschild `∂r`/`∂f` closed
  forms, `M = 0` gives zero derivatives, and f32 tracks f64 to 1e-5.
- Christoffels: symmetry (4ε), metric compatibility (1e-13·(1+f)³/r), static-observer acceleration
  `M/(r² sqrt(1−2M/r))` (1e-12).
- Hamiltonian: RHS equals the tensor form; RHS equals the finite-difference gradient of `H` (independent of the
  analytic derivatives); Hamilton flow equals the geodesic equation with `Γ`; flat space gives straight rays.
- Invariants: `dH, dE, dL_z, dQ` along the flow vanish (1e-9 of scale); equality with the BL definitions; for `a = 0`,
  `Q + L_z² = |x × p|²`; Bardeen spherical-photon-orbit `ξ, η` round-trip through BL → KS (1e-12).

**Visual checks**
- n/a

**Performance**
- n/a

**Known issues / tech debt**
- `ks_hamiltonian_rhs` calls `ks_field_derivatives`, which calls `ks_fields` (r computed once). That is fine for now;
  WP-12 may fuse it further for the GPU.

**Notes for the next agent**
- WP-03 integrators: use `ks_hamiltonian_rhs` (or `Kerr::hamiltonian_rhs`). `t` is cyclic, so you may skip
  integrating `x[0]` for rays, but keep it available for tests.
- PRD test 8 (Kerr shadow edge): `invariants::tests::spherical_photon_orbit_constants_are_consistent` shows
  how to build a photon from Bardeen `ξ, η` via `bl_to_ks_covector`.
- PRD test 4 (conservation along rays): use `Kerr::{hamiltonian, energy, angular_momentum_z, carter_q}`.
  `carter_q` is safe on the axis.
- Strategies `test_util::event()` / `momentum()` and `test_util::richardson()` are available for new unit tests.

## Maintenance — Toolchain update to Rust 1.99.1 — 2026-10-30 — COMPLETE

**Summary**
- The user replaced Homebrew Rust 1.87.0 with rustup and deleted the stale `~/.cargo/bin/{rustfmt,cargo-fmt}`.
- `rust-toolchain.toml` now pins `1.99.1` (latest stable on this date, PRD PL-1 now satisfied);
  `rust-version = "1.99"` in the workspace; README updated.
- `cargo update` refreshed the lockfile; the `proptest` requirement was raised to 1.12.0. All direct dependencies are on
  their latest stable releases (`approx` 0.6 is only a release candidate, so it stays on 0.5).

**Key files/modules**
- `rust-toolchain.toml`, `Cargo.toml`, `Cargo.lock`, `crates/gravography-physics/Cargo.toml`, `README.md`.

**Decisions & deviations**
- This supersedes the WP-00 deviation (pin to 1.87.0) and the `PATH=/opt/homebrew/bin` workaround in the
  WP-00/WP-01 notes.

**Tests**
- 37 passed, 0 ignored on 1.99.1; fmt (rustfmt 1.10.0) and clippy clean with no code changes needed.

**Visual checks**
- n/a

**Performance**
- n/a

**Known issues / tech debt**
- None.

**Notes for the next agent**
- Just use `cargo` directly. rustup auto-installs the pinned toolchain if missing.

## WP-01 — Metric core — 2026-10-30 — COMPLETE

**Summary**
- `Real` scalar trait (`num_traits::Float + FloatConst`, implemented for f32 and f64; `T::lit(f64)` for literals).
- `linalg`: `Vec4`/`Mat4` plain arrays, `eta`, `mat_mul`, `mat_vec`, `transpose`, `inner`, `lower`, `raise`,
  `determinant`, `inverse` (adjugate).
- `kerr_schild`: free functions `ks_radius` (cancellation-free), `ks_fields` (`r`, `f`, `l_μ`), `ks_g_lo`, `ks_g_up`,
  generic over `M` so `M = 0` can be tested.
- `Kerr<T>`: validated `(M, a)` with `|a| < M`, wrapping the metric, plus radii: `outer_horizon`, `inner_horizon`,
  `ergosurface(θ)`, `photon_orbit(prograde)`, `isco(prograde)`.
- `boyer_lindquist` (methods on `Kerr`): `delta`, `sigma`, `bl_g_lo`, offsets `ks_time_offset` / `ks_phi_offset`,
  `bl_to_ks_position`, `ks_to_bl_position`, `jacobian_ks_from_bl`, vector/covector transforms both ways.
- `docs/physics.md`: KS metric, null vector, cancellation-free `r` and its conditioning, `r = 0` behaviour, BL ↔ KS
  derivation and Jacobian, radii.

**Key files/modules**
- `crates/gravography-physics/src/scalar.rs`: `Real`.
- `crates/gravography-physics/src/linalg.rs`: 4×4 helpers, plus `test_util` (test-only matrix comparisons).
- `crates/gravography-physics/src/kerr_schild.rs`: metric core (the future WGSL port mirrors these names).
- `crates/gravography-physics/src/kerr.rs`: `Kerr` type and characteristic radii.
- `crates/gravography-physics/src/boyer_lindquist.rs`: BL metric and BL ↔ KS transforms.
- `crates/gravography-physics/src/error.rs`: `PhysicsError` (thiserror).

**Decisions & deviations**
- Events are plain `[T; 4]` arrays: KS `[t, x, y, z]`, BL `[t, r, θ, φ]`. No newtypes, to keep the WGSL port 1:1.
  Callers must not mix them up.
- Vector/covector transforms take the **BL event** as the evaluation point. KS → BL transforms that need `J⁻¹`
  return `Result` (`CoordinateSingularity` on the axis or a horizon). `J⁻¹` is numerical (adjugate), not analytic.
- Integration constants of `T(r)` and `Φ(r)` are zero, so `T = 2 ln|r − 2|` for Schwarzschild.
- At exactly `r = 0` (the disk), `ks_fields` returns `f = 0` instead of NaN. Detecting the singularity is the
  caller's job (PRD §5.6). Documented in `docs/physics.md`.
- `Kerr::with_mass` accepts negative `a` (`|a| < M`). The UI clamp `0 ≤ a ≤ 0.999` belongs to the app.
- The frame-dragging sign test (normally WP-04) is done here: it only needs the WP-01 transforms
  (`boyer_lindquist::tests::frame_dragging_sign_for_positive_spin`).
- Tolerances changed during development (not weakened against correct behaviour):
  `radius_is_spheroidal_root` scales its tolerance with the problem's condition number `a²/(r² + a²cos²θ)`
  (the inputs x, y themselves limit accuracy near the disk). Exact symmetry `g[i][j] == g[j][i]` was relaxed
  to 4ε, because `f·lᵢ·lⱼ` and `f·lⱼ·lᵢ` round differently.
- New dependencies: `num-traits` (generic float trait),
  `thiserror` (library errors, CLAUDE.md §6). Dev: `approx`, `proptest`.

**Tests**
- 37 passed, 0 ignored (`gravography-physics`); proptest suites also pass with `PROPTEST_CASES=20000`.
- PRD §9 test 1: `g_lo·g_up = 𝟙`, `det g = −1`, `l` null for `η` and `g`, symmetry, `t`-independence. Property tests
  over a ∈ [0, 0.999] and generic, near-axis, near-equator and near-hole points with `r > 0.3`. Tolerances scale as
  `ε(1 + f)²` (`ε(1 + f)⁴` for det). Plus `M = 0` → Minkowski, `a = 0` → ingoing EF, finite disk interior, and an f32
  identity check (1e-5).
- PRD §9 test 3: Schwarzschild radii (2, 3, 6); near-extremal limits; a = 0.5 tabulated values (4 s.f.); mass
  scaling. Property tests: Δ(r_±) = 0, `g_tt(r_E) = 0`, photon orbits on the circular-orbit energy pole, ISCO on
  the marginal-stability quartic (BPT 1972).
- BL ↔ KS: position round trip (1e-11 relative), KS r = BL r, Jacobian vs finite differences, **pullback
  `Jᵀ g_KS J = g_BL`** (1e-13 of scale), contraction and norm invariance, vector/covector round trips, and
  ZAMO `ω > 0` matching `2Mar/((r²+a²)² − Δa² sin²θ)` to 1e-11.

**Visual checks**
- n/a

**Performance**
- n/a

**Known issues / tech debt**
- `ks_to_bl_position` returns an arbitrary `φ` on the axis (documented).
- No `bl_g_up` yet (invert `bl_g_lo` with `linalg::inverse` if needed).

**Notes for the next agent**
- WP-02: put the analytic `∂_α r`, `∂_α f` and `∂_α l_μ` next to `ks_fields` (same structure, for the WGSL port).
  Check them against central finite differences and/or `num-dual`. Check whether the
  `num-dual` types implement `num_traits::Float` + `FloatConst`; if so, adding `Real` impls (`lit`, `to_f64_lossy`) is
  enough to run the existing generic code on dual numbers.
- `p_θ` for Carter `Q`: `ks_to_bl_covector` already gives `p_BL = Jᵀ p_KS`, and `p_θ` is its index 2 (PRD §5.3).
  That is a cheap cross-check for any closed-form Jacobian you derive.
- Use `Kerr::bl_g_lo` and `bl_to_ks_*` to build test initial conditions from BL formulas.
- ~~Run checks with `export PATH=/opt/homebrew/bin:$PATH`~~: obsolete, see the toolchain entry above.

## WP-00 — Repository bootstrap — 2026-10-30 — COMPLETE

**Summary**
- Cargo workspace (resolver 3, edition 2024) with the four crates of PRD §4.1 and the dependency chain
  `physics` ← `sky` ← `render` ← `app`. The app builds the binary `gravography` (currently it prints a stub message).
- Shared `[workspace.package]` metadata (SPDX `MIT OR Apache-2.0` in every crate via inheritance) and shared
  `[workspace.dependencies]` entries for the internal crates.
- Workspace lints: `missing_docs`, `unused_qualifications` (warn), `unsafe_code` (deny); clippy `unwrap_used`,
  `float_cmp`, `dbg_macro`, `todo` (warn). Warnings become errors under `clippy -D warnings`.
- `LICENSE-MIT`, `LICENSE-APACHE`, `README.md`, `THIRD_PARTY.md`, `docs/physics.md` stub, `scenes/`, `assets/`.

**Key files/modules**
- `Cargo.toml`: workspace, shared metadata, lints, profiles.
- `rust-toolchain.toml`: pins 1.87.0 plus rustfmt and clippy.
- `clippy.toml`: allows `unwrap`/`expect` in tests.
- `rustfmt.toml`: edition 2024, `max_width = 110`.
- `crates/*/src/{lib,main}.rs`: crate-level docs only.

**Decisions & deviations**
- **No `git init`:** the user had already created the repo (commit `360f0d7`, README stub, remote `origin`). The
  WP-00 commit sits on top of it on `main`.
- **Toolchain pin is 1.87.0, not "latest stable" (PRD PL-1):** the machine has Homebrew Rust 1.87.0 and no
  rustup, so `rust-toolchain.toml` has no effect here and I could not install or verify a newer release. Pinning
  the version that was actually tested is the honest choice. If rustup is installed later, bump `channel`
  and `rust-version` together.
- `unsafe_code = "deny"` rather than `forbid`, so a justified local `#[allow]` with a `// SAFETY:` comment is
  still possible (CLAUDE.md §6).
- `[profile.dev.package."*"] opt-level = 2`: dependencies (later `proptest`, `nalgebra`-like crates, `rayon`) are
  optimised in test builds to keep `cargo test` fast. Workspace crates stay at opt-level 0 for debuggability.
- No external dependencies yet. Each WP adds what it needs with `cargo add`.
- `scenes/` and `assets/` hold a `.gitkeep` so git tracks them.

**Tests**
- No tests yet (0 passed, 0 ignored). `cargo build`, `cargo test`, `cargo fmt --all --check` and
  `cargo clippy --workspace --all-targets -- -D warnings` all pass.

**Visual checks**
- n/a

**Performance**
- n/a

**Known issues / tech debt**
- The stale `~/.cargo/bin/{rustfmt,cargo-fmt}` on the user's machine breaks plain `cargo fmt` ("This version of
  rustfmt is deprecated").

**Notes for the next agent**
- Run the checks with Homebrew first in PATH:
  `export PATH=/opt/homebrew/bin:$PATH` before `cargo fmt --all --check` (otherwise `cargo fmt` fails).
- The `missing_docs` and `unwrap_used` lints are active: document every public item and use `expect("reason")`
  only for true invariants.
- `clippy::float_cmp` is on, including in tests: use `approx` (`cargo add -p gravography-physics --dev approx proptest`).
- WP-01 should introduce the scalar abstraction (consider `num-traits::Float`) in `gravography-physics` and start
  filling `docs/physics.md`.
