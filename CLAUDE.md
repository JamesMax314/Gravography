# CLAUDE.md — Gravography

Gravography is an open-source, real-time interactive viewer that ray traces null geodesics in Schwarzschild and
Kerr spacetimes (Rust + wgpu, macOS primary). The full specification is in `docs/PRD.md`.

You are one of a series of agent instances. Each instance completes **exactly one work package (WP)** from
`docs/PRD.md` §10, hands off via `docs/progress_report.md`, and stops. The next instance starts with a fresh
context and knows only what is in the repository, so write everything the next agent needs into the repo.

---

## 1. Session start protocol (do this first, every session)

1. Read this file, then `docs/PRD.md`, then `docs/progress_report.md` (start with its "Current state" section and the most recent entry).
2. Run `git status`, `git log --oneline -15` and `git tag`. The working tree should be clean on `main`. If it is
   not, work out from the progress report what happened; if it is still unclear, stop and ask the user.
3. Identify the next WP (the "Next work package" in the progress report; otherwise the first WP not marked done).
4. Run `cargo test --workspace` to confirm the baseline is green before changing anything. If the baseline is
   red, fixing it is your first job; record it in the report.
5. Create the branch: `git switch -c wp-XX-short-name`.

Exception for **WP-00**: there is no repository or Cargo workspace yet. Run `git init -b main`, build the
skeleton, make the initial commit directly on `main` (including `CLAUDE.md` and `docs/`), and tag `wp-00`.
Skip steps 2, 4 and 5 above, and skip the merge in §2.

## 2. Session end protocol (do this last, every session)

1. All checks pass:
   ```bash
   cargo fmt --all --check
   cargo clippy --workspace --all-targets -- -D warnings
   cargo test --workspace
   ```
2. Update `docs/progress_report.md` (template in §7): rewrite "Current state", tick the WP in the status table,
   add a new entry at the top of the log.
3. Update `docs/physics.md` if you implemented or changed any physics, and `README.md` if user-facing behaviour changed.
4. Commit, merge and tag:
   ```bash
   git switch main
   git merge --no-ff wp-XX-short-name -m "Merge WP-XX: <title>"
   git tag wp-XX
   ```
5. Stop. Tell the user, in a few lines, what was completed, anything that needs their attention, and the next WP.
   **Do not start the next WP**, even if you have context left.

If you cannot finish the WP (blocked, or the WP is clearly too large), commit the work in progress to the branch
(do not merge broken work to `main`), write a progress entry marked **INCOMPLETE** explaining exactly where you
stopped and what remains, then stop and tell the user. If a WP is too large, propose a split in the report.

## 3. Git rules

- Never push, force-push, rewrite history on `main`, or delete tags. The user handles remotes.
- Small, focused commits on the WP branch. Message format: `<area>: <imperative summary>`
  (e.g. `physics: add analytic inverse-metric derivatives`). No CI is used, so run the checks yourself before
  committing.
- Never commit generated output (`target/`, screenshots, baked sky caches). Keep them gitignored.
- End commit messages with the attribution trailer the user's environment specifies (if any).

## 4. Testing requirements (mandatory)

- **Every function containing physics, maths or colour science gets unit tests**, in a `#[cfg(test)] mod tests`
  in the same file. Integration/validation tests go in `crates/<crate>/tests/`.
- Validate against **analytic results** wherever possible (`docs/PRD.md` §9 lists the minimum set and which WP
  owns each test). Cite the formula source in a comment on the test.
- Use property tests (`proptest`) for identities that must hold everywhere (e.g. `g_lo · g_up = 𝟙`, `det g = −1`,
  tetrad orthonormality).
- Every tolerance is explicit, named, and justified in a comment (e.g. `// DOPRI5 rtol=1e-10 over ~1e3 steps`).
  Compare floats with `approx`, never `==`.
- Tests are deterministic: fixed seeds, no wall-clock dependence.
- **Never weaken, skip or delete a failing test to make the suite pass** unless the test itself is wrong. If you
  change a test or tolerance, explain why in the progress report under "Decisions & deviations".
- GPU tests: they need an adapter. If none is available, they must skip with a clear printed message rather than
  fail. On the user's Mac they must run, and they count towards the suite.
- Write a test that reproduces any bug before fixing it.
- Keep `cargo test --workspace` reasonably fast (well under ~2 min). Put slow validation behind `#[ignore]` with
  a comment, and run ignored tests (`cargo test --workspace -- --ignored`) before ending any WP that touches the
  code they cover.

## 5. Physics conventions (normative; details in `docs/PRD.md` §2 and §5)

