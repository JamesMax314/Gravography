# Gravography — Product Requirements Document

**Version:** 1.0 (v1 scope + v2 outline)
**Status:** Approved for implementation
**Audience:** Claude Code agent instances implementing the project, one work package at a time.

---

## 1. Overview

Gravography is an open-source, cross-platform, **real-time interactive viewer** that renders what an observer
sees near a black hole by **general-relativistic ray tracing**. The user flies a camera through the
Schwarzschild or Kerr spacetime and sees, live, the gravitationally lensed, aberrated and Doppler/gravitationally
shifted sky.

This is an **exploration and visualisation tool, not a research code**. Physics must be correct (validated
against analytic results), but the priority is interactivity, visual quality and robustness, not
publication-grade numerical precision.

### 1.1 Goals (v1)

1. Real-time GPU ray tracing of null geodesics in Kerr spacetime (Schwarzschild = Kerr with `a = 0`).
2. Lensing of a procedurally generated, **spectrally defined** star field (multi-frequency): every star is a
   blackbody, and its observed colour/brightness follow from the exact frequency shift `g` along each ray.
3. A physically moving observer: free fall on geodesics, optional rocket thrust, Fermi–Walker-transported
   camera frame, presets (hover, circular orbit, radial infall, ZAMO), horizon crossing.
4. Interactive desktop viewer (macOS required, other platforms best-effort) with a control/HUD panel.
5. A headless render mode (CLI) producing PNG/EXR images, used for screenshots and for automated visual checks.
6. A CPU, double-precision reference implementation of all physics, used to validate the GPU path.

### 1.2 Non-goals (v1)

- Accretion disk or any matter emission (v2).
- Video recording (v2).
- Metrics other than Kerr/Schwarzschild (Kerr–Newman, wormholes, numerical metrics, …).
- Polarisation, volumetric radiative transfer, GRMHD data.
- Quantitative science outputs (FITS, per-pixel diagnostics exports as a product feature).
- Official support for Linux/Windows/Web (the code should not preclude them; see §3).
- CI pipelines (not wanted).

### 1.3 Users

Technically literate users (physicists, students, enthusiasts) exploring black-hole optics. Assume the user
understands concepts such as horizon, ergosphere, ISCO and redshift; the UI may use these terms.

---

## 2. Glossary & conventions (normative)

| Item | Convention |
|---|---|
| Units | Geometrised, `G = c = 1`, and **black-hole mass `M = 1`**. All lengths/times are in units of `M`. |
| Spin | Dimensionless `a = J/M²`, range `0 ≤ a ≤ 0.999` (UI clamp). Spin axis is `+z`; `a > 0` means the hole rotates in the `+φ` sense (counter-clockwise seen from `+z`). Must be confirmed by a frame-dragging test (ZAMO `dφ/dt > 0`). |
| Signature | `(−, +, +, +)` |
| Coordinates | **Ingoing Cartesian Kerr–Schild** `(t, x, y, z)`, index 0 = `t`. Boyer–Lindquist (BL) `(t, r, θ, φ)` used only for formulas, presets, tests and HUD display. |
| Momenta | Rays evolved with **covariant** momentum `p_μ`. |
| Hamiltonian | `H(x, p) = ½ g^{μν}(x) p_μ p_ν`; `H = 0` for photons, `H = −½` for unit-mass particles (with affine parameter = proper time). |
| Ray direction | Rays are traced **backward in time** from the camera (past-directed). See §5.4. |
| Frequency shift | `g = ν_obs / ν_emit` (g > 1 is blueshift). |
| Code naming | `g_lo` = `g_{μν}`, `g_up` = `g^{μν}`, `p_lo` = `p_μ`, `u_up` = `u^μ`. Arrays indexed `[t, x, y, z]`. |

---

## 3. Platform & technology requirements

| ID | Requirement |
|---|---|
| PL-1 | Language: Rust, **edition 2024**, toolchain pinned via `rust-toolchain.toml` to the latest stable release available at WP-00. |
| PL-2 | GPU: **`wgpu`** (native Metal backend on macOS; Vulkan/DX12 elsewhere). Shaders in **WGSL**. No direct Vulkan/Metal code. |
| PL-3 | Windowing/input: `winit`. UI: `egui` (via `egui-wgpu`, `egui-winit`). |
| PL-4 | macOS on Apple Silicon is the required, tested platform. Do not use OS-specific APIs outside what `wgpu`/`winit` provide, so Linux/Windows remain possible. |
| PL-5 | GPU arithmetic is **f32 only** (Apple GPUs have no f64). All CPU physics is f64. |
| PL-6 | Licence: dual **MIT OR Apache-2.0** (`LICENSE-MIT`, `LICENSE-APACHE`, SPDX in every `Cargo.toml`). Any bundled data (e.g. CIE tables) must be redistributable; record its source and licence in `THIRD_PARTY.md`. |
| PL-7 | Version control: git, local only (the user pushes). See `CLAUDE.md` for workflow. |

