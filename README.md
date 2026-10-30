# Gravography

Real-time interactive viewer that ray traces null geodesics in Schwarzschild and Kerr spacetimes, rendering the
gravitationally lensed, aberrated and Doppler/gravitationally shifted sky seen by an observer near a black hole.
Written in Rust with `wgpu`; macOS (Apple Silicon) is the primary platform.

**Status:** early development. The CPU physics (geodesics, observers) and the sky model (colour science, star
field, cube-map baking) are implemented and tested; there is no renderer or viewer yet. See `docs/PRD.md` for the specification and
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

Requires Rust 1.99 or newer (edition 2024); the pinned version is in `rust-toolchain.toml`.

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
