# Gravography — Progress Report

Hand-off log between agent sessions. Format and rules: `CLAUDE.md` §7. Newest log entry first.

---

## Current state

- **All work packages WP-00 to WP-13 are done.** v1 is feature-complete per `docs/PRD.md`:
  - FR-R1–R6, FR-O1–O7 and FR-X1–X3 are satisfied;
  - NF-1 is met (WP-12; 2560×1440 re-measured in WP-13);
  - PRD §9 tests 1–16 pass.
- **What exists** (details in the log entries):
  - physics (Kerr–Schild geodesics, observers) in f64 on the CPU, the reference;
  - the sky: blackbody star field, cube map, human vision plus false-colour bands, and an approximate texture sky;
  - the CPU reference renderer and the GPU renderer (dynamic resolution, async statistics);
  - the `gravography` binary: viewer, `render` and `bench`.
  - The viewer's projections, bookmarks and camera paths are new in WP-13.
- **Released as `v1.0.0`** (2026-10-30), with the user's approval.
  - Deferred with approval: the optional Schwarzschild fast path (WP-12). NF-1 is met without it.
  - README screenshots: the user approved committing curated images in `docs/images/` (six JPEGs, about
    1.9 MB). This is recorded as an exception in `CLAUDE.md` §3.
- **Next:** v2 (`docs/PRD.md` §11): thin accretion disk, video of camera paths, Kerr LUT acceleration.
- Toolchain: rustup, Rust **1.99.1** (`rust-toolchain.toml`).
- The user approved shipping the CC BY-SA 4.0 CIE table (2026-10-30; see `THIRD_PARTY.md`).
- The user confirmed (2026-10-30) that the viewer opens and resizes. They reported full-screen lag (addressed in
  WP-12) and "inverted" thrust, which is physical aberration. They chose (2026-10-30) **not** to add an
  aberration-free mode.
- Open questions for the user:
  - The example scenes use auto exposure with −2 EV (−4/−5 EV for some), chosen by eye. Adjust to taste.

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
| WP-07 | GPU foundation | done | `wp-07` |
| WP-08 | GPU ray tracer | done | `wp-08` |
| WP-09 | GPU shading & headless GPU render | done | `wp-09` |
| WP-10 | Viewer shell | done | `wp-10` |
| WP-11 | Interactive flight | done | `wp-11` |
| WP-12 | Performance | done | `wp-12` |
| WP-13 | v1 polish & release | done | `wp-13`, `v1.0.0` |

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

## v1.0.0 release — 2026-10-30

- **User decisions:** tag `v1.0.0`; add README screenshots. The six images in `docs/images/` were rendered
  with `render --backend gpu --samples 16` from the example scenes, plus the smoke-test window capture, as
  JPEG quality 82 (290–370 KB each). The README has a new "Screenshots" section, and `CLAUDE.md` §3 records the
  exception.
- **Tag:** `v1.0.0` on the merge commit of branch `v1-release`. No code changes.

## WP-13 — v1 polish & release — 2026-10-30 — COMPLETE

**Summary**
- **Projection modes (FR-R2):**
  - `Projection::Equirectangular` (full sphere) on the CPU and GPU (a `projection` code in `GpuCamera`);
  - viewer controls: projection selector and rectilinear FOV 10–150°;
  - mouse sensitivity per projection (`Projection::degrees_per_pixel`);
  - example `scenes/kerr_panorama.toml`.
- **Bookmarks (FR-O6)** (`viewer/persist.rs`): RON files with the full state, so a view is reproduced bit for bit:
  - observer event, tetrad, τ and motion (`Observer::from_state` validates them);
  - look, preset, flight and view settings, spin and sky spec.
  - Panel: name, save, load list. `--bookmark <file>` starts from one, using its sky.
- **Camera paths (FR-O7):**
  - `Recorder` stores samples at exact proper-time marks: `Flight::update_marked` splits the integration at each
    mark;
  - `Playback` replays at a chosen M/s, optionally looping;
  - RON files in `paths/`.
- **False-colour bands (§6.1):** near IR (J, H, K), near UV and soft X-ray (Chandra split) as three-window
  composites (short → B, medium → G, long → R).
  - A band now carries a display matrix and luminance weights. Each channel is normalised so that the band's
    Wien-peak reference blackbody is neutral; human vision is unchanged bit for bit.
  - The viewer switches bands live (only the LUT is re-uploaded). Scenes use `[sky] band = …`.
  - Example `scenes/fast_observer_infrared.toml`.
- **Texture sky (§6.5, approximate)** (`gravography-sky/src/texture.rs`):
  - an equirectangular EXR/HDR image becomes the `(w, w·T)` cube by a per-pixel blackbody fit (nearest Planckian
    locus point, luminance-matched);
  - so the renderer's `w B(gT)` gives `g⁴` brightness scaling and the hue shift automatically;
  - `[sky] texture = "…"`; the panel shows an "approximate" warning.
- **Fixed a WP-12 flaw in the GPU timer.** `on_submitted_work_done` callbacks only fire in `poll`, so with one
  poll per frame the measured "busy time" was the frame interval. The HUD showed "GPU 0.0 ms" when two completed
  in one poll, and on a 60 Hz display the controller would always have scaled down. A helper thread now blocks on
  each sample's submission index and timestamps the real completion:
  - 1.8 / 15.3 ms at 320×180 / 1280×720, against 2.9 / 23.3 ms quantised before;
  - the smoke-test fall converges toward the budget.
- **Docs:**
  - `README.md`: status, viewer options, panel sections, texture sky, band and projection in the scene schema,
    new example scenes;
  - `THIRD_PARTY.md`: the egui fonts (Ubuntu Font Licence, MIT / Bitstream Vera, OFL 1.1, MIT) and a summary of
    the Rust dependency licences;
  - `docs/physics.md`: equirectangular, false-colour bands and texture-sky sections.
- **Smoke test:** additionally records the fall as a camera path (checks the 0.5 M grid), replays it, and
  round-trips a bookmark (camera restored exactly). Its files stay under the screenshot directory.

**Key files/modules**
- `crates/gravography-render/src/camera.rs`: equirectangular projection; `shaders/camera.wgsl`; `gpu/tracer.rs`
  `PROJECTION_*`.
- `crates/gravography-app/src/viewer/persist.rs`: `Bookmark`, `CameraPath`, `Recorder`, `Playback`, RON I/O.
- `crates/gravography-app/src/viewer/flight.rs`: `update_marked`, `restore`.
- `crates/gravography-physics/src/observer.rs`: `Observer::from_state`.
- `crates/gravography-sky/src/band.rs`: `Band::{near_infrared, near_ultraviolet, soft_xray, false_colour}`,
  display matrix, luminance weights, `LN_S_FLOOR`.
- `crates/gravography-render/src/scene.rs`: `BandKind`, `SkySpec::{band, texture}` (`SkySpec` is no longer `Copy`).
- `crates/gravography-sky/src/texture.rs`, `crates/gravography-render/src/output.rs::load_equirect`.
- `crates/gravography-render/src/gpu/timer.rs`: helper-thread timer.
- `scenes/kerr_panorama.toml`, `scenes/fast_observer_infrared.toml`.

**Decisions & deviations**
- **"False-colour bands rendered with a colour map" (PRD §6.1)** is implemented as three-window composites mapped
  to RGB, not a scalar colour map. A composite fits the existing three-channel LUT/shading pipeline with no new
  pass, and it keeps the frequency shift visible as hue, which a single-channel map cannot. Bands, windows and
  reference temperatures are listed in the README.
- **The star field is band-independent.** The catalogue's visible flux defines each star's `(w, T)`, and the
  bake always uses the human-vision table. Ordinary stars are therefore essentially invisible in soft X-rays
  unless blueshifted by `g ≳ 50`, which is physically right and documented.
- **Texture-sky approximation:** the hue moves along the Planckian locus only; off-locus colours collapse. It is
  labelled "approximate" in the UI and README, as the PRD requires.
- **Bookmarks use real state, not presets,** so arbitrary flight states are reproducible. They are rejected if
  the tetrad is not orthonormal for the stored spin (corrupt or foreign files).
- **New dependencies:** `serde` (derive) and `ron` in the app (PRD-suggested crates, for RON bookmarks and paths),
  and the `image` crate's `hdr` feature (Radiance input for the texture sky).
- **Changed tests:** `mouse_sensitivity_matches_pinhole_centre` moved into the render crate as part of
  `equirectangular_covers_the_sphere` (sensitivity is now a `Projection` method). The view-settings reset test
  covers projection and band. Several of my new tests had wrong expectations, which I corrected before
  committing (marks at the frame boundary, lit-pixel count, X-ray "zero" vs 1e-35, Y vs Rec. 709 luminance).
  No existing tolerance was loosened.