Suggested crates (use `cargo add` to get current versions; do not pin versions in this document):
`wgpu`, `winit`, `egui`, `egui-wgpu`, `egui-winit`, `glam` (f32 + `DVec*` f64 types for app-side math),
`bytemuck`, `pollster`, `rayon`, `image` (PNG), `exr`, `serde`, `toml`, `ron`, `clap`, `log`, `env_logger`,
`thiserror` (libraries), `anyhow` (binary), `rand` + a seedable PRNG (`rand_pcg` or `rand_chacha`).
Testing: `approx`, `proptest`. Optional: `num-dual` (automatic differentiation for the reference
implementation), `num-traits`.

---

## 4. Architecture

### 4.1 Workspace layout

```
gravography/
├── CLAUDE.md
├── README.md
├── THIRD_PARTY.md
├── LICENSE-MIT, LICENSE-APACHE
├── Cargo.toml                    # workspace
├── rust-toolchain.toml
├── docs/
│   ├── PRD.md                    # this file
│   ├── progress_report.md        # agent hand-off log (see CLAUDE.md)
│   └── physics.md                # derivations & conventions, maintained by agents
├── scenes/                       # example scene files (TOML) for headless renders/tests
├── assets/                       # small redistributable data (e.g. CIE CMF table)
└── crates/
    ├── gravography-physics/      # pure f64 (generic) physics, no GPU deps
    ├── gravography-sky/          # spectra, colour science, procedural star field, sky baking
    ├── gravography-render/       # rendering backends: `cpu` (f64 reference, rayon) and `gpu` (wgpu)
    └── gravography-app/          # binary `gravography`: viewer + `render` CLI subcommand
```

Dependency direction: `physics` ← `sky` ← `render` ← `app`. `physics` must not depend on `wgpu`.

### 4.2 Crate responsibilities

**`gravography-physics`**
- Scalar abstraction so core formulas can run in f64 (default), f32 (to emulate GPU precision on CPU) and,
  where practical, dual numbers (to verify analytic derivatives).
- Kerr–Schild metric: `g_lo`, `g_up`, `r(x, y, z)`, null vector `l`, analytic derivatives `∂_α g^{μν}`, and
  Christoffel symbols for observer transport.
- BL ↔ KS coordinate and momentum transforms.
- Characteristic radii: horizons, ergosurface, photon orbits, ISCO (Appendix A).
- Hamiltonian right-hand side; conserved quantities `E = −p_t`, `L_z`, Carter `Q`.
- Integrators: adaptive Dormand–Prince 5(4) (`DOPRI5`) and fixed-step RK4; step-size heuristics.
- Ray tracing (single ray): initial conditions from a tetrad + direction, termination/classification.
- Observer: worldline integration with optional thrust, Fermi–Walker transported tetrad, presets.
- Redshift factor for sky rays.

**`gravography-sky`**
- CIE 1931 2° colour matching functions (bundled table), blackbody spectra, XYZ → linear sRGB.
- Blackbody colour lookup table `T → XYZ` over a wide log-temperature range.
- Observing bands (v1: human-visible; later WP: false-colour bands).
- Procedural star catalogue (deterministic from a seed) and diffuse Milky-Way-like band.
- Baking the catalogue into a mip-mapped, linearly-filterable cube map (§6.3).
- Optional HDR equirectangular texture sky (approximate mode, §6.5).

**`gravography-render`**
- A backend-agnostic `Scene`/`FrameParams` description (spin, observer tetrad, projection, sky, exposure, …).
- `cpu`: f64 reference renderer (rayon), slow but correct. Ground truth for parity tests and images.
- `gpu`: wgpu device management (headless and surface-backed), WGSL module assembly, compute ray tracer,
  shading, accumulation, tone mapping.
- GPU ↔ CPU parity test harness.

**`gravography-app`**
- `gravography` (no args): interactive viewer.
- `gravography render --scene <file.toml> --out <file.png|exr> [--width W --height H] [--backend gpu|cpu] [--samples N]`: headless render.
- Input handling, observer simulation loop, egui panels/HUD, screenshots, bookmarks and camera paths.

### 4.3 Frame pipeline (GPU, per frame)

