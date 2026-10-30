# Gravography — Progress Report

Hand-off log between agent sessions. Format and rules: `CLAUDE.md` §7. Newest log entry first.

---

## Current state

- WP-00, WP-01 and WP-02 done. `gravography-physics` has: the scalar abstraction (`Real` for f32/f64), 4×4
  linear algebra, the Kerr–Schild metric with analytic first derivatives, Christoffel symbols, the Hamiltonian
  and its right-hand side (`ks_hamiltonian_rhs`, the form to port to WGSL), the invariants `E`, `L_z` and
  Carter `Q`, BL ↔ KS transforms, and the characteristic radii. No integrators, ray tracing, observer, sky or
  rendering yet.
- **Next work package:** WP-03 — CPU ray tracing.
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
| WP-03 | CPU ray tracing | not started | |
| WP-04 | Observer | not started | |
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