**Tests**
- `cargo test --workspace`: **254 passed, 3 ignored** (was 242). Ignored tests pass in release.
- **New tests:**
  - `equirectangular_covers_the_sphere` (4π to 1e-5, axis directions);
  - `from_state_restores_exactly_and_validates`, `marked_updates_stop_exactly_on_the_marks`;
  - `bookmarks_round_trip_bit_for_bit`, `recorded_paths_have_fixed_intervals_and_play_back`,
    `file_stems_are_safe`;
  - `false_colour_bands_are_neutral_at_reference_and_shift_with_temperature`,
    `xray_lut_is_finite_where_stars_are_dark`;
  - `blackbody_colours_are_recovered`, `orientation_and_uniform_images`;
  - `exr_round_trip_and_refusals`, `uniform_texture_renders_as_blueshifted_blackbody` (g from PRD test 10,
    within 6%).
- **Test 16** now also covers both new scenes:
  - `kerr_panorama`: flux within 0.2%; with the same footprint method the images match to 2e-5;
  - `fast_observer_infrared`: flux within 0.8%; same footprint method 4e-6.

**Visual checks**
- **`kerr_panorama`:** the galactic band is a sinusoid across the map. The shadow is centred, ≈ 34° in radius,
  flattened on the left and shifted right (Kerr).
- **`fast_observer_infrared`:** the shadow is about 28° right of centre (prediction 16°–42°). The sky behind is
  red/orange, ahead blue-white and saturated (beaming ≈ g⁴).
- **Texture sky:** the Kerr panorama EXR used as the sky of a Schwarzschild scene is lensed into a new Einstein
  ring and photon ring, with its colours preserved. Conversion: 2.3 s for a 1024² cube.
- **Window capture:** the panel shows the band selector and the "Bookmarks & camera paths" section. The HUD shows
  real GPU ms after the timer fix.

**Performance**
- **2560×1440 (WP-12 follow-up, GPU idle; ms per sample, scales 1 / 0.5 / 0.375):**

  | Scene | 1.0 | 0.5 | 0.375 (= 960×540) | Still, 16 spp at 1440p |
  |---|---|---|---|---|
  | schwarzschild_far | 52.1 ms | 13.5 ms | 7.7 ms | 0.83 s |
  | kerr_edge_on | 92.1 ms | 23.8 ms | 13.6 ms | 1.47 s |
  | inside_horizon | 60.1 ms | 15.8 ms | 9.2 ms | 0.97 s |
  | fast_observer | 32.9 ms | 8.8 ms | 5.1 ms | 0.53 s |

  At the moving floor (960×540 pixels) every scene exceeds 60 fps (74–195).
- The equirectangular and false-colour paths cost the same per ray: the panorama renders 960×480×4 spp in 0.7 s,
  including startup.

**Known issues / tech debt**
- **Texture conversion speed:** brute-force nearest-locus search, about 2.3 s for a 1024² cube and an estimated
  9 s at the default 2048². A precomputed 2D `(x, y) → T` table would make it O(1) per texel.
- **No viewer control to load a texture:** texture skies are set in the scene file only.
- **Bookmark sky mismatch:** a bookmark loaded under a different sky warns instead of re-baking. `--bookmark`
  at startup uses the bookmark's sky.
- **Per-frame allocations** (WP-12 note) remain.

**Notes for the next agent**
- `v1.0.0` waits for the user's approval of the deferrals and the screenshot question (see "Current state"). Do
  not tag it without that.
- **v2** (`docs/PRD.md` §11):
  - accretion disk: add a surface-hit status to the tracer and an emitter record to the G-buffer (it has a free
    `_pad` word);
  - video: `CameraPath` plus the headless renderer;
  - Kerr LUT acceleration.

## WP-12 — Performance — 2026-10-30 — COMPLETE

