# Gravography

Real-time interactive viewer that ray traces null geodesics in Schwarzschild and Kerr spacetimes, rendering the
gravitationally lensed, aberrated and Doppler/gravitationally shifted sky seen by an observer near a black hole.
Written in Rust with `wgpu`; macOS (Apple Silicon) is the primary platform.

**Status:** early development. The CPU physics (geodesics, observers), the sky model (colour science, star
field, cube-map baking) and a CPU f64 reference renderer with a headless `render` command are implemented and
tested. The GPU backend renders the same images as the CPU reference (PRD tests 15 and 16) in about a second,
with FR-R6 debug views. The interactive viewer is not built yet. See `docs/PRD.md` for the specification and
`docs/progress_report.md` for progress.

## Building

```bash
cargo build --workspace
cargo test --workspace
cargo run --release -p gravography-app
```

Preview the procedural sky (no black hole) as an equirectangular image:

```bash
cargo run --release -p gravography-sky --example sky_preview -- target/screenshots/sky.ppm 1024 1.0
```

Property tests use a fixed seed (`.cargo/config.toml`); set `PROPTEST_RNG_SEED=<n>` to explore others.
GPU tests (`crates/gravography-render/tests/gpu_parity.rs`, `gpu_tracer.rs`, `gpu_render.rs`) run on the default wgpu adapter (Metal on macOS). On a
machine without a GPU adapter they print `SKIPPED …` and pass. Run them with `-- --nocapture` to see the GPU ↔ CPU
parity report.

Requires Rust 1.99 or newer (edition 2024); the pinned version is in `rust-toolchain.toml`.

## Headless rendering

```bash
cargo run --release -p gravography-app -- render --scene scenes/schwarzschild_far.toml --out target/screenshots/far.png
```

Options: `--out <file.png|file.exr>` (PNG: tone-mapped 8-bit sRGB; EXR: linear HDR, exposure applied, not tone
mapped), `--width W --height H --samples N` (override the scene), `--backend cpu|gpu` (default `cpu`: f64
reference; `gpu`: wgpu/Metal, f32, ~50× faster), `--debug status|shift|steps` (GPU only, PNG: ray-status map,
false-colour frequency shift, step-count heat map), `--no-cache` (do not use the baked-sky cache). The baked sky (≈ 270 MB at the default 2048² faces) is cached
in `~/Library/Caches/gravography` on macOS (`$XDG_CACHE_HOME/gravography` or `~/.cache/gravography` elsewhere);
deleting it is always safe. The CPU reference renderer traces every ray in f64 at `rtol = 1e-10`, so a 960×540
image with 4 samples per pixel takes about a minute on an M1-class Mac; the GPU backend takes about a second.

```bash
cargo run --release -p gravography-app -- render --backend gpu --scene scenes/kerr_edge_on.toml --out target/screenshots/kerr.png
cargo run --release -p gravography-app -- render --backend gpu --debug shift --scene scenes/fast_observer.toml --out target/screenshots/g.png
```

Example scenes in `scenes/`: `schwarzschild_far.toml` (static observer at 50 M), `kerr_edge_on.toml` (`a = 0.99`
seen edge-on), `inside_horizon.toml` (free fall at r = 1.2 M inside a Schwarzschild horizon) and
`fast_observer.toml` (moving at 0.9 c past a Schwarzschild hole at 10 M).

## Scene files