1. **CPU (f64):** advance observer worldline by `Δτ = Δt_wall × time_warp` (adaptive substeps); transport the
   tetrad; apply user look rotation; compute uniforms (camera position, tetrad as f32, spin, settings).
2. **Compute pass — trace:** one invocation per pixel (at internal render resolution, with sub-pixel jitter).
   Build the initial past-directed momentum from the tetrad, integrate with DOPRI5 in f32, classify the ray,
   and write a G-buffer: `{status, escape direction (unit 3-vector), g, step count}`.
3. **Compute pass — shade:** estimate each pixel's sky footprint from neighbouring escape directions (ray
   differentials), sample the star cube map with explicit gradients, convert the spectral representation to
   XYZ using `g`, write linear HDR colour; accumulate into a progressive buffer when the view is static.
4. **Render pass — present:** exposure + tone mapping, upscale to the swapchain, draw egui on top.

The headless renderer runs the same passes into an offscreen texture.

---

## 5. Physics specification

### 5.1 Kerr–Schild metric (Cartesian, ingoing)

With `η = diag(−1, 1, 1, 1)`, `ρ² = x² + y² + z²`:

- `r` is the positive root of `r⁴ − (ρ² − a²) r² − a² z² = 0`:
  `r² = ½(ρ² − a²) + sqrt(¼(ρ² − a²)² + a² z²)`.
  Use a cancellation-free form when `ρ² − a² < 0` (e.g. `r² = a² z² / (sqrt(¼(ρ²−a²)² + a²z²) − ½(ρ²−a²))`).
- `f = 2 M r³ / (r⁴ + a² z²)`
- `l_μ = ( 1, (r x + a y)/(r² + a²), (r y − a x)/(r² + a²), z / r )`
- `g_{μν} = η_{μν} + f l_μ l_ν`
- `g^{μν} = η^{μν} − f l^μ l^ν`, with `l^μ = η^{μν} l_ν` (`l` is null w.r.t. both `η` and `g`).

Properties to test: `g_lo · g_up = 𝟙`; `det g = −1`; `a = 0` reduces to ingoing Eddington–Finkelstein in
Cartesian form; `M = 0` gives Minkowski. Ring singularity at `r = 0, z = 0` — guard against NaNs.

### 5.2 Characteristic radii

Outer/inner horizons `r_± = M ± sqrt(M² − a²)`; ergosurface `r_E(θ) = M + sqrt(M² − a² cos²θ)`; prograde and
retrograde equatorial photon orbits and ISCO per Appendix A.

### 5.3 Geodesic equations

Hamilton's equations, affine parameter `λ`:

- `dx^μ/dλ = g^{μν} p_ν`
- `dp_μ/dλ = −½ (∂_μ g^{αβ}) p_α p_β`

Since `g^{αβ} = η^{αβ} − f l^α l^β`, `∂_μ g^{αβ} = −∂_μ(f l^α l^β)`; implement analytic derivatives of `r`, `f`,
and `l` (they depend on `x, y, z` only). The `t` coordinate is cyclic: `p_t` is conserved exactly by
construction and `t` need not be integrated for rays.

**Conserved quantities (for tests and diagnostics):**
`E = −p_t`; `L_z = x p_y − y p_x`; `H` (= 0 for photons); Carter constant
`Q = p_θ² + cos²θ (a²(μ² − E²) + L_z² / sin²θ)` with `μ = 0` for photons and `p_θ` obtained via the Cartesian →
spheroidal Jacobian (`x + i y = (r + i a) sinθ e^{iφ}`, `z = r cosθ`; ingoing-Kerr and BL share `r, θ`, and their
`t, φ` differ only by functions of `r`, so `p_θ` is the same in both). Document the derivation in
`docs/physics.md`.

### 5.4 Ray initial conditions and backward tracing

Camera at event `x_c` with orthonormal tetrad `{e_(0) = u, e_(1), e_(2), e_(3)}` (contravariant components),
`g(e_(a), e_(b)) = η_ab`. Spatial legs are labelled forward/left/up and form a right-handed triad.

For a pixel with unit viewing direction `n = nⁱ e_(i)` (pointing from the camera toward the sky),
the arriving photon has `p_photon ∝ e_(0) − n`. We integrate the **past-directed** vector
`k^μ = −e_(0)^μ + nⁱ e_(i)^μ`, lower it with `g_lo`, and evolve with §5.3. At escape, the spatial part of `k`
points toward the source on the celestial sphere.