**Summary**
- **Measurement first:** `gravography bench` (`crates/gravography-app/src/bench.rs`).
  - It renders single progressive samples (exactly the viewer's per-frame work) at several scales and waits for
    each; it also times the trace alone and a static convergence.
  - `--parity` runs PRD test 15 with the same integrator settings. Every GPU integrator parameter can be
    overridden from the command line, and `--steps-map` writes step heat maps.
  - Baseline at 1080p: 755 ms per sample at scale 1 and 190 ms at scale 0.5 (Schwarzschild). The trace pass was
    100% of it. This was the lag the user saw while flying.
- **9.3× faster trace from a shader change.** `ks_hamiltonian_rhs` (WGSL) is now in fused vector form
  (`docs/physics.md` § "GPU right-hand side in vector form"): same maths, no arrays or dynamically indexed
  vectors. naga had lowered those to thread-local memory on Metal. PRD test 15 was unchanged.
- **Integrator retuning (≈ 2.5× fewer steps):** found with `bench --parity` sweeps.
  - The far-field step cap was binding (`rtol` had no effect at all): it is now 0.5, with `rtol = 2e-6`.
  - Escape radius `max(4 r_cam, 100)`.
  - Captured rays stop at a spin-scaled margin `r_+ + ½ (r_ph,pro − r_+)`. This is a new
    `TraceParams::capture_photon_fraction`, justified by the photon periapsis bound, documented and tested.
  - Momentum blow-up `10³`.
  - 1024² dispatch tiles (≈ 10% faster than 256²).
  - The CPU reference is unchanged (κ = 0).
- **GPU timing:** `gpu::GpuTimer` measures the GPU busy time per sample:
  `done(N) − max(submitted(N), done(N−1))`, from `Queue::on_submitted_work_done`, polled without blocking and
  tagged with the render scale.
- **Dynamic resolution (FR-R3):** `state::DynamicResolution`, a damped, quantised controller using `t ∝ s²`.
  - It aims at 80% of the frame budget for the target fps (default 60).
  - Floor: at least 960×540 internal pixels (`min_moving_scale`). That is the PRD's "1080p at 0.5" as a pixel
    count, so the floor means the same on Retina windows.
  - The camera counts as moving for 120 ms after its last change. The render-scale setting is now the **still**
    scale (default 1.0): a still camera switches to it and accumulates (progressive refinement).
- **Resize debounce** (the user's full-screen lag): the surface is reconfigured at once, but the internal
  resolution follows only once the size has been stable for 150 ms. Meanwhile the old image is stretched.
- **No blocking readbacks in the frame loop:** `gpu::GpuStats` (`stats.wgsl`) computes the log-average luminance
  (per-workgroup f32 partial sums of `ln Y`) and the ray-status counts (workgroup atomics) on the GPU. It reads
  back ≈ 8 bytes per 256 pixels asynchronously.
  - It replaces the HDR-image and G-buffer copies that caused a hitch once a second while moving.
  - Metering interval while moving: 0.25 s.
  - Screenshots wait for statistics if none have arrived, so they never use an unmetered exposure.
  - **Bug found and fixed in this WP:** the metering at convergence could be refused (both readback slots busy
    with requests from the motion) and was never retried, because no further samples are rendered after
    convergence. A still view then kept the exposure and ray rates of the motion: the smoke test showed 81%
    "escaped" inside the horizon, against 4% correct. `state::Metering` now keeps a metering due until a request
    is accepted (test `a_refused_metering_at_convergence_is_retried`), and the smoke stages wait for it.
- **Viewer UI:** still scale, a dynamic-resolution checkbox, target fps, and the GPU ms in the HUD.

**Key files/modules**
- `crates/gravography-render/src/gpu/shaders/geodesic.wgsl`: vector-form RHS.
- `crates/gravography-physics/src/ray.rs`:
  - `TraceParams::capture_photon_fraction` and `capture_margin`;
  - the new GPU constants (`GPU_RTOL`, `GPU_FAR_STEP_FACTOR`, `GPU_ESCAPE_*`, `GPU_CAPTURE_PHOTON_FRACTION`,
    `GPU_MOMENTUM_BLOWUP`);
  - the periapsis test.
- `crates/gravography-render/src/gpu/timer.rs`: `GpuTimer`, `busy_ms`.
- `crates/gravography-render/src/gpu/stats.rs` and `shaders/stats.wgsl`: `GpuStats`, `FrameStats`, `combine`.
- `crates/gravography-render/src/gpu/tracer.rs`: `DEFAULT_TILE = 1024`.
- `crates/gravography-app/src/viewer/state.rs`: `DynamicResolution`, `Metering`, `min_moving_scale`, `gpu_budget_ms`,
  `MOVE_HOLD`, `RESIZE_SETTLE`, `METER_INTERVAL` (0.25 s).
- `crates/gravography-app/src/viewer/mod.rs`: frame loop (timing → controller → resize → restart), async stats,
  debounced resize.
- `crates/gravography-app/src/bench.rs`: the benchmark.

**Decisions & deviations**
- **The WGSL RHS no longer mirrors the Rust code line by line** (CLAUDE.md §5 asks for the same structure). That
  structure was the 9× bottleneck. The function keeps its name, the maths is identical, the derivation is in
  `physics.md`, and the GPU-vs-CPU RHS, DOPRI5 test-ray and test 15 comparisons cover it.
- **GPU-only classification change:** captured rays stop at the spin-scaled margin, which the CPU reference does
  not use (κ = 0 there). This is sound by the periapsis bound, and the status agreement with the reference in
  test 15 is 99.999%.
- **Timestamp queries were not used.** The adapter reports `TIMESTAMP_QUERY`. With pass `timestamp_writes`
  (wgpu 30 / Metal), however:
  - the shade pass's end-of-pass timestamp read back as 0;
  - with a begin write added, the shade pass's values were stale from the second frame on (identical to the
    first), while the trace pass's begin timestamp updated.

  The PRD's fallback ("else CPU frame timing") is used instead: queue-completion busy time. The first version
  (plain submission → completion) read 33 ms at 115 fps, because the CPU ran ahead and queueing was counted. The
  busy-time formula fixes that.
- **The render-scale meaning changed:** it is now the still scale (default 1.0, was 0.5). While moving the
  controller decides, with a floor of 960×540 pixels instead of a fixed 0.5. On a 2560×1440 Retina window the
  floor is scale 0.375.
- **Schwarzschild fast path (optional in the PRD): deferred.** NF-1 is met without it on the hardest (Kerr)
  example scene, and a second tracer would need its own parity test. If wanted, it fits as a per-frame 1D LUT in
  the trace pass (see the PRD WP-12 text).
- **Changed test:** `gpu_timer_measures_a_sample` first asserted that 16× the pixels takes > 2× the time. It
  failed once in about 20 runs, because parallel GPU tests share the device. That violates CLAUDE.md §4
  (determinism), so the scaling is now printed, not asserted. It was my own new test, before merging.
  `accumulation_progresses_and_meters` and the view-settings tests were extended for the new fields.
- **No new dependencies.** PRD unchanged.

**Tests**
- `cargo test --workspace`: **242 passed, 3 ignored** (was 231). Ignored tests (release) pass:
  `test15_full_resolution`, `test16_larger`, `default_sky_bake_conserves_flux`.
- **New tests:**
  - `photon_periapsis_never_below_prograde_photon_orbit` (with a positive control) and
    `capture_margin_scales_with_the_photon_orbit`;
  - `gpu_timer_measures_a_sample`, `busy_time_excludes_queueing`;
  - `gpu_stats_match_cpu_readbacks` (log-average to 3e-7 relative, exact status counts, partial workgroups),
    `partials_combine_like_the_cpu_log_average`;
  - `dynamic_resolution_converges_to_the_budget` (from above and below, no oscillation),
    `dynamic_resolution_respects_limits_and_ignores_garbage`, `moving_floor_and_budget`;
  - `summary_statistics` (bench);
  - `a_refused_metering_at_convergence_is_retried`.
- **Test 15 with the new defaults (128²/camera):**
  - status 99.999% (2 differ, next to the critical curve);
  - 99.78% of directions within 1e-3 rad;
  - p99 1.9e-4 rad (0.2 px at 1080p/60°);
  - mean steps 42 (was 100).
- **Test 16:** flux within 0.4%, and ≤ 3.4% of pixels > 8 levels (as before, footprint method).

**Visual checks**
- **Smoke test window captures:** panel, HUD and image are correct at hover and inside the horizon. The HUD shows
  GPU ms and the active scale.
- **Steps heat map** (`bench --steps-map`) of Kerr edge-on: before the retuning the shadow (captured rays) cost
  about 2× the sky. This motivated the capture-margin change.

**Performance** (Apple M2 Pro, macOS, 1920×1080 output, `gravography bench`, ms per sample = trace + shade;
measured with nothing else on the GPU)

| Scene | Before WP-12, scale 0.5 | Scale 1.0 | Scale 0.75 | Scale 0.5 | Static 16 spp at 1080p (before) |
|---|---|---|---|---|---|
| schwarzschild_far | 190 ms | 29.6 ms | 17.0 ms | 7.9 ms (127 fps) | 0.49 s (12.1 s) |
| kerr_edge_on (a = 0.99) | 337 ms | 52.7 ms | 29.8 ms | 14.0 ms (72 fps) | 0.85 s (20.6 s) |
| inside_horizon | 194 ms | 35.8 ms | 32.5 ms* | 9.5 ms (105 fps) | 0.56 s (12.5 s) |
| fast_observer | 149 ms | 19.5 ms | 11.3 ms | 5.3 ms (188 fps) | 0.31 s (9.5 s) |

\* noisy p95 in that run.

- **NF-1:** 60 fps at 1080p with internal scale ≥ 0.5 while moving: **met** on all scenes, worst case 14 ms. A
  converged full-resolution image in ~1 s when still: **met**, 0.3–0.9 s.
- **Viewer smoke test** (window 1920×1152 physical, 120 Hz display): falling through the horizon, 75–79 fps. The
  controller climbed from the floor toward the 13.3 ms budget during the 2.5 s fall.
- **Caveat:** a later bench run was disturbed by a viewer instance the user was running (about 2× slower, noisy),
  so those numbers were discarded.
- **2560×1440 (Retina) was not measured cleanly:** the user's viewer stayed open for the rest of the session.
  Cost scales with pixel count, since the per-ray cost does not depend on resolution, so the moving floor
  (960×540) performs like the 1080p scale-0.5 row. A full-resolution still image at 2560×1440 has 1.78× the
  pixels of 1080p: about 1.5 s to 16 samples for Kerr edge-on. **Next agent:** re-run
  `bench --width 2560 --height 1440 --scales 1,0.5,0.375` with no viewer open and record the result.

**Known issues / tech debt**
- **Per-frame allocations remain:** uniform buffers and bind groups in `render_sample`, `Presenter::draw` and
  `GpuStats::request`. They are not significant now (the trace dominates), but could be made persistent.
- **Convergence on large windows:** a Retina window at full resolution is 3.7 M pixels. For Kerr edge-on, still
  convergence (16 samples) is estimated at about 1.5 s, slightly over the ~1 s goal, which NF-1 states for 1080p.
  Lower the still scale or the sample target if that matters.
- **Timer approximation:** the busy time can include the present/egui work of the previous frame if it overlaps.
  The controller is damped, so this is harmless.
- **Smoke-test convergence while the camera is about to switch scale:** the smoke stages may converge at the
  moving scale within the 120 ms hold (cosmetic, smoke only).
- **Schwarzschild fast path:** not implemented (see Decisions).

**Notes for the next agent (WP-13)**
- **Bookmarks (FR-O6)** need `Flight` serialisation: `Observer` (`pos`, `tetrad`, `tau`, `motion`; `h_next` is
  private, so reset it on load), `LookSpec`, `FlightSettings`, the preset, the spin and `ViewSettings`.
- **Projection modes (FR-R2):** `Projection` has only `Rectilinear`. Equirectangular needs WGSL
  `projection_direction` and CPU support. The mouse sensitivity (`input::degrees_per_pixel`) assumes rectilinear.
- **Check performance after a change:** `cargo run --release -p gravography-app -- bench --scene … --parity`.
  Close any running viewer first, since it shares the GPU.
- **Fonts:** `epaint_default_fonts` (enabled in the WP-11 follow-up) must be listed in `THIRD_PARTY.md`.

## WP-11 follow-up — Viewer UI fixes — 2026-10-30 — COMPLETE

User report after WP-11: "I don't seem to be able to see the HUD or controls … the movement with the keys works
but the mouse movement doesn't seem to." Fixed on branch `wp-11-ui-fixes`, merged without a new tag (`wp-11`
is left where it was).

**Summary**
- **Root cause, unreadable panel and HUD:** egui 0.37's `default` feature set is empty, and the bundled fonts
  sit behind `default_fonts`. With no font, every character is drawn as a solid box. This affected WP-10 too;
  nobody had seen the live window.
  - **Fix:** `egui` gets `features = ["default_fonts"]`.
  - `⏸`, `▶` and `☉` are not in the bundled fonts, so they were replaced with "Pause"/"Resume" and "M_sun".
- **Latent bug:** egui texture deltas (the font atlas comes once, on the first frame) were uploaded *after*
  `get_current_texture`. A skipped first frame would have lost the atlas for good. The upload now comes first.
- **Mouse look:** the left-drag code worked: a drag through the viewer's handlers turns the view (smoke test).
  Two problems remained:
  - the only hint ("drag: look around") was unreadable;
  - there was no "move the mouse to look" mode.

  Changes:
  - new `input::MouseLook` state machine (unit-tested), covering left-drag ("grab the sky") and right-click
    captured look (FPS convention, via `DeviceEvent::MouseMotion`; the cursor is locked and hidden);
  - right-click, Esc or focus loss releases the capture; Esc quits only when nothing is captured;
  - a hint line under the Flight section.
- **Window-frame capture (`viewer/capture.rs`):**
  - the surface is configured with `COPY_SRC` where supported;
  - after each smoke-test screenshot, the next presented frame is copied to `window-<name>.png`, including the
    panel and HUD;
  - this is the first agent-side view of the composed window, and it is how the font bug was found.