Scene files are TOML. Every section and field is optional except `[observer]` (which may be empty); omitted
values take the defaults below, and unknown keys are rejected. Units: `G = c = M = 1` (lengths in units of the
hole's mass `M`), angles in degrees.

```toml
[spacetime]
spin = 0.0                  # a/M in [0, 0.999]; 0 = Schwarzschild

[observer]
preset = "hover"            # "hover" | "zamo" | "circular_orbit" | "radial_infall"
r = 30.0                    # Boyer–Lindquist radius
theta_deg = 90.0            # polar angle from the spin axis (+z); ignored by circular_orbit
phi_deg = 0.0               # Kerr–Schild azimuth (0 = +x side)
prograde = true             # circular_orbit only
boost = [0.0, 0.0, 0.0]     # extra velocity (forward, left, up) in units of c, |β| < 1
look = { yaw_deg = 0.0, pitch_deg = 0.0, roll_deg = 0.0 }

[camera]
width = 960
height = 540
projection = { kind = "rectilinear", fov_deg = 60.0 }   # vertical field of view, 0 < fov < 180
samples = 1                 # samples per pixel (jittered)

[sky]
seed = 1
star_count = 200000
face_size = 2048            # cube-map face size, power of two ≤ 8192
diffuse_band = true
galactic_normal = [0.0, 0.8660254037844386, 0.5]

[render]
exposure = { auto = true, ev = 0.0 }
tone_map = "aces"           # "aces" | "reinhard"
effects = { lensing = true, frequency_shift = true, beaming = true }
rtol = 1e-10                # CPU integrator relative tolerance, (0, 1e-3]
```

| Key | Meaning |
|---|---|
| `spacetime` | The black hole. `spin` is `a/M`. |
| `observer` | Where the camera is and how it moves (`docs/PRD.md` §5.8). By default it looks at the hole with `+z` up. |
| `preset` | `"hover"`: static, held by thrust (impossible inside the ergosphere). `"zamo"`: zero angular momentum, co-rotating with the frame dragging (outside the horizon). `"circular_orbit"`: equatorial geodesic orbit (above the photon orbit; unstable below the ISCO). `"radial_infall"`: free fall from rest at infinity (allowed inside the horizon; for Kerr it keeps `θ` fixed and is dragged in `φ`). The observer must stay outside `r_− + 0.05` and inside `r = 1000`. |
| `r`, `theta_deg`, `phi_deg` | Position as Boyer–Lindquist radius, polar angle and Kerr–Schild azimuth. |
| `prograde` | Orbit direction for `"circular_orbit"`. |
| `boost` | Lorentz boost of the camera relative to the preset observer, as a velocity in the preset's (forward, left, up) frame. Positions and the preset's motion are unchanged; only the camera frame moves. |
| `look` | Look rotation relative to the default view: `yaw_deg` (positive turns left), then `pitch_deg` (positive looks up), then `roll_deg` (about the view direction). |
| `camera` | Image size (`width`, `height`), `projection` and `samples` per pixel. |
| `projection` | `kind`: `"rectilinear"` (pinhole) with vertical field of view `fov_deg` (the equirectangular projection follows in WP-13). |
| `samples` | Jittered samples per pixel (R2 sequence; 1 = pixel centres). |
| `sky` | Procedural sky: catalogue `seed`, `star_count`, cube-map `face_size`, `diffuse_band` (Milky-Way-like glow) and the `galactic_normal` (pole of the galactic plane, KS axes). |
| `render` | Display and accuracy settings. |
| `exposure` | `auto` = true: the log-average luminance of the lit pixels maps to 0.18, then `ev` stops of compensation. `auto` = false: absolute, `2^ev` divided by the luminance of a 5772 K blackbody filling the pixel. |
| `tone_map` | `"aces"` (filmic, default) or `"reinhard"` (extended Reinhard on luminance, white point 16; hue-preserving). |
| `effects` | Pedagogical toggles (`docs/PRD.md` §6.4): `lensing` (off: straight rays, aberration kept), `frequency_shift` (off: `g = 1`), `beaming` (off: colour shifts but bolometric brightness stays at the `g = 1` value). |
| `rtol` | Relative tolerance of the CPU ray integrator (absolute tolerance is `rtol/100`). |

## Layout

| Crate | Purpose |
|---|---|
| `crates/gravography-physics` | f64 (scalar-generic) Kerr physics: metric, geodesics, observers. No GPU deps. |
| `crates/gravography-sky` | Colour science, blackbody spectra, procedural star field, sky baking. |
| `crates/gravography-render` | CPU reference renderer and wgpu GPU renderer. |
| `crates/gravography-app` | `gravography` binary: interactive viewer and headless `render` CLI. |

## Licence

Licensed under either of [Apache License, Version 2.0](LICENSE-APACHE) or [MIT licence](LICENSE-MIT) at your
option. Third-party data and its licences are listed in [`THIRD_PARTY.md`](THIRD_PARTY.md). The bundled CIE
colour-matching table (`assets/cie/`) is CC BY-SA 4.0 (CIE 2019).
