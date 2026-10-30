# Gravography — Progress Report

Hand-off log between agent sessions. Format and rules: `CLAUDE.md` §7. Newest log entry first.

---

## Current state

- WP-00 to WP-04 done. `gravography-physics` has:
  - the full single-ray CPU pipeline: Kerr–Schild metric and derivatives, Hamiltonian RHS, invariants,
    BL ↔ KS transforms, radii, tetrads, DOPRI5/RK4, `trace_ray` and camera rays with `g = 1/p_t`;
  - observer dynamics (WP-04): `Observer` with thrust, a Fermi–Walker transported tetrad, the presets hover,
    ZAMO, circular orbit, radial infall and free, regions, inner/outer limits, spin change, look rotation
    (`Tetrad::rotated`) and g-force conversion.
- PRD §9 tests 1–12 pass. No sky or rendering yet.
- **Next work package:** WP-05 — Colour & sky.
- Toolchain: rustup, pinned to stable **1.99.1** (`rust-toolchain.toml`). Plain `cargo fmt/clippy/test`
  work; the old `PATH=/opt/homebrew/bin` workaround is obsolete (Homebrew Rust is uninstalled).
- Open questions for the user: none.

---

## Work package status

| WP | Title | Status | Tag |
|---|---|---|---|
| WP-00 | Repository bootstrap | done | `wp-00` |
| WP-01 | Metric core | done | `wp-01` |
| WP-02 | Derivatives, Hamiltonian, invariants | done | `wp-02` |
| WP-03 | CPU ray tracing | done | `wp-03` |
| WP-04 | Observer | done | `wp-04` |
| WP-05 | Colour & sky | not started | |
| WP-06 | CPU reference renderer + headless CLI | not started | |
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