**Decisions & deviations**
- **Captured cursor in addition to drag.** FR-O1 allows either, and the user expected plain mouse movement to
  look around. Right-click toggles it, so a drag still works without capturing.
- **Dependency change:** the `default_fonts` feature on `egui`, which adds `epaint_default_fonts` (OFL/Ubuntu
  font licences, embedded by egui). **TODO WP-13:** list the fonts in `THIRD_PARTY.md`.

**Tests**
- `cargo test --workspace`: **231 passed, 3 ignored**. New tests:
  - `drag_looks_only_from_the_3d_view`;
  - `right_click_toggles_captured_look`;
  - `rows_are_aligned_and_unpadded` (capture row unpadding).
- **Smoke test** (passes), which now also:
  - drags 200 px right and 100 px down via the handlers, giving yaw +11.5° and pitch +5.7°;
  - writes three window captures.

**Visual checks**
- **Window capture, before the fix:** the panel and HUD were laid out but every glyph was a filled box.
- **After the fix:** readable panel (Flight, Presets, collapsed sections, Keys) and HUD. At hover r = 30:
  τ, KS t, r = 30.0000, θ = 90°, φ = 0°, β = 0, γ = 1, acceleration 1.150e-3 c²/M = 7.14e8 g, "outside the
  ergosphere, hovering", fps, scale and ray rates.
- **Infall start at r = 8, a = 0.5:** v vs ZAMO = 0.49976 c, close to √(2M/r) = 0.5 for a = 0, as expected.

**Known issues / tech debt**
- **HUD width:** the HUD window wraps long lines (about 420 logical px). Cosmetic; WP-13 polish.
- **Untested on hardware:** captured mouse look needs the live window (raw `DeviceEvent`s cannot be injected
  here). **User:** right-click, move the mouse, then press Esc.

## WP-11 — Interactive flight — 2026-10-30 — COMPLETE

**Summary**
- **Flight state** (`viewer/flight.rs`, no GPU code, fully unit-tested): `Flight` holds the `Kerr`, the
  `Observer<f64>`, the look rotation (`LookSpec`), the flight settings and the last applied preset.
  - `update(dt_wall, keys)`: `Δτ = Δt_wall (≤ 0.1 s) × time warp`, or one `step_tau` when paused and a step was
    requested.
  - Thrust is `|A| · R_look · d̂_view` in the body frame. A thrust key releases hover/ZAMO station keeping first.
  - Limit or integration errors reset to the last preset (or the safe hover at r = 30) and pause, with a message.
  - `apply_preset` with `check_preset`:
    - errors: hover inside the ergosurface, ZAMO at or inside `r_+`, an orbit at or below the photon orbit,
      `r` outside `[r_min, 200]`;
    - warning: orbits below the ISCO.
  - `set_spin` uses `Observer::change_spin` and resets on failure. `cut_thrust` is Space.
  - `look_by`: yaw wraps to (−180°, 180°], pitch is clamped to ±89°.
- **HUD values** (`Hud`, FR-O4):
  - τ, KS t;
  - BL r, θ, φ (φ shown as "—" where it is not finite);
  - β and γ relative to the ZAMO (none inside the horizon);
  - the felt acceleration in c²/M and in g (selectable mass);
  - the region, spin and motion mode.

  The renderer adds fps, scale, size, samples and ray-status rates.
- **Input** (`viewer/input.rs`): physical-key mapping.
  - Held: W/S, A/D, R/F.
  - Once per press: Space cut thrust, Enter pause, `.` step, `[ ]` warp ÷2/×2, `- =` thrust ÷2/×2, P screenshot,
    Esc quit.
  - Diagonals are normalised. The drag sensitivity matches the pinhole scale at the image centre ("grab the sky").
- **Viewer** (`viewer/mod.rs`):
  - per frame: apply the panel, advance the flight, compare the camera with the accumulated one
    (`Camera::differs`, relative tolerance 1e-9), restart if it moved, then add one sample;
  - redraws continuously while running; idles when paused and converged;
  - key releases always count, and held keys clear on focus loss;
  - presses are ignored when egui consumes them.
- **Panel** (`viewer/ui.rs`): Flight, Presets (with the error/warning text and a disabled Apply), Spacetime (spin),
  Effects (§6.4 toggles), Render (with exposure), a key reference, and the HUD window in the top-right corner.
- **Exposure while moving:** `Accumulation::meter_exposure_now(since_last)` meters at convergence, and on the
  first sample only if ≥ 1 s has passed (`METER_INTERVAL`). The same throttle drives the ray-status readback.
  The last log-average is kept across camera restarts.
- **Smoke test extended:** after the resize stage it sets a = 0.5 while hovering (must not reset), applies radial
  infall from r = 8 at 4 M/s, falls until r < 1.5, asserts the HUD region is "inside the horizon", pauses,
  converges and takes a third screenshot. It prints the HUD at each screenshot.
- **Render crate:** `ObserverSpec::place` and `place_boosted`, which releases station keeping when boosted.
  `Scene::observer` and `Scene::frame` now use them (same results).

**Key files/modules**
- `crates/gravography-app/src/viewer/flight.rs`: `Flight`, `FlightSettings`, `Camera`, `check_preset`,
  `safe_preset`, `Hud`, `viewer_params` (r_max = 200).
- `crates/gravography-app/src/viewer/input.rs`: `key_action`, `ThrustKeys`, `degrees_per_pixel`, `drag_to_look`.
- `crates/gravography-app/src/viewer/ui.rs`: panel sections, `FlightPanel`, `hud_lines`, `hud_window`.
- `crates/gravography-app/src/viewer/state.rs`: `ViewSettings.effects`, `METER_INTERVAL`.
- `crates/gravography-app/src/viewer/mod.rs`: event handling, per-frame flight, camera-change restart, smoke test.
- `crates/gravography-render/src/scene.rs`: `ObserverSpec::{place, place_boosted}`.
- `docs/physics.md` § "Interactive flight (viewer)"; `README.md` § "Viewer" (controls table).

**Decisions & deviations**
- **Keys beyond FR-O1:** pause, step, warp and thrust shortcuts are my choice (Enter, `.`, `[ ]`, `- =`). P stays
  the screenshot key. Physical key codes keep WASD in place on other layouts.
- **Space = cut thrust:** clears the held thrust keys and releases hover/ZAMO station keeping (free fall). Thrust
  only acts while keys are held, so "cut thrust" is only meaningful for the automatic station-keeping thrust.
- **Thrust while station keeping:** the physics ignores user thrust in hover/ZAMO mode, so the first thrust key
  press switches to free flight (with a message). Otherwise W would silently do nothing.
- **Resets pause the simulation:** otherwise a radial infall preset would loop endlessly between the preset and
  the inner limit. The message says "Enter resumes".
- **Time warp unit:** M of proper time per wall-clock second (default 1, range 1e-3–1e3). The default thrust is
  0.01 c²/M, about 10× the hover acceleration at r = 30.
- **Viewer `r_max` = 200 M** (PRD §5.9, the f32 GPU range) instead of the physics default 1000. Scenes start at
  r ≤ 50.
- **Mouse look:** drag with the left button, not a captured cursor. Drags that start on the panel do not turn the
  view. Pitch is clamped to ±89°. Roll from the scene file is kept but there is no roll control.
- **Effect toggles** live in `ViewSettings` (they reset accumulation), initialised from the scene.
- **Applying a preset resets the look rotation** to "look at the hole", and ignores the scene's boost. Resets go to
  the unboosted preset.
- **Corrected test expectation:** while writing `hud_reports_station_keeping_and_motion` I first expected
  β = 0.5 for a circular orbit at r = 10. The correct value is `sqrt(M/(r − 2M)) = sqrt(1/8)`; the code was
  right. No existing test or tolerance changed. `accumulation_progresses_and_meters` was extended for the new
  `meter_exposure_now` signature.
- **New dependency:** `approx` as a dev-dependency of `gravography-app` (already used by the other crates). PRD
  unchanged.

**Tests**
- `cargo test --workspace`: **228 passed, 3 ignored** (was 208). The ignored tests (`test15_full_resolution`,
  `test16_larger`, `default_sky_bake_conserves_flux`) were run in release and pass. The scene refactor touches
  the path they use.