Because tracing is past-directed, an exterior camera's "captured" rays asymptote to the **past** horizon, which
ingoing KS reaches only at `t → −∞`; the spatial coordinates stay finite but covariant momentum components grow
without bound. A camera **inside** the horizon sees past-directed rays move outward through the **future**
horizon, which ingoing KS covers regularly. Termination logic must handle both (§5.6).

### 5.5 Integrator

- `DOPRI5` with embedded error estimate, per-ray adaptive step, relative/absolute tolerances configurable
  (CPU reference default `1e-10`; GPU default tuned in WP-08/WP-12, expected `~1e-5 – 1e-4`).
- Step-size heuristic caps: `h ≤ c₁ · (r − r_+)` near the outer horizon (exterior rays), `h ≤ c₂ · r` far away;
  enforce `h_min`, `h_max` and a maximum step count (GPU default ≤ ~1000; must avoid GPU watchdog timeouts —
  split dispatches into tiles if needed).
- Fixed-step `RK4` available as a simple baseline and for debugging.

### 5.6 Termination & classification

| Status | Condition (indicative; refine in implementation and document) |
|---|---|
| `Escaped` | `r > r_escape` and `dr/dλ > 0`, with `r_escape = max(k · r_camera, R_min)` (e.g. `k = 2`, `R_min = 100`). Output: normalised spatial direction of `k^μ`. |
| `Captured` | Exterior side: `r < r_+ + ε_h` with `dr/dλ < 0`, or `|p|` exceeds a blow-up threshold. Rendered black. |
| `Singular` | `r` below a small threshold near the ring singularity (should not occur for allowed camera positions; flag it). |
| `MaxSteps` | Step budget exhausted (count as captured for rendering; track the rate as a quality metric). |
| `Error` | NaN/Inf encountered. Rendered magenta in debug builds, black in release. |

Escape-direction accuracy at finite `r_escape` should be checked against larger `r_escape` in tests; optionally
apply a weak-field asymptotic correction.

### 5.7 Frequency shift for the sky

Sources are at rest at infinity (`u_emit = ∂_t`, the same Killing field in BL and KS). For the photon,
`g = ν_obs/ν_emit = (−p_μ u_obs^μ) / (−p_t)`. With the normalisation of §5.4 (`−p·u_obs = 1`), this is
`g = 1 / k_t`, computable **at the camera**, before integration. Specific intensity transforms as
`I_ν,obs = g³ I_ν,emit(ν/g)`; a blackbody of temperature `T` is observed as a blackbody of temperature `g T`.

### 5.8 Observer dynamics

- State: `x^μ`, `u^μ` (or `p_μ` with `H = −½`), tetrad `e_(a)^μ`, proper time `τ`.
- Equation of motion with thrust: `du^μ/dτ + Γ^μ_{αβ} u^α u^β = A^μ`, `A·u = 0`. Thrust `A` is specified by the
  user in the camera frame (`A = Aⁱ e_(i)`), magnitude in units of `c²/M` and displayed in g-forces for a
  configurable physical mass (e.g. `M = 10 M_☉`, default; UI selectable).
- Tetrad: **Fermi–Walker transport**,
  `De^μ_(a)/dτ = (u^μ A_ν − A^μ u_ν) e^ν_(a)` (check: for `e_(0) = u` this gives `Du/dτ = A`). Re-orthonormalise
  (Gram–Schmidt with `g`) every step, keeping `e_(0) = u`.
- User look rotation (mouse) is a separate SO(3) rotation applied to the spatial legs for rendering and for
  interpreting thrust input; it is not physical rotation.
- Integrated on the CPU in f64 with adaptive substeps; constraint `u·u = −1` monitored and corrected.
- Presets (BL formulas, converted to KS):
  - **Hover / static:** `u ∝ ∂_t`; thrust computed automatically to stay static. Not possible inside the
    ergosphere — UI must disable and explain.
  - **Circular equatorial orbit**, prograde or retrograde, at user-chosen `r` (allowed for `r` above the
    corresponding photon orbit; warn below ISCO that the orbit is unstable).
  - **Radial infall from rest at infinity** (along the spin axis for Kerr, or general direction for `a = 0`).
  - **ZAMO** at given `(r, θ)` (with thrust to maintain).
  - **Free:** current state, no thrust.
- Horizon crossing: allowed. For Schwarzschild the camera may go down to a small radius `r_min` (e.g. `0.05`)
  near the singularity, then the simulation stops/resets with a message. For Kerr the camera must stay at
  `r > r_- + margin`; approaching the inner horizon stops the simulation with a message.
- Changing spin `a` while flying: keep position and spatial velocity components, renormalise `u`, rebuild the
  tetrad; if invalid (e.g. now inside a forbidden region), reset to a safe preset with a message.