- Units `G = c = M = 1`. Spin `a ∈ [0, 0.999]`, spin axis `+z`.
- Signature `(−, +, +, +)`. Coordinates: ingoing Cartesian Kerr–Schild `(t, x, y, z)`, index 0 = `t`.
  Boyer–Lindquist only for formulas, presets, tests and HUD.
- Rays use covariant momentum `p_μ` and `H = ½ g^{μν} p_μ p_ν`. They are traced **backward** (past-directed).
- Naming: `g_lo` (`g_{μν}`), `g_up` (`g^{μν}`), `p_lo`, `u_up`, `e_up[a][μ]` for tetrad legs.
- Precision: all CPU physics is f64. The GPU is f32 only (Apple GPUs have no f64). The physics crate stays
  generic over the scalar type so f32 behaviour can be reproduced on the CPU.
- Any equation implemented in code carries a comment pointing to its derivation in `docs/physics.md` or a
  reference in `docs/PRD.md` Appendix B. When you derive something new, write the derivation in `docs/physics.md`.
- The CPU f64 implementation is the ground truth. GPU code must match it within the parity tolerances
  (`docs/PRD.md` §9). When WGSL ports a function, name it after its Rust counterpart and keep the same structure.

## 6. Rust & code conventions

- Edition 2024; toolchain pinned in `rust-toolchain.toml`. Add dependencies with `cargo add` (latest compatible
  versions) and keep them minimal; justify each new dependency in the progress report.
- Respect the crate boundaries in `docs/PRD.md` §4 (`physics` ← `sky` ← `render` ← `app`; `physics` has no GPU deps).
- Libraries use `thiserror` error types; the binary uses `anyhow`. No `unwrap()` in library code outside tests;
  use `expect("reason")` only for true invariants.
- No `unsafe` unless unavoidable. If you need it, add a `// SAFETY:` comment and mention it in the report.
- Document every public item with `///`, including units and conventions where relevant.
- WGSL shaders live in `crates/gravography-render/src/gpu/shaders/` as `.wgsl` modules, assembled by ordered
  concatenation. GPU structs are `#[repr(C)]` + `bytemuck::Pod`, with WGSL alignment rules respected and tested
  (size/offset assertions).
- Do not optimise before WP-12 beyond avoiding obviously wasteful designs. Correctness first.

## 7. `docs/progress_report.md` format

Keep the file structured as follows (the template already exists; preserve its headings):

1. **Current state:** rewritten every session. What works, what does not, the next work package, and any open
   questions for the user.
2. **Work package status table:** one row per WP with its status and tag.
3. **Log:** newest entry first. Each entry contains:
   - `## WP-XX — <title> — <YYYY-MM-DD> — COMPLETE | INCOMPLETE`
   - **Summary:** what was built (a few bullets).
   - **Key files/modules:** paths, with one line each.
   - **Decisions & deviations:** design choices and why; any deviation from the PRD; any changed tests or tolerances.
   - **Tests:** counts (passed/ignored), new validation tests, tolerances achieved.
   - **Visual checks:** renders inspected, and what they showed (from WP-06 on).
   - **Performance:** measurements, where relevant (machine, resolution, settings, ms/frame).
   - **Known issues / tech debt.**
   - **Notes for the next agent:** pitfalls, gotchas, where to start.

Be concise but complete: the next agent has no other memory of your session.

## 8. Scope discipline

- Implement only the current WP. If you notice needed work outside it, record it under "Known issues" or
  "Notes for the next agent" instead of doing it, unless it blocks the current WP.
- Do not change the scope of `docs/PRD.md`. Small clarifications or corrections (e.g. a formula typo) are
  allowed; record each one in the progress report. Anything bigger: propose it in the report and ask the user.
- v2 features (`docs/PRD.md` §11) are out of scope for v1, but keep the extension points the PRD asks for.

## 9. Visual verification

From WP-06 on, use the headless renderer to check your work visually:

```bash
cargo run --release -p gravography-app -- render --scene scenes/<name>.toml --out target/screenshots/<name>.png
```

Open the PNG with your file-reading tool and inspect it: shadow size and shape, Einstein rings, asymmetry for
Kerr, colour shifts. Describe what you saw in the progress report. Expected appearance is sketched in the
references (Luminet 1979; James et al. 2015). For the interactive viewer (WP-10 on), launch it with
`cargo run --release -p gravography-app` to confirm it opens and renders, but prefer scripted/headless checks for
anything that can be automated.

## 10. Useful commands

```bash
cargo build --workspace
cargo test --workspace
cargo test -p gravography-physics <filter>
cargo test --workspace -- --ignored
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all
cargo run --release -p gravography-app
```
