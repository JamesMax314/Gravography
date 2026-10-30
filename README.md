# Gravography

Real-time interactive viewer that ray traces null geodesics in Schwarzschild and Kerr spacetimes, rendering the
gravitationally lensed, aberrated and Doppler/gravitationally shifted sky seen by an observer near a black hole.
Written in Rust with `wgpu`; macOS (Apple Silicon) is the primary platform.

**Status:** v1. What it does:

- **Physics.** Light rays are traced in f64 on the CPU (the reference) and in f32 on the GPU, which matches the
  reference within the PRD's parity tests. The observer is simulated physically: free fall, thrust, a
  Fermi–Walker-transported camera, and horizon crossing.
- **Sky.** A procedural star field in which every star is a blackbody, so colour and brightness follow the exact
  frequency shift. False-colour bands (near IR, near UV, soft X-ray) and an approximate texture-sky mode are
  available.
- **Viewer** (macOS, 60+ fps at 1080p):
  - free flight with mouse look, presets, time warp, a HUD, a live spin slider and effect toggles;
  - rectilinear and 360° equirectangular projections;
  - bookmarks and recorded camera paths;
  - dynamic resolution while moving, progressive refinement when still.
- **Headless renders** and a benchmark from the command line.

See `docs/PRD.md` for the specification, `docs/physics.md` for derivations and validation, and
`docs/progress_report.md` for the development log.

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

## Viewer

```bash
cargo run --release -p gravography-app                                   # default scene (static observer, r = 30 M)
cargo run --release -p gravography-app -- --scene scenes/kerr_edge_on.toml
cargo run --release -p gravography-app -- --bookmark bookmarks/my-view.ron     # resume a saved view
```

Viewer options: `--scene <file.toml>`, `--bookmark <file.ron>`, `--screenshot-dir` (default `screenshots/`),
`--bookmark-dir` (default `bookmarks/`), `--path-dir` (default `paths/`), `--no-cache` and `--smoke-test`.

The observer starts at the scene's `[observer]` (preset, boost, look) and is simulated in real time: each frame
advances its proper time by the frame time × the **time warp** (in `M` per second), integrating its worldline and
Fermi–Walker-transported frame on the CPU in f64. While the view moves, every frame is a fresh one-sample image;
once it is still, samples accumulate up to the sample target.

| Input | Action |
|---|---|
| left-drag | look around ("grab the sky"; a view rotation, not a physical one) |
| right-click | toggle captured mouse look: the cursor hides and mouse motion turns the view; right-click or **Esc** releases |
| **W / S**, **A / D**, **R / F** | thrust forward/back, left/right, up/down, relative to where you look |
| **Space** | cut thrust: stop station keeping and fall freely |
| **Enter** | pause / resume; **.** single step while paused |
| **[ / ]** | halve / double the time warp |
| **- / =** | halve / double the thrust |
| **P** | screenshot (tone-mapped PNG + linear HDR EXR at full window resolution, in `screenshots/` or `--screenshot-dir`) |
| **Esc** | release captured mouse look, otherwise quit |

The left panel has these sections:

- **Flight:** pause and step, time warp, thrust magnitude (`c²/M`), black-hole mass in solar masses (only used to
  show accelerations in g), cut thrust and reset look.
- **Presets:** hover, ZAMO, circular orbit (prograde or retrograde) and radial infall at chosen `r`, `θ`, `φ`.
  Impossible presets are disabled with an explanation, for example hovering inside the ergosphere or an orbit
  below the photon orbit. Orbits below the ISCO get a warning.
- **Spacetime:** the spin slider, usable in flight. It keeps the position and velocity.
- **Band:** human vision, or a false-colour composite (near infrared, near ultraviolet, soft X-ray). See `band`
  under "Scene files".
- **Effects:** lensing, frequency shift and beaming, each switchable.
- **Render:**
  - projection: rectilinear with a 10–150° vertical field of view, or equirectangular 360°;
  - sample target and still scale (the internal resolution when the camera is at rest, default 1.0);
  - dynamic resolution while moving, with its target frame rate (default 60 fps);
  - auto/manual exposure, tone map and screenshot.
- **Bookmarks & camera paths:**
  - **Save bookmark** writes `bookmarks/<name>.ron` with the complete state: the observer's event, frame, proper
    time and motion; the look; the flight and render settings; the spin; the sky. **load …** restores it exactly.
  - **Record path** samples the observer every chosen interval of proper time. The integration stops exactly at
    each mark. **Stop & save path** writes `paths/<name>.ron`.
  - **play …** replays a path at a chosen rate (proper time per second), optionally looping. Paths are the basis
    for offline video, planned for v2.

  Both are plain RON text files.

**Performance.** While the camera moves, each frame is a fresh 1-sample image. Its internal resolution is chosen
from the measured GPU time so that the target frame rate holds, but never below 960×540 pixels. When the camera
stops, the view switches to the still scale and accumulates samples. On an M2 Pro at 1080p:

- moving views run at 70–190 fps at the 0.5 scale;
- a still view converges (16 samples) in 0.3–0.9 s.

Window resizes, including full-screen transitions, take effect once the size has been stable for 150 ms. Until
then the last image is stretched.

The **HUD** (top right) shows:

- proper time `τ`, KS time `t`;
- BL `r, θ, φ`;
- speed and Lorentz factor relative to the local ZAMO;
- the felt acceleration (in `c²/M` and in g);
- the region (outside the ergosphere, in the ergosphere, inside the horizon);
- the spin, the frame rate and the render size;
- the ray-status rates.

Thrust keys switch a hovering or ZAMO observer to free flight. Crossing the horizon is allowed. The simulation
stops near the singularity (Schwarzschild, `r < 0.05 M`) or the inner horizon (Kerr), or beyond `r = 200 M`. It
then resets to the last preset, paused, with a message.

`--smoke-test` runs a scripted check for automated verification:

1. it renders, takes a screenshot, resizes the window, renders and takes a screenshot again;
2. it drags the view through the viewer's mouse handlers;
3. it sets `a = 0.5` and falls from `r = 8` through the horizon while recording a camera path, then takes a
   screenshot inside;
4. it replays the recorded path and round-trips a bookmark (the camera must be restored exactly).

After each screenshot it saves a capture of the composed window, panel and HUD included (`window-*.png`). Its
files stay under the screenshot directory.

**Texture sky** (approximate, `docs/PRD.md` §6.5). Set `texture = "path/to/sky.exr"` (or `.hdr`) in `[sky]`
to use an equirectangular linear-radiance image instead of the star field. The image centre is `+x` and the top
is `+z`.

Real images have no spectra, so each pixel is shown as the blackbody whose colour is nearest to the pixel's
chromaticity, at the pixel's luminance. Shifted light then changes hue along the blackbody colours, and its
brightness scales as `g⁴`. Colours far from that locus (greens, purples) are approximated by the nearest blackbody
colour, and the panel says so. Pixel values are relative, so use auto exposure.

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

Performance measurement (GPU time per sample at several render scales, static convergence, and optionally PRD
test 15 with the same integrator settings):

```bash
cargo run --release -p gravography-app -- bench --scene scenes/kerr_edge_on.toml            # 1920×1080, scales 1, 0.75, 0.5
cargo run --release -p gravography-app -- bench --width 2560 --height 1440 --scales 1,0.5 --parity
```

`bench` accepts overrides of the GPU integrator settings for tuning: `--rtol`, `--far-step`, `--horizon-step`,
`--escape-factor`, `--escape-min`, `--horizon-eps`, `--capture-fraction`, `--blowup`, `--max-steps` and `--tile`.
`--steps-map <dir>` writes the step-count heat maps.

Example scenes in `scenes/` (each file's header describes what to expect):

- `schwarzschild_far.toml`: a static observer at 50 M.
- `kerr_edge_on.toml`: `a = 0.99` seen edge-on.
- `inside_horizon.toml`: free fall at r = 1.2 M inside a Schwarzschild horizon.
- `fast_observer.toml`: moving at 0.9 c past a Schwarzschild hole at 10 M.
- `kerr_panorama.toml`: a 360° equirectangular view at r = 8 M around `a = 0.9`.
- `fast_observer_infrared.toml`: the fast observer as a near-infrared false-colour panorama, with hue following
  the Doppler shift.

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
projection = { kind = "rectilinear", fov_deg = 60.0 }   # or { kind = "equirectangular" } (360°, use 2:1)
samples = 1                 # samples per pixel (jittered)

[sky]
seed = 1
star_count = 200000
face_size = 2048            # cube-map face size, power of two ≤ 8192
diffuse_band = true
galactic_normal = [0.0, 0.8660254037844386, 0.5]
band = "human_vision"       # "human_vision" | "near_infrared" | "near_ultraviolet" | "soft_xray"

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
| `projection` | `kind`: `"rectilinear"` (pinhole) with vertical field of view `fov_deg` (0 < fov < 180), or `"equirectangular"`: the full sphere, centre = forward, longitude across (+180° at the left edge), latitude down (use a 2:1 image for square pixels). |
| `samples` | Jittered samples per pixel (R2 sequence; 1 = pixel centres). |
| `sky` | Procedural sky: catalogue `seed`, `star_count`, cube-map `face_size`, `diffuse_band` (Milky-Way-like glow) and the `galactic_normal` (pole of the galactic plane, KS axes). |
| `band` | Observing band (`docs/PRD.md` §6.1). `"human_vision"`: CIE 1931 colour. The false-colour composites each show three wavelength windows as blue, green and red: `"near_infrared"` (J, H, K: 1.1–1.4, 1.5–1.8, 2.0–2.4 µm), `"near_ultraviolet"` (200–260, 260–320, 320–380 nm) and `"soft_xray"` (2–8, 1–2, 0.3–1 keV). A blackbody peaking in the band (1756 K, 9990 K, 3.6 MK) shows neutral grey. Hotter or blueshifted light looks bluer, cooler or redshifted light redder. Ordinary stars are dark in X-rays unless blueshifted by `g ≳ 50`. The star field itself is the same in every band. |
| `render` | Display and accuracy settings. |
| `exposure` | `auto` = true: the log-average luminance of the lit pixels maps to 0.18, then `ev` stops of compensation. `auto` = false: absolute, `2^ev` divided by the luminance of the band's reference blackbody filling the pixel (5772 K for human vision). |
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