### 5.9 Precision strategy

- f64 on CPU everywhere (physics crate, observer, reference renderer).
- GPU f32 for per-pixel rays. Keep camera radius in the typical range `r ≲ 200`; the escape radius scales with
  camera radius. Use the cancellation-free `r` formula. Quantify f32 error vs CPU f64 in parity tests.
- The physics crate is generic so the CPU path can also run in f32 to reproduce/diagnose GPU precision issues.

---

## 6. Sky and spectral rendering

### 6.1 Colour pipeline

- CIE 1931 2° colour matching functions (tabulated, bundled under `assets/`, source/licence in `THIRD_PARTY.md`).
- Blackbody LUT: `T ↦ XYZ(T) = ∫ B_ν(T) {x̄,ȳ,z̄}(ν) dν` for `T ∈ [100 K, 10⁷ K]` on a log grid (≥ 1024 entries),
  uploaded as a 1D texture (or 2D strip). Must cover star temperatures × the full range of `g` reachable in the
  app (clamp and indicate when exceeded).
- XYZ → linear sRGB (D65) matrix; tone mapping in the present pass.
- Observing band abstraction: a band = a spectral response function and a display mapping. v1 default: human
  vision (CIE). WP-13 adds false-colour bands (e.g. near-IR, near-UV, soft X-ray) rendered with a colour map.

### 6.2 Procedural star field

- Deterministic from a `u64` seed. Configurable star count (default ~10⁵–10⁶).
- Temperature distribution roughly realistic (dominated by cool K/M stars, few hot O/B), apparent brightness from
  a power-law luminosity function, isotropic positions plus an optional concentration toward a "galactic plane".
- Optional diffuse "Milky Way" band (procedural noise) with its own effective temperature.

### 6.3 Sky representation (baseline)

- Cube map (default 2048² per face, RG16F or RG32F) storing a linearly filterable pair, e.g.
  `(w, w·T)` where `w` is the solid-angle-filling factor (`I_ν = w B_ν(T)`) and `T` is recovered as `(w·T)/w`.
  Stars are splatted as small normalised kernels (≈ 1–2 texels) to avoid aliasing. Full mip chain.
- Shading: `XYZ_obs = w · XYZ_LUT(g · T_eff)`. Lensing magnification emerges automatically from sampling with
  the correct footprint (surface brightness is conserved), so the footprint must come from ray differentials.
- Document the approximation of mixing temperatures within a texel/mip level in `docs/physics.md`.

### 6.4 Effect toggles (pedagogical)

Each independently switchable in the UI: (a) gravitational lensing (off → straight rays in the local frame),
(b) frequency shift (off → `g = 1`), (c) intensity beaming (off → keep colour shift but normalise bolometric
brightness to the `g = 1` value).

### 6.5 Texture sky mode (optional, approximate)

Load an equirectangular HDR image (EXR/HDR). Colour cannot be redshifted exactly from RGB; apply bolometric
`g⁴` intensity scaling and a documented approximate hue shift (e.g. treat each pixel as a blackbody fitted to its
chromaticity). Clearly labelled "approximate" in the UI.

### 6.6 Tone mapping & exposure

Manual exposure (EV) and auto-exposure (smoothed log-average luminance). Operators: at least ACES-fitted and one
neutral alternative (e.g. AgX or Reinhard-extended). Dynamic range is extreme near blueshifted regions; clamp
only in the display pass, never in the HDR buffer.

---

## 7. Functional requirements

### 7.1 Rendering

| ID | Requirement |
|---|---|
| FR-R1 | Real-time GPU rendering of the lensed star field for any allowed observer state and spin `0 ≤ a ≤ 0.999`. |
| FR-R2 | Projections: rectilinear (pinhole, adjustable FOV 10°–150°) and equirectangular 360°. |
| FR-R3 | Dynamic internal resolution while the camera or parameters change; progressive (jittered, accumulated) refinement to full resolution when static. |
| FR-R4 | Correct shadow, photon-ring structure, Einstein rings and multiple images, relativistic aberration and frequency shift. |
| FR-R5 | HDR pipeline with exposure and tone mapping (§6.6). |
| FR-R6 | Debug views: ray status map, `g` map (false colour), step-count heat map. |

### 7.2 Observer & interaction