- New flight tests (20 in the app, 1 in render):
  - a Schwarzschild hover camera is unchanged after 600 frames (so the viewer converges), and τ = 10 M to 1e-12;
  - time warp, the frame-time clamp, NaN/negative frame times, pause, single step (exactly one per request);
  - a thrust key releases hover; felt acceleration = thrust to 1e-12; Δr after 5 M within [0.1, 0.2] M
    (½ a τ² ≈ 0.14);
  - thrust follows the look rotation (yaw 90°, pitch 30°) and W thrusts along the camera's forward leg to 1e-14;
  - yaw wrap and pitch clamp;
  - preset checks against r_E(θ), r_+, r_ph, r_isco, r_min and r_max, plus "every preset that passes the check
    can be placed" over a grid;
  - apply/refuse a preset;
  - a circular orbit at r = 10, a = 0.5 over ~3 orbits (3600 frames): radius to 1e-6;
  - radial infall r = 6: crosses the horizon (HUD region, no ZAMO speed), then resets at r_min, paused;
  - outer-limit reset;
  - spin change: kept for hover at r = 30; a ZAMO at r = 1.6 with a = 0.9 → a = 0 resets to the safe preset;
  - HUD: hover acceleration `M/(r² sqrt(1 − 2M/r))` to 1e-12; orbital β = sqrt(1/8) to 1e-12; a scene boost
    gives γ = 1/sqrt(1 − 0.81);
  - settings clamping; HUD text contains every FR-O4 item;
  - key mapping, diagonal normalisation, mouse sensitivity;
  - `boosted_placement_releases_station_keeping` (render).
- **Manual test checklist** (✔ = executed by the agent, how; **user** = needs the live window):
  1. ✔ (smoke) Launch, the default hover view converges, and the HUD shows "hovering (auto thrust)".
  2. ✔ (unit) W from hover switches to free flight and accelerates toward the hole. **user:** the feel of the
     thrust magnitude.
  3. ✔ (unit) Thrust follows the look direction. **user:** drag to look around; dragging on the panel does not turn
     the view.
  4. ✔ (unit) Space cuts station keeping and the observer falls.
  5. ✔ (unit) Enter pauses, `.` steps once, `[ ]` change the warp, `- =` change the thrust (clamped).
  6. ✔ (unit) Each preset applies. Invalid presets are refused with a reason, and the ISCO warning appears.
     **user:** the Apply button is disabled and the red/yellow text shows.
  7. ✔ (smoke + unit) Fall through the horizon: the region changes, the ZAMO speed shows "—", rendering inside
     works (screenshot), and the inner limit resets and pauses with a message.
  8. ✔ (smoke + unit) The spin slider works in flight (a = 0 → 0.5 while hovering, no reset). An invalid state
     resets to a safe preset.
  9. ✔ (unit) Effect toggles reset accumulation. The GPU path honours them (WP-09 tests). **user:** toggle them
     and watch.
  10. ✔ (unit) Outer limit at r = 200 resets.
  11. **user:** typing in a panel field does not thrust the camera. Switching windows while holding W does not
      leave the thrust stuck.
  12. ✔ (launch) `--scene scenes/kerr_edge_on.toml` ran for 10 s with no errors.

**Visual checks**
- Smoke test on the M2 Pro: hover r = 30 at 2560×1440 and 1920×1152 (as WP-10). Then, after a = 0.5 and infall
  from r = 8, a screenshot inside the horizon at r = 1.459 M (θ = 90°, dragged to BL φ = 11.2°), τ = 9.76 M.
  - **What it shows:** the shadow covers most of the 60° view. A thin blue-white, strongly blueshifted crescent
    of sky lies on the left edge. It matches a headless GPU render of the same preset at that radius, and
    4% of rays escape.
  - **An earlier run** reached r = 0.98 before pausing (frame timing differs between runs). There the
    escaping region was outside the 60° view and the image was entirely black. The headless render at the same r
    confirms the sky is only at the very edge or beyond, so this was physically expected, not a bug. The smoke test
    now prints the HUD (including ray rates and the exposure scale) to make this kind of check quick.
- No live-window inspection (screen capture is not permitted for the agent; see WP-10).

**Performance**
- Not measured systematically (WP-12). In the smoke test the HUD showed about 70 fps at 960×576 internal
  (render scale 0.5 of a 1920×1152 window) on the M2 Pro, with the view still and converged. The CPU observer step
  per frame is negligible.
- While moving, each frame is a 1-sample image at the internal resolution. Every ≥ 1 s there is an exposure and
  statistics readback, which causes a short hitch.

**Known issues / tech debt**
- **Full-screen lag (user report, 2026-10-30).** Entering full screen on macOS delivers a stream of
  `Resized` events during the animation. Each one reconfigures the surface, reallocates `GpuFrame` and restarts
  accumulation. The next redraw traces every pixel and, with auto exposure, did a blocking HDR readback before
  presenting.
  - **WP-11 does not change the resize path.** A resize is a "hard" restart, which re-meters on the next
    sample, so the lag is still there. The 1 s throttle only applies to camera moves.
  - **WP-12 fix:** debounce resizes (present the old image stretched until the size has been stable for about
    100 ms), and replace the blocking metering with a GPU log-average reduction.
- **Hitch while moving:** blocking exposure/statistics readbacks about once per second (see Performance).
- **No dynamic resolution yet:** while moving, the frame rate depends on the render scale (WP-12).
- **Smoke-test stop radius:** the infall stops at the first frame with r < 1.5, so the final radius depends on
  the frame rate (0.98–1.46 seen). This is fine for a smoke test. The assertion only requires "inside the
  horizon".
- **No roll control and no cursor capture:** these could be added in WP-13 polish if wanted.
- **`Flight` is not serialisable yet:** bookmarks (FR-O6, WP-13) need a mirror of `Observer` (see WP-04 notes)
  plus `LookSpec`, `FlightSettings` and the preset.

**Notes for the next agent (WP-12)**
- **Frame loop:** `Gfx::redraw` in `viewer/mod.rs`. The camera-change test is `self.rendered.is_none_or(|r|
  cam.differs(&r))`, then `restart(false)`. Dynamic resolution belongs in `restart`/`internal_size`: lower the
  scale while `update.advanced && moved`, restore it when still.
- **Per-frame allocations remain:** uniform buffers and bind groups in `render_sample` and `Presenter::draw`.
- **Throttled readbacks:** `state::METER_INTERVAL` and `Accumulation::meter_exposure_now`. Replace them with a GPU
  reduction, and keep the "meter at convergence" behaviour for screenshots.
- **Resize debounce:** see Known issues.
- **Flight tests:** `cargo test -p gravography-app` runs them in about 0.6 s.

## WP-10 — Viewer shell — 2026-10-30 — COMPLETE

(Started in one session, which was cut off by a usage limit after committing work in progress to the branch;
finished in the next.)

**Summary**
- **Viewer** (`gravography`, no subcommand; `crates/gravography-app/src/viewer/`):
  - winit 0.30 window and wgpu surface (format from `egui_wgpu::preferred_framebuffer_format`, `AutoVsync`);
  - progressive rendering of a fixed camera: one jittered sample per redraw via `GpuRenderer::render_sample`
    into a persistent `GpuFrame`, until the sample target is reached, then idle (`ControlFlow::Wait`);
  - present pass (tone-mapped accumulation buffer stretched over the window) with the egui panel on top;
  - resize reconfigures the surface and restarts accumulation;
  - auto exposure metered from an HDR readback after the first sample and at convergence.
- **Panel (egui 0.37):** sample target, render scale, auto exposure, EV, tone-map operator, screenshot button;
  status (GPU, fps, internal size, samples, exposure scale, last message).
- **Screenshots (FR-X1):** P or the button renders a converged frame at the full window resolution (`render`
  with the sample target) and writes `screenshots/gravography-<unix>-<n>.png` (GPU tone map) and `.exr`
  (linear HDR, exposed). They use the exposure shown on screen. `screenshots/` is gitignored.
- **Render crate:**
  - `GpuFrame::new` + `GpuRenderer::render_sample` for persistent per-frame buffers;
  - `Presenter` (`present.wgsl`);
  - tone-map operators shared in `tonemap_ops.wgsl`;
  - `GpuContext::{for_surface, from_adapter}`, plus an `adapter` field.
- **CLI:** `--scene`, `--screenshot-dir`, `--no-cache` and `--smoke-test`. The smoke test converges, takes a
  screenshot, asks for a 75%×80% window, waits for the size change, converges, takes a second screenshot,
  checks the PNG sizes and that the EXRs exist, then exits 0 (or fails after 120 s).

**Key files/modules**
- `crates/gravography-app/src/viewer/mod.rs`: `App` (ApplicationHandler), `Gfx` (surface, renderer, egui),
  redraw, resize, screenshot, smoke test.
- `crates/gravography-app/src/viewer/state.rs`: `ViewSettings`, `internal_size`, `Accumulation` (tested).
- `crates/gravography-app/src/viewer/ui.rs`: the egui panel.
- `crates/gravography-app/src/viewer/screenshot.rs`: file naming (tested).
- `crates/gravography-render/src/gpu/present.rs` and `shaders/present.wgsl`, `shaders/tonemap_ops.wgsl`.

