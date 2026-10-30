# Gravography — Progress Report

Hand-off log between agent sessions. Format and rules: `CLAUDE.md` §7. Newest log entry first.

---

## Current state

- Workspace skeleton in place (WP-00): four empty crates (`gravography-physics`, `-sky`, `-render`, `-app`
  with binary `gravography`), workspace lints, licences, doc stubs. Build, test, fmt and clippy are clean.
  No physics or rendering yet.
- **Next work package:** WP-01 — Metric core.
- Open questions for the user:
  - The local `cargo fmt` resolves to a stale crates.io `rustfmt`/`cargo-fmt` in `~/.cargo/bin`, which shadows
    Homebrew's working one. Agents should run checks with `PATH=/opt/homebrew/bin:$PATH` (see the WP-00 notes).
    You may want to delete `~/.cargo/bin/rustfmt` and `~/.cargo/bin/cargo-fmt`.
  - Toolchain is pinned to the installed Homebrew Rust 1.87.0 (see the WP-00 decisions). Bump it if you install rustup.

---

## Work package status

| WP | Title | Status | Tag |
|---|---|---|---|
| WP-00 | Repository bootstrap | done | `wp-00` |
| WP-01 | Metric core | not started | |
| WP-02 | Derivatives, Hamiltonian, invariants | not started | |
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