| ID | Requirement |
|---|---|
| FR-O1 | Mouse look (drag or captured cursor), keyboard thrust (W/S forward/back, A/D left/right, R/F up/down, Space = cut thrust), thrust magnitude adjustable. |
| FR-O2 | Presets per §5.8, with parameters editable in the panel. |
| FR-O3 | Time warp (proper-time rate, including pause), and single-step when paused. |
| FR-O4 | HUD: proper time `τ`, coordinate time `t`, BL `r, θ, φ`, speed relative to local ZAMO (and Lorentz factor), current thrust in g, region indicator (outside ergosphere / in ergosphere / inside horizon), spin, FPS, render scale, ray status rates. |
| FR-O5 | Spin slider usable during flight (§5.8). |
| FR-O6 | Bookmarks: save/load full observer + scene state to RON files; reproduce the view exactly. |
| FR-O7 | Camera paths: record observer states at fixed proper-time intervals; play back. (Foundation for v2 video.) |

### 7.3 Output

| ID | Requirement |
|---|---|
| FR-X1 | Screenshot from the viewer (key `P`): tonemapped PNG and linear HDR EXR, at the current full resolution (forcing a converged render first). |
| FR-X2 | Headless CLI render from a TOML scene file (`scenes/*.toml`), GPU or CPU backend, configurable resolution and samples per pixel. |
| FR-X3 | Scene file schema documented in `README.md` and covered by a round-trip test. |

---

## 8. Non-functional requirements

| ID | Requirement |
|---|---|
| NF-1 | **Performance target:** 60 fps at 1080p output on an M1-class Mac using dynamic internal resolution while moving (internal scale may drop to ≥ 0.5); full-resolution converged image within ~1 s when static. Measure and record numbers (machine, settings, timings) in `progress_report.md` (WP-12). |
| NF-2 | Robustness: no panics in the viewer under any UI input; invalid states produce messages and safe resets. |
| NF-3 | Determinism: same scene file + seed ⇒ identical CPU-reference image; GPU images identical on the same machine. |
| NF-4 | Code quality: `cargo fmt` clean, `cargo clippy --workspace --all-targets -- -D warnings` clean, public items documented. |
| NF-5 | Testing per `CLAUDE.md`: unit tests for every physics/colour function, analytic validation tests, GPU parity tests. |
| NF-6 | Startup time < 3 s on the reference machine (sky baking may be cached on disk under the platform cache dir). |

---

## 9. Validation requirements (minimum test set)

Physics (CPU, f64):

1. Metric identities (§5.1) at random points (property tests), including near the axis and the equatorial plane.
2. Analytic `∂g^{μν}` vs finite differences and/or dual numbers.
3. Characteristic radii vs Appendix A; ZAMO frame-dragging sign check for `a > 0`.
4. Conservation along rays: `|H|`, `E`, `L_z`, `Q` drift below documented tolerances.
5. Schwarzschild critical impact parameter `b_c = 3√3 M` (bisection on captured/escaped from a distant source; agree to ≤ 1e-5).
6. Weak deflection `δ ≈ 4M/b + (15π/4)(M/b)²` for large `b` (relative agreement ≤ 1e-3 at `b ≥ 500`).
7. Schwarzschild exact deflection angle vs numerical quadrature of the orbit equation for several `b > b_c`.
8. Kerr shadow edge for a distant equatorial observer: edges on the `β = 0` axis at `α = −ξ(r_ph,pro)` and `α = −ξ(r_ph,retro)` (Appendix A), to ≤ 1e-3 relative with a distant observer (`r_obs ≥ 10⁴`, account for finite distance).
9. Special-relativity limit (`M = 0`): aberration `cos θ' = (cos θ − β)/(1 − β cos θ)` and Doppler `g = 1/(γ(1 − β cos θ))` for a boosted observer.
10. Static observer in Schwarzschild: uniform sky blueshift `g = (1 − 2M/r)^{-1/2}` for all directions.
11. Observer: circular geodesic orbit stays circular (radius drift bound over ≥ 10 orbits at `r = 10`); orbital period matches `Ω = 1/(r^{3/2} ± a)`; `u·u = −1` and tetrad orthonormality maintained; radial infall from rest at infinity: `τ(r₀ → 0) = (2/3) sqrt(r₀³/(2M))` (Schwarzschild).
12. Escape-direction convergence with increasing `r_escape`.

Colour:

13. Planckian locus chromaticities `(x, y)` from the LUT match published values at several temperatures (≤ 2e-3).
14. Wien peak and Stefan–Boltzmann scaling sanity checks; blackbody `T → gT` transformation consistency.

GPU:

15. Parity: for a fixed set of cameras (Schwarzschild & Kerr `a ∈ {0.5, 0.9, 0.998}`, outside and inside horizon, moving observers), ray-status agreement ≥ 99.5% overall (disagreements confined to near the critical curve) and escape-direction angular error ≤ 1e-3 rad for ≥ 99% of agreeing rays.
16. Image parity: GPU vs CPU reference images for `scenes/*.toml` within a documented perceptual/relative-luminance tolerance.

---

## 10. Work packages (v1)

Each work package (WP) is sized for one agent session. **An agent completes exactly one WP per session**, then
updates `docs/progress_report.md`, commits, merges, tags and stops (see `CLAUDE.md`). WPs are done in order unless
the progress report says otherwise. Every WP must leave `main` building with all tests passing.

### WP-00 — Repository bootstrap
- `git init`, workspace `Cargo.toml`, `rust-toolchain.toml` (current stable), four crate skeletons, `.gitignore`
  (incl. `target/`), licences, `README.md` stub, `THIRD_PARTY.md`, `docs/physics.md` stub, `scenes/`, `assets/`.
- Workspace lints (`[workspace.lints]`), `rustfmt.toml` if needed.
- **Accept:** `cargo build`, `cargo test`, `cargo fmt --check`, `cargo clippy -D warnings` pass on an empty
  skeleton; initial commit on `main`; tag `wp-00`.

### WP-01 — Metric core
- Scalar abstraction; KS metric (`r`, `f`, `l`, `g_lo`, `g_up`); BL ↔ KS transforms (positions, vectors, covectors);
  characteristic radii.
- **Accept:** tests 1 and 3 (except frame-dragging, which may need WP-04 utilities — then do it there).

### WP-02 — Derivatives, Hamiltonian, invariants
- Analytic `∂_α g^{μν}` and `∂_α g_{μν}`; Christoffel symbols; Hamiltonian RHS; `E, L_z, Q`; document derivations in
  `docs/physics.md`.
- **Accept:** test 2; Christoffels symmetric and consistent with metric derivatives.

### WP-03 — CPU ray tracing
- DOPRI5 + RK4, step control, termination & classification, ray from tetrad + direction, escape direction, `g`.
  Distant-observer helper (Bardeen `α, β` → initial conditions) for tests.
- **Accept:** tests 4–8, 12.

### WP-04 — Observer
- Observer state, thrust, Fermi–Walker transport, re-orthonormalisation, presets, region detection, horizon
  crossing limits, spin change handling, frame-dragging test.
- **Accept:** tests 9–11; frame-dragging sign test.

### WP-05 — Colour & sky
- CIE tables, blackbody LUT, sRGB conversion, procedural star catalogue, diffuse band, cube-map bake with mips,
  optional on-disk cache.
- **Accept:** tests 13–14; deterministic catalogue for a seed; bake produces correct total flux (sum test).

### WP-06 — CPU reference renderer + headless CLI
- `Scene`/`FrameParams`, scene TOML schema, f64 rayon renderer (trace, footprint via ray differentials, shading,
  tone mapping), `gravography render --backend cpu`, PNG + EXR output, example scenes (Schwarzschild far view,
  Kerr `a = 0.99` edge-on, inside-horizon view, fast-moving observer).
- **Accept:** renders of all example scenes (visually inspected and described in the progress report); FR-X3 test.

### WP-07 — GPU foundation
- wgpu device (headless), WGSL module assembly (simple ordered concatenation of shader modules), metric + RK4 ray
  tracer as a compute shader on a batch of test rays, readback; parity harness skeleton.
- **Accept:** RK4 GPU vs CPU-f32 vs CPU-f64 comparison test runs and reports errors.

### WP-08 — GPU ray tracer
- DOPRI5 in WGSL, termination/classification identical to CPU, per-pixel trace pass writing the G-buffer, tiling
  if required for watchdog limits.
- **Accept:** test 15.

### WP-09 — GPU shading & headless GPU render
- Footprint from neighbouring rays, cube-map sampling with gradients, spectral shading, accumulation buffer, tone
  mapping, debug views, `gravography render --backend gpu`.
- **Accept:** test 16; FR-R6 debug views render.

### WP-10 — Viewer shell
- winit window, wgpu surface, render loop with a fixed camera, resize handling, egui panel scaffolding (render
  settings, exposure), screenshot (FR-X1).
- **Accept:** app launches on macOS, renders the default scene, resizing works, screenshot files written.

### WP-11 — Interactive flight
- Input mapping (FR-O1), per-frame observer integration, presets UI (FR-O2), time warp (FR-O3), HUD (FR-O4), spin
  slider (FR-O5), effect toggles (§6.4), user-facing messages for invalid states.