**Decisions & deviations**
- **Render scale 0.5 by default:** a Retina window is 2560×1440 physical pixels. Dynamic resolution is WP-12.
- **Sample target:** at least 16 (from the scene's `samples`); each sample's footprint is `1/target` of a
  pixel, as in the headless renderer.
- **Exposure metering:** a blocking HDR readback at sample 1 and at convergence, instead of a GPU reduction.
  Fine for a fixed camera; WP-11/12 needs a GPU log-average for a moving camera.
- **Screenshot exposure:** the on-screen (internal-resolution) exposure, not re-metered at full resolution, so
  the file matches what was seen. Auto exposure is resolution-dependent (WP-06 known issue).
- **No viewport split:** the 3D view covers the whole window and the panel overlays its left side.
- **Default scene:** `Scene::default()`, a static observer at r = 30, Schwarzschild, auto exposure at 0 EV
  (brighter than the example scenes' −2 EV).
- **New app dependencies:** `winit` 0.30.13, `egui`, `egui-wgpu` and `egui-winit` 0.37 (all on the same wgpu
  30.0.1), `wgpu` and `pollster`. PRD unchanged.

**Tests**
- `cargo test --workspace`: 208 passed, 3 ignored. `test16_larger` was run for this WP and passes; the
  tracer is unchanged.
- **New:** `present_pass_matches_tonemap`. The present pass rendered into an offscreen Rgba8Unorm texture
  matches the GPU tone-map compute output to ±1 level. This verifies the window path headlessly.
- **New:** `Presenter` layout vs naga; present shader validation; viewer state (internal size, accumulation and
  metering, which settings reset); screenshot naming.

**Visual checks**
- **Smoke test:** `gravography --smoke-test --screenshot-dir target/screenshots/viewer` on an Apple M2 Pro /
  macOS:
  - the window opened and rendered at 2560×1440 physical pixels;
  - it was resized to 1920×1152 and re-rendered;
  - PNG + EXR screenshots were written at both sizes;
  - exit code 0, 11.5 s total.
- **Screenshot content:** the default-scene screenshot shows the Schwarzschild shadow at r = 30, the photon
  ring, the Einstein ring and the galactic band's secondary images. It is brighter than the example scenes
  (auto 0 EV).
- **Live window capture:** not possible from the agent session (`screencapture` reports "could not create image
  from display", probably missing Screen Recording permission). The on-screen path is covered by the present-pass
  test and the smoke test.
- **Normal launch:** `gravography --scene scenes/kerr_edge_on.toml` launched and ran without errors until it
  was terminated. **User:** please run `cargo run --release -p gravography-app` once to confirm the window and
  panel look right.

**Performance**
- Startup: sky from cache 0.1 s, then texture upload and pipeline creation; first frame well under the 3 s of
  NF-6.
- Smoke test: 4 samples at 1280×720 internal, plus two full-resolution (2560×1440, 1920×1152) 4-sample
  screenshots, 11.5 s in total including EXR writing (43 MB at full resolution).
- No fps measurements yet (WP-12).

**Known issues / tech debt**
- **Per-frame allocation:** `render_sample` and `Presenter::draw` create small uniform buffers and bind groups
  every frame. Fine for now; make them persistent in WP-12.
- **Blocking exposure readback:** auto exposure uses a blocking readback, which is not suitable for a moving
  camera (see Decisions).
- **No live window capture:** the agent could not inspect the live window image.
- **No progress indicator:** screenshots render synchronously, so the UI freezes for about a second.

**Notes for the next agent (WP-11)**
- **Camera and observer:** the camera comes from `ViewerOptions.scene` via `Gfx::frame_params`. For flight,
  keep an `Observer<f64>` in `Gfx`, advance it per frame (`Observer::advance`), build `FrameParams` from its
  position and tetrad (with the look rotation applied by `Tetrad::rotated`), and call `restart()` whenever the
  camera moves.
- **Panel:** add controls in `ui.rs`.
- **Input:** keyboard events already reach `window_event`. Check `response.consumed` so typing into egui does
  not move the camera.

## WP-09 — GPU shading & headless GPU render — 2026-10-30 — COMPLETE

**Summary**
- **Sky upload** (`GpuSky::upload`): the `(w, w·T)` cube map becomes a 6-layer RG32Float 2D-array texture with
  the full mip chain. The CPU data layout already matches layer and row order. The blackbody LUT goes into a
  storage buffer.
- **Shading pass** (`shade.wgsl` + `sky.wgsl`), per sample after the trace pass:
  - footprint from neighbouring G-buffer escape directions (±1 px, the CPU's fallback order, isotropic and
    unlensed fallbacks);
  - LOD = `½ log₂(Ω/Ω_texel)`, with manual bilinear/trilinear `textureLoad` sampling identical to the CPU's;
  - LUT lookup at `g·T`; beaming toggle; XYZ → linear sRGB; written to an accumulation buffer
    `(Σ rgb, count)`.
- **Tone mapping** on the GPU (`tonemap.wgsl`): ACES fitted and extended Reinhard, sRGB encoding, packed
  RGBA8. Matches `tonemap::to_srgb8` to ±1 level.
- **FR-R6 debug views** (`debug.wgsl`, `DebugView::{Status, Shift, Steps}`), drawn from the last sample's
  G-buffer.
- **`GpuRenderer`:** `render` (all samples, the same R2 jitter as the CPU), `read_hdr`, `read_stats`,
  `tonemap`, `debug_view`.
- **CLI:** `--backend gpu` (PNG via the GPU tone map; EXR from the HDR readback) and
  `--debug status|shift|steps`. The GPU device is created before the sky bake, so a missing adapter fails
  fast.
- **Shared structs:** `gbuffer.wgsl` holds the G-buffer and tile structs used by the trace, shade and debug
  shaders.

**Key files/modules**
- `crates/gravography-render/src/gpu/renderer.rs`: `GpuRenderer`, `GpuSky`, `GpuFrame`, `DebugView`,
  `GpuShadeParams`, `GpuToneParams`, `GpuDebugParams`.
- `crates/gravography-render/src/gpu/shaders/{gbuffer,sky,shade,tonemap,debug}.wgsl`: new modules.
- `crates/gravography-render/src/gpu/shader.rs`: `SHADE_MODULES`, `TONEMAP_MODULES`, `DEBUG_MODULES`.
- `crates/gravography-render/src/output.rs`: `write_rgb8_png`.
- `crates/gravography-render/tests/gpu_render.rs`: test 16, tone-map parity, uniform sky, debug views,
  determinism.
- `crates/gravography-app/src/main.rs`: GPU backend and debug views.

**Decisions & deviations**
- **Footprint from neighbours** (PRD §4.3), not extra rays: this is the CPU method with `ε = 1` px. The CPU
  reference keeps `ε = 0.25` px. Test 16 therefore checks against both:
  - the reference, with a documented perceptual/luminance tolerance;
  - the CPU with `ε = 1`, with tight bounds that isolate the f32 port.
- **Manual texture filtering with `textureLoad`** instead of hardware sampling with gradients (PRD wording:
  "cube-map sampling with gradients"). It reproduces the CPU sampling exactly (face-edge clamping, explicit LOD)
  and avoids `FLOAT32_FILTERABLE`. RG16F was rejected because `w·T` overflows f16. Hardware filtering could be
  revisited in WP-12 if profiling shows sampling matters.
- **Explicit bind-group layout for the shade pass:** the RG32Float texture must be bound as non-filterable.
  The other passes use auto layouts.
- **Exposure:** computed on the CPU from the HDR readback (`display_scale`), for determinism and CPU parity.
  A GPU log-average reduction is left for the viewer (WP-10/12).
- **Debug views** show the last sample's G-buffer. The status view counts in `--backend gpu` output are also
  for the last sample.
- No new dependencies. PRD unchanged.

**Tests**
- `cargo test --workspace`: 202 passed, 3 ignored (sky production bake, `test15_full_resolution`,
  `test16_larger`; both GPU ones were run for this WP and pass). The full suite takes about 62 s wall time,
  including the build.
- **Test 16** (`test16_gpu_images_match_cpu_reference`, 96×54, all four scenes). Worst values (scene):

  | Comparison | Total luminance | Mean relative | Pixels > 8 levels |
  |---|---|---|---|
  | vs reference | 0.37% (fast_observer) | 2.0% (inside_horizon) | 3.3% (inside_horizon) |
  | vs `ε = 1` CPU | 1.4e-4 | 1.1e-3 | 0.19% |

  - Bounds: reference 1% / 4% / 6%; `ε = 1` 1e-3 / 3e-3 / 0.5%.
  - Ignored `test16_larger` (192×108×2 spp): vs reference ≤ 0.46% / 1.3% / 1.8%; vs `ε = 1` ≤ 1.2e-4 / 3.5e-4
    / 0.07%.
- **`gpu_tonemap_matches_cpu`:** ±1 level for both operators at three exposures.
- **`uniform_sky_is_exact_on_gpu`:** a uniform sky gives `w·XYZ(gT)` with `g = (1 − 2/r)^{-1/2}` to 1e-5
  relative in every lit pixel. On its first run it failed because shadow-edge pixels with 2 samples were half
  lit; the test now uses 1 sample per pixel. The test was wrong, not the renderer.
- **`debug_views_render`:** status colours match G-buffer statuses exactly; the shift and steps views are
  non-uniform.
- **`gpu_render_is_deterministic`:** two GPU renders are bit-identical (NF-3).
- **Unit tests:** naga validation of all five shaders and their entry points; naga layouts of `ShadeParams`,
  `ToneParams` and `DebugParams`.

**Visual checks** (960×540, 4 spp, default sky, `--backend gpu`; `target/screenshots/*_gpu.png`)
- All four scenes are visually indistinguishable from the WP-06 CPU references:
  - same shadow sizes and positions;
  - the Kerr D-shape with photon-ring arcs on the flat side;
  - the inside-horizon blue-white rim;
  - the fast observer's forward blaze.
- Auto-exposure scales agree with the CPU to 1e-4 (e.g. 6.4174e-5 vs 6.4168e-5).
- **Debug views** (480×270):
  - status (kerr_edge_on): blue sky with a black D-shaped shadow;
  - steps (kerr_edge_on): the most steps hug the shadow edge, where near-critical rays orbit;
  - shift (fast_observer): blueshift peaks at the image centre (direction of motion), white ring at g ≈ 1,
    pink corners (g ≈ 0.8, matching the WP-06 probe), dark-grey shadow.

**Performance** (Apple M2 Pro, release, 960×540 × 4 spp, wall time including the sky texture upload and pipeline
compilation):
- schwarzschild_far 1.2 s; kerr_edge_on 1.5 s; inside_horizon 0.9 s; fast_observer 0.7 s;
- the CPU reference takes 35–75 s for the same images;
- mean GPU steps per ray 71 (Schwarzschild) to 118 (Kerr 0.99).
- No per-pass GPU timings yet (WP-12).

**Known issues / tech debt**
- **Per-call allocation:** `GpuRenderer::render` allocates the G-buffer, accumulation buffer and uniforms each
  call, and rebuilds bind groups per sample. That is fine headless; the viewer (WP-10/12) should keep them
  persistent and accumulate progressively across frames.
- **Sky upload cost:** uploading the 268 MB sky texture takes a noticeable part of the ~1 s.
- **Debug-view statistics:** debug views and the GPU statistics use only the last sample.
- **Exposure readback:** auto exposure needs an HDR readback. The viewer needs a GPU reduction.

**Notes for the next agent (WP-10)**
- **Viewer per frame:** reuse `GpuRenderer`. Per frame, call `tracer.trace(...)` with a jitter, then the shade
  pass with `sample_index` (0 resets accumulation). This keeps the accumulation buffer across static frames for
  progressive refinement (FR-R3, WP-12).
- **Presenting:** present by tone-mapping `GpuFrame.accum`. `tonemap.wgsl` writes packed RGBA8 to a buffer; for
  a surface, either copy it into a texture or write a fragment-shader variant that reuses `aces_fitted`,
  `reinhard_extended` and `srgb_encode`. If the swapchain format is `*Srgb`, skip `srgb_encode`.
- **Screenshot (FR-X1):** PNG via `GpuRenderer::tonemap` + `output::write_rgb8_png`, EXR via `read_hdr` +
  `write_image`.

## WP-08 — GPU ray tracer — 2026-10-30 — COMPLETE

**Summary**
- **DOPRI5 in WGSL** (`dopri5.wgsl`): step with FSAL, scaled RMS error norm and step controller, ported from
  `ode.rs` with the same tableau. `ray.wgsl` gains `trace_ray_dopri5`, which follows the Rust loop exactly:
  rejected steps count toward `max_steps`, steps at or below `h_min` are accepted, and an error check comes
  first. The termination block is factored into `ray_status_after_step`, which both WGSL tracers share. Its checks
  run in the same order as the Rust code.
- **Per-pixel trace pass** (`camera.wgsl`, `trace_pixels.wgsl`, `gpu/tracer.rs`):
  - the camera ray is generated on the GPU (rectilinear projection, f32 tetrad, `ks_g_lo`);
  - `g = 1/p_t`;
  - the lensing / frequency-shift effect flags behave like `CpuRenderer::cast`;
  - DOPRI5 trace → `GBufferTexel { dir_g, status, steps, rejected }`.
- **Tiling:** the image is dispatched in tiles (`GpuTracer::tile`, default 256²), one submission per tile, so
  no command buffer runs long enough to hit the watchdog. Results are bit-identical for any tile size (tested).
- **GPU defaults:** `TraceParams::gpu_default()` in the physics crate:
  - `rtol 1e-5`, `atol 1e-7`, `h_min 1e-6`, `max_steps 2000`;
  - momentum blow-up `10⁴` (see Decisions).
- **Test-ray DOPRI5:** new entry `trace_dopri5_main` and `TestRayPipeline::trace_dopri5`.
- **Parity helpers:** `frame_for(camera, side, fov)` and `near_status_boundary`.

**Key files/modules**
- `crates/gravography-render/src/gpu/shaders/dopri5.wgsl`, `camera.wgsl`, `trace_pixels.wgsl`: new modules.
- `crates/gravography-render/src/gpu/shaders/ray.wgsl`: DOPRI5 tracer, shared termination block.
- `crates/gravography-render/src/gpu/tracer.rs`: `GpuTracer`, `GBuffer`, `GBufferTexel`, `GpuCamera`, `GpuTile`.
- `crates/gravography-render/src/gpu/shader.rs`: `TRACE_PIXEL_MODULES`, `ALL_SHADERS`.
- `crates/gravography-physics/src/ray.rs`: `TraceParams::gpu_default` and the `GPU_*` constants.
- `crates/gravography-render/tests/gpu_tracer.rs`: test 15, tiling, effect flags, DOPRI5 test rays.

**Decisions & deviations**
- **GPU momentum blow-up factor `10⁴`** (CPU reference: `10⁸`). With `10⁸` the first test-15 run had 72
  misclassified pixels deep inside the shadow of the Schwarzschild `r = 1.2` camera. Those rays approach the
  white-hole horizon from inside, and `r − r_+ ≈ 1e-8` is unresolvable in f32. Measured over the parity set,
  escaping rays peak at `|p|/|p₀| ≤ 12` and blow-up captures reach `10⁸`, so `10⁴` classifies identically to the
  reference with an 800× margin. The classification logic is unchanged; only this parameter differs. Details
  are in `docs/physics.md` § "GPU tracer tolerances".
- **`rtol = 1e-5`:** `1e-4` gave identical agreement and only 3% fewer steps, because step caps dominate the
  step count. The tighter value costs almost nothing.
- **Test-15 boundary criterion:** a disagreeing pixel counts as "near the critical curve" if a pixel within
  2 px (Chebyshev distance) has a different status in the CPU reference map. No disagreements remain, so the
  criterion is not exercised.
- **G-buffer layout:** a storage buffer of 32-byte texels, rather than textures. WP-09 shading can read it
  directly. `dir_g.w` carries `g`; `rejected` is kept for diagnostics.
- **Per-tile resources:** each tile gets its own small uniform buffer and bind group (no `write_buffer` reuse
  between submissions). This is simple and the cost is negligible.
- No new dependencies. PRD unchanged.

**Tests**
- `cargo test --workspace`: 196 passed, 2 ignored (sky production bake; `test15_full_resolution`).
- **Test 15** (`test15_gpu_tracer_matches_cpu_reference`, 64²/camera, ~15 s):
  - 100% status agreement; 0 disagreements;
  - 99.97% within 1e-3 rad; p99 3.0e-5 rad;
  - g error ≤ 2.7e-6.
- **Ignored `test15_full_resolution`** (128²/camera, 163 840 rays, 63 s; run for this WP): 100% status
  agreement; 99.97% within 1e-3; p99 2.9e-5 rad; g ≤ 3.3e-6; no step-budget exhaustion.
- **`dopri5_test_rays_match_cpu_f32`:** GPU DOPRI5 vs CPU f32 DOPRI5, same inputs: 100% status agreement,
  p99 3.0e-5 rad.
- **`tiling_does_not_change_the_result`:** bit-identical G-buffers for tile sizes 256 and 24.
- **`lensing_off_and_no_frequency_shift`:** straight-ray directions within 1e-5 rad of the CPU; `g = 1` exactly.
- **Unit tests:** naga validation of both shaders and their entry points; every module is assembled; naga
  layouts of `Camera`, `Tile`, `GBufferTexel` and `TraceParams`; `near_status_boundary`.

**Visual checks**
- None: there is no GPU image output until WP-09 (shading).

**Performance**
- On an Apple M2 Pro, the 64² × 10-camera GPU trace is a negligible part of the test, which is dominated by the
  CPU f64 reference: about 41 k rays at rtol 1e-10.
- Mean GPU steps are ≈ 100 per ray (≈ 290 for the Kerr 0.998 interior camera).
- No GPU timings yet: timestamp queries are WP-12.

**Known issues / tech debt**
- **Persistent buffers:** `GpuTracer::trace` creates uniforms and bind groups per call. Make them persistent for
  the per-frame viewer path (WP-10/12).
- **Per-camera accuracy:** two cameras exceed 1e-3 rad at the 99th percentile on their own. At 128²:
  - the Schwarzschild interior, at 1.5e-3 rad, where only ≈ 3% of its pixels escape, all near the strongly
    blueshifted shadow edge;
  - the overall p99 is 2.9e-5 rad, and the PRD criterion applies to the whole set.
- **Rectilinear only:** the WGSL camera supports only the rectilinear projection, like the CPU.

**Notes for the next agent (WP-09)**
- **Shading:** read `GBuffer.buffer` in the shading pass: `dir_g.xyz` is the escape direction, `dir_g.w` is `g`,
  and `status` follows `cpu::status_index`. Port `CpuRenderer::footprint` from neighbouring G-buffer texels.
  The PRD asks for neighbours, not extra rays: the CPU uses ±0.25 px differential rays, so test 16 needs a
  documented tolerance for that difference. Then port `Sky::lod` and `Sky::shade`, and the tone mapping from
  `tonemap.rs`.
- **Trace settings:** use `TraceParams::<f32>::gpu_default()` for the GPU trace. `GpuTracer::trace` takes a
  jitter for progressive accumulation.
- **Before ending WP-09:** run the ignored `test15_full_resolution`
  (`cargo test -p gravography-render --test gpu_tracer -- --ignored`).

## WP-07 — GPU foundation — 2026-10-30 — COMPLETE

**Summary**
- **Headless device:** `GpuContext::new_headless()` uses the high-performance adapter with the adapter's own
  limits, and returns `Ok(None)` when there is no adapter. `GpuContext::for_tests` prints `SKIPPED <test>: no GPU
  adapter available`. It panics if an adapter exists but device creation fails, so that failure can't be hidden
  as a skip.
- **WGSL module assembly:** `gpu/shader.rs` holds `ShaderModule` constants (`include_str!`). `assemble`
  concatenates them in order with `// ---- module: <name> ----` headers.
- **WGSL ports** (`gpu/shaders/`): `common` (status codes, bit-level finiteness test), `kerr_schild`, `geodesic`,
  `rk4` and `ray` (RK4 `trace_ray` with the same step caps and termination order), plus `test_rays` (entry points
  `trace_rk4_main` and `eval_rhs_main`).
- **Test-ray pipeline:** `TestRayPipeline::{trace, eval_rhs}` uploads a batch of `(x, p)` rays, dispatches
  64-wide workgroups and reads the results back with a blocking map.
- **Parity harness** (`parity.rs`):
  - `parity_cameras()`: 10 cameras (Schwarzschild; Kerr 0.5/0.9/0.998; hover, ZAMO, orbit, infall inside the
    horizon, boosted 0.9c);
  - `camera_rays()`;
  - `RayOutcome` built from CPU f32/f64 or GPU results;
  - `compare()` → `Comparison` (confusion matrix, sorted angular errors, quantiles, `fraction_within`, `merge`,
    `summary`).
- **`Tetrad` reuse:** the harness uses `Tetrad::boosted` (WP-06) for moving cameras.

**Key files/modules**
- `crates/gravography-render/src/gpu/context.rs`: `GpuContext`.
- `crates/gravography-render/src/gpu/shader.rs`: module list and assembly, plus naga validation tests.
- `crates/gravography-render/src/gpu/test_rays.rs`: `GpuTraceParams`, `GpuRay`, `GpuRayResult`, `GpuPhase`
  (`#[repr(C)]` Pod) and `TestRayPipeline`.
- `crates/gravography-render/src/gpu/shaders/*.wgsl`: the WGSL ports (mapping table in `docs/physics.md` § "GPU
  port (f32)").
- `crates/gravography-render/src/parity.rs`: the harness.
- `crates/gravography-render/src/cpu.rs`: new `status_from_index`.
- `crates/gravography-render/tests/gpu_parity.rs`: GPU acceptance tests.

**Decisions & deviations**
- **Crate placement:** the GPU code lives in `gravography-render` as the PRD describes (no feature flag), so
  `wgpu` is a normal dependency of the render crate and the app.
- **Uniform/storage layout:** params go in a 64-byte uniform; rays, results and RHS outputs go in storage arrays.
  Layouts are checked twice: by Rust size/offset assertions, and against naga's computed WGSL layout
  (`layouts_match_naga`).
- **Pipeline layout:** auto-derived per entry point (`layout: None`). Each pipeline gets its own bind group from
  `get_bind_group_layout(0)`, because unused bindings are absent from derived layouts.
- **Matching inputs:** the GPU and CPU f32 paths start from identical f32 inputs (f64 camera rays rounded once).
  `r_+` is computed in f32 on the CPU and uploaded.
- **RK4 step budget:** `max_steps = 20 000` for the RK4 batch (never reached; keeps dispatches short).
- New dependencies:
  - `wgpu` 30 (GPU);
  - `bytemuck` with `derive` (Pod structs);
  - `pollster` (blocking on adapter/device futures; tiny, no transitive deps).
  - naga comes via `wgpu::naga` for the WGSL validation tests; it is not a separate dependency.

**Tests**
- `cargo test --workspace`: 190 passed, 1 ignored (sky production bake; sky code untouched). New: 11 render
  unit tests, 2 GPU integration tests.
- **WP-07 acceptance (`rk4_gpu_vs_cpu_f32_vs_cpu_f64`):** 10,240 rays, Apple M2 Pro (Metal), all three pairs:

  | Pair | Status agreement | p50 angle | p99 angle | Max angle |
  |---|---|---|---|---|
  | GPU vs CPU-f32 | 100% | 1.3e-7 rad | 8.4e-5 rad | 1.4e-3 rad |
  | GPU vs CPU-f64 | 100% | 1.9e-7 rad | 6.1e-5 rad | 1.6e-3 rad |
  | CPU-f32 vs CPU-f64 | 100% | 1.4e-7 rad | 4.4e-5 rad | 2.8e-4 rad |

  - The largest errors are near-critical rays of the `r = 4` prograde orbiter.
  - Assertions: PRD test 15 thresholds (≥ 99.5% status; ≥ 99% of rays within 1e-3 rad) for every pair.
  - The per-camera table is printed with `--nocapture`.
- **`gpu_rhs_matches_cpu_f32`:** GPU Hamiltonian RHS vs CPU f32 at points along every camera's rays: max 5.5e-7
  of the natural scale (≈ 5 ulp); tolerance 1e-5.
- **Unit tests:**
  - WGSL validates in naga;
  - assembly order and headers; every `.wgsl` file is assembled;
  - struct layouts (Rust and naga);
  - params conversion; status-code round trip;
  - comparison statistics; precise small-angle `angle_between`;
  - all parity cameras valid and their rays null; the CPU-only f32 vs f64 harness runs.

**Visual checks**
- None: there is no GPU image output in this WP. The WP-06 reference renders are unchanged.

**Performance**
- Both GPU tests together take ≈ 7 s, mostly the CPU f64/f32 traces. The GPU dispatch of 1024 RK4 rays takes
  milliseconds. Full suite ≈ 47 s wall time including the build.

**Known issues / tech debt**
- **`for_tests` in the library:** `GpuContext::for_tests` sits in library code (used by integration tests), and
  panics on device-creation failure by design.
- **One pipeline per call:** the test-ray pipeline creates buffers per call. Fine for tests; the WP-08 renderer
  should keep persistent buffers.
- **Metal only:** only measured on Metal (M2 Pro). Other backends should pass the same thresholds but are
  untested.

**Notes for the next agent (WP-08)**
- **Adding DOPRI5:** add a `dopri5.wgsl` module ported from `ode.rs::{dopri5_step, error_norm,
  StepController}`, and a DOPRI5 branch of `trace_ray` in `ray.wgsl` following the Rust loop exactly. That
  covers rejected steps, `h_min` acceptance, FSAL `k_last` and `steps + rejected >= max_steps`.
- **Extending the assembly:** append new modules to a new module list in `shader.rs`. The
  `every_module_is_listed_once` test needs each `.wgsl` file to be in some assembly, so update it to check the
  union of lists.
- **Reusing the harness:** `parity.rs` is ready for test 15. Swap the RK4 batch for the DOPRI5 per-pixel tracer
  and compare against CPU f64 DOPRI5 (`TraceParams::cpu_reference`) with the GPU tolerances
  (`docs/PRD.md` §5.5: rtol ~1e-5–1e-4). Expect status disagreements only near the critical curve.
- **Pitfalls:**
  - dynamic indexing into a `let` array in WGSL is not portable. Use `var`, as `geodesic.wgsl` does for
    `dl_lo`.
  - WGSL has no `isnan`; use `is_finite_f32`.
  - Map buffers with `PollType::wait_indefinitely()`.
  - wgpu 30 `RequestAdapterOptions` has an `apply_limit_buckets` field; use `..Default::default()`.

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