- **Accept:** manual test checklist written into the progress report and executed by the agent where possible
  (e.g. via scripted state + headless renders); unit tests for input → thrust mapping and state transitions.

### WP-12 — Performance
- GPU timing (timestamp queries if available, else CPU frame timing), dynamic resolution controller, progressive
  refinement, integrator tolerance/step-cap tuning, optional **Schwarzschild fast path** (spherical symmetry ⇒
  escape direction depends only on camera `r` and the angle to the radial direction: a per-frame 1D LUT of ~2–4k
  rays, sampled non-uniformly near the critical angle) behind a toggle with a parity test against the full tracer.
- **Accept:** NF-1 measured and recorded; no regression in parity tests.

### WP-13 — v1 polish & release
- Bookmarks (FR-O6), camera paths (FR-O7), projection modes (FR-R2), false-colour observing bands, texture-sky mode
  (§6.5), README with usage, controls, screenshots and scene format; `THIRD_PARTY.md` complete.
- **Accept:** all FRs satisfied or explicitly deferred in the progress report with user approval; tag `v1.0.0`.

---

## 11. v2 outline (not to be implemented in v1)

- **Thin accretion disk:** Novikov–Thorne (and optionally simpler power-law) emissivity, disk in the equatorial
  plane from ISCO outward, multi-frequency via local blackbody temperature `T(r)` shifted by `g` computed with
  the Keplerian emitter four-velocity; higher-order images (n = 1, 2) and an option to isolate them.
- **Video recording** of camera paths (offline, fixed time step, high samples per pixel) to image sequences and/or
  MP4 (via an external encoder or a pure-Rust option).
- Kerr acceleration via per-frame 2D LUT, point-source star rendering (per-pixel catalogue lookup inside the ray
  footprint), bloom/glare.
- Possible later: thick disks/tori, polarisation, additional metrics, Web (WebGPU) build.

Design v1 interfaces (e.g. the ray termination and shading stages) so that adding emitting surfaces in v2 does
not require rewriting the tracer: termination should be extensible with "surface hit" events, and the G-buffer
should have room for an emitter record.

---

## Appendix A — Reference formulas (BL, `M = 1`)

- Horizons: `r_± = 1 ± sqrt(1 − a²)`.
- Equatorial circular photon orbits: `r_ph = 2 {1 + cos[(2/3) arccos(∓a)]}` (upper sign prograde).
- ISCO: `Z₁ = 1 + (1 − a²)^{1/3} [(1 + a)^{1/3} + (1 − a)^{1/3}]`, `Z₂ = sqrt(3a² + Z₁²)`,
  `r_isco = 3 + Z₂ ∓ sqrt((3 − Z₁)(3 + Z₁ + 2Z₂))` (upper sign prograde).
- Equatorial circular orbit angular velocity: `Ω = ±1 / (r^{3/2} ± a)`.
- Photon-orbit constants of motion (Bardeen 1973), for spherical photon orbits of radius `r`:
  `ξ = L_z/E = [r²(3 − r) − a²(r + 1)] / [a (r − 1)]`,
  `η = Q/E² = r³ [4a² − r(r − 3)²] / [a² (r − 1)²]`.
  Distant observer at inclination `i`: `α = −ξ / sin i`, `β = ± sqrt(η + a² cos² i − ξ² cot² i)`.
- Schwarzschild: photon sphere `r = 3`, critical impact parameter `b_c = 3√3`, ISCO `r = 6`.
- Proper time of radial free fall from rest at infinity: `τ(r₀ → 0) = (2/3) sqrt(r₀³ / 2)`.
- Static observer redshift (Schwarzschild): `g = (1 − 2/r)^{-1/2}` for light from infinity.

## Appendix B — References

- Bardeen, J. M. (1973), *Timelike and null geodesics in the Kerr metric*, in Black Holes (Les Houches).
- Bardeen, Press & Teukolsky (1972), ApJ 178, 347 — circular orbits, ISCO.
- Visser, M. (2007), *The Kerr spacetime: a brief introduction*, arXiv:0706.0622 — Kerr–Schild forms.
- James, von Tunzelmann, Franklin & Thorne (2015), *Gravitational lensing by spinning black holes in
  astrophysics, and in the movie Interstellar*, CQG 32, 065001 — camera tetrads, ray bundles, star rendering.
- Luminet, J.-P. (1979), A&A 75, 228 — black-hole imaging.
- Gralla & Lupsasca (2020), PRD 101, 044032 — null geodesics of Kerr (analytic reference).
- CIE 1931 2° standard observer colour matching functions (CVRL database).
