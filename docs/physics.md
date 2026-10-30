# Gravography — Physics notes

Derivations and conventions behind the code. Every equation implemented in code points here or to a reference
in `docs/PRD.md` Appendix B (`CLAUDE.md` §5). Maintained by the agent that implements or changes the physics.

## Conventions

Normative conventions are in `docs/PRD.md` §2: units `G = c = M = 1`, signature `(−, +, +, +)`, ingoing
Cartesian Kerr–Schild coordinates `(t, x, y, z)` (index 0 = `t`), covariant ray momenta `p_μ`, Hamiltonian
`H = ½ g^{μν} p_μ p_ν`, rays traced backward in time, `g = ν_obs / ν_emit`.

Code keeps a general mass `M` (`Kerr::with_mass`) so that mass scaling can be tested, but the application always
uses `M = 1`.

## Kerr–Schild metric

Code: `crates/gravography-physics/src/kerr_schild.rs`.

Spheroidal coordinates `(r, θ, φ̃)` are related to Cartesian KS coordinates by

    x + i y = (r + i a) sinθ e^{iφ̃},   z = r cosθ,   i.e.
    x = sinθ (r cosφ̃ − a sinφ̃),   y = sinθ (r sinφ̃ + a cosφ̃).

The metric is `g_{μν} = η_{μν} + f l_μ l_ν` with (Visser 2007)

    f   = 2 M r³ / (r⁴ + a² z²)  = 2 M r / Σ,         Σ = r² + a² cos²θ,
    l_μ = (1, (r x + a y)/(r² + a²), (r y − a x)/(r² + a²), z/r).

Because `l` is null with respect to `η` (`−1 + sin²θ cos²φ̃ + sin²θ sin²φ̃ + cos²θ = 0`, using the identities
below), the inverse is exactly `g^{μν} = η^{μν} − f l^μ l^ν` with `l^μ = η^{μν} l_ν`. Also `det g = det η = −1`.

**Spatial part of `l`.** Substituting the spheroidal form:
`r x + a y = sinθ cosφ̃ (r² + a²)` and `r y − a x = sinθ sinφ̃ (r² + a²)`, so
`l_i = (sinθ cosφ̃, sinθ sinφ̃, cosθ)`: a unit Euclidean vector. In spheroidal coordinates,

    l_μ dx^μ = dt + sinθ cosφ̃ dx + sinθ sinφ̃ dy + cosθ dz = dt + dr − a sin²θ dφ̃.

(Use `sinθ Re(e^{−iφ̃} d(x + iy)) = sin²θ dr + r sinθ cosθ dθ − a sin²θ dφ̃`, then add `cosθ dz`.)
This is the standard ingoing Kerr null congruence `l = dv − a sin²θ dφ̃` with `v = t + r`.

### Cancellation-free `r`

`r` is the positive root of `r⁴ − (ρ² − a²) r² − a² z² = 0`, `ρ² = x² + y² + z²`. With
`w = ½(ρ² − a²)` and `s = sqrt(w² + a² z²)`:

    r² = w + s                  if w ≥ 0,
    r² = a² z² / (s − w)        if w < 0        (same value: (s + w)(s − w) = a² z²).

For `w < 0` (inside the oblate shell `ρ < a`) the naive `w + s` subtracts nearly equal numbers. The second form
has a denominator `s − w ≥ 2|w| > 0`.

Conditioning: since `ρ² − a² = r² − a² cos²θ`, the inputs `x, y` (of size `~a`) determine `w` only to an
absolute `~ε a²`. The relative accuracy of `r` is therefore limited to `~ε a² / (r² + a² cos²θ)` by the
problem itself, near `θ = π/2` with `r ≪ a` (close to the disk). The test `radius_is_spheroidal_root` uses that
condition number as its tolerance.

### Behaviour at r = 0

`r = 0` is the disk `z = 0, x² + y² ≤ a²`. The ring singularity is its rim. For `a = 0` it is the central point.
Approaching the disk interior, `f = 2Mr/(r² + a² cos²θ) → 0`. `ks_fields` therefore returns `f = 0` (metric
`= η`) at exactly `r = 0` instead of the NaN of `0/0`. This is wrong only *at* the singularity (a set of
measure zero). Ray and observer code must detect singular approach through `r` (`docs/PRD.md` §5.6) and not
rely on the metric blowing up.

## Boyer–Lindquist ↔ Kerr–Schild

Code: `crates/gravography-physics/src/boyer_lindquist.rs`.

Ingoing Kerr coordinates follow from BL by (Visser 2007)

    dv = dt_BL + (r² + a²)/Δ dr,     dφ̃ = dφ_BL + a/Δ dr,     Δ = r² − 2Mr + a²,

and KS time is `t_KS = v − r`. Hence

    t_KS = t_BL + T(r),   T'(r) = (r² + a²)/Δ − 1 = 2Mr/Δ,
    φ̃   = φ_BL + Φ(r),   Φ'(r) = a/Δ.

With `Δ = (r − r_+)(r − r_−)` and partial fractions,

    T(r) = (2M/(r_+ − r_−)) [ r_+ ln|r − r_+| − r_− ln|r − r_−| ],
    Φ(r) = (a/(r_+ − r_−)) ln| (r − r_+)/(r − r_−) |.

The integration constants are chosen as zero. For `a = 0` this gives `T = 2M ln|r − 2M|` (ingoing
Eddington–Finkelstein) and `Φ = 0`. The absolute values let the same expressions serve each BL patch
(`r > r_+`, `r_− < r < r_+`, `0 < r < r_−`). BL is singular on the horizons, where `T` and `Φ` diverge.

**Inverse.** `r` from the KS radius; `θ = arccos(z/r)`; `arg(x + iy) = φ̃ + arg(r + ia)`, so
`φ̃ = atan2(y, x) − atan2(a, r)`; then subtract the offsets. `φ` is wrapped into `(−π, π]`.

**Jacobian** `J^μ_ν = ∂x_KS^μ/∂x_BL^ν` (rows `t, x, y, z`; columns `t, r, θ, φ`). Using `∂x/∂φ̃ = −y`,
`∂y/∂φ̃ = x` and `∂φ̃/∂r = Φ'`:

    ∂t_KS/∂r = 2Mr/Δ
    ∂x/∂r = sinθ cosφ̃ − y a/Δ,   ∂x/∂θ = cosθ (r cosφ̃ − a sinφ̃),   ∂x/∂φ = −y
    ∂y/∂r = sinθ sinφ̃ + x a/Δ,   ∂y/∂θ = cosθ (r sinφ̃ + a cosφ̃),   ∂y/∂φ =  x
    ∂z/∂r = cosθ,                 ∂z/∂θ = −r sinθ,                    ∂z/∂φ =  0

Vectors: `v_KS = J v_BL`. Covectors: `p_BL = Jᵀ p_KS`, so `p_KS = J⁻ᵀ p_BL`. `J⁻¹` is computed numerically (4×4
adjugate). It does not exist on the axis (`∂/∂φ` column vanishes) or on a horizon.

**BL metric** (for tests and presets): `g_tt = −(1 − 2Mr/Σ)`, `g_tφ = −2Mar sin²θ/Σ`, `g_rr = Σ/Δ`,
`g_θθ = Σ`, `g_φφ = (r² + a² + 2Ma²r sin²θ/Σ) sin²θ`.

**Consistency checks (tests).** `g_BL = Jᵀ g_KS J` at random events (checks the metric, the map and the sign of
`a` together). The ZAMO, defined by `p_BL ∝ (−1, 0, 0, 0)`, is transformed to KS, raised with `g^{μν}_KS` and
transformed back. It gives `dφ/dt = ω = 2Mar/((r² + a²)² − Δ a² sin²θ) > 0` for `a > 0`: the hole rotates in
the `+φ` sense (counter-clockwise seen from `+z`), as `docs/PRD.md` §2 requires.

## Characteristic radii

Code: `crates/gravography-physics/src/kerr.rs` (formulas: `docs/PRD.md` Appendix A; Bardeen, Press &
Teukolsky 1972). With `χ = a/M`:

- Horizons `r_± = M ± sqrt(M² − a²)` (roots of `Δ`).
- Ergosurface `r_E(θ) = M + sqrt(M² − a² cos²θ)` (root of BL `g_tt`).
- Equatorial photon orbits `r_ph = 2M {1 + cos[(2/3) arccos(∓χ)]}`. Tested against the circular-orbit
  energy pole `r^{3/2} − 3M r^{1/2} ± 2a M^{1/2} = 0` (BPT 1972).
- ISCO via `Z₁, Z₂` (BPT 1972). Tested against marginal stability
  `r² − 6Mr ± 8a sqrt(Mr) − 3a² = 0` (BPT 1972).

Upper sign = prograde throughout.

## Derivatives of the Kerr–Schild fields

Code: `ks_field_derivatives`, `ks_dg_lo`, `ks_dg_up` in `crates/gravography-physics/src/kerr_schild.rs`.
All fields are independent of `t`, so every `∂_t` vanishes. Let `D = r⁴ + a² z²` and `q = r² + a²`.

**`∂_i r`.** Implicit differentiation of `F(r, x) = r⁴ − (ρ² − a²) r² − a² z² = 0`:
`∂_i r = −F_{,i} / F_{,r}` with `F_{,r} = 4r³ − 2(ρ² − a²) r`, `F_{,x} = −2x r²`, `F_{,y} = −2y r²`,
`F_{,z} = −2z r² − 2a² z`. On the root, `ρ² − a² = r² − a² z²/r²`, so `F_{,r} = 2 D / r`, and

    ∂_x r = x r³ / D,    ∂_y r = y r³ / D,    ∂_z r = z r (r² + a²) / D.

(For `a = 0`: `∂_i r = x_i / r`.)

**`∂_i f`.** `f = 2M r³ / D` gives, with `∂_i D = 4 r³ ∂_i r + 2 a² z δ_iz`,

    ∂_i f = f [ 3 ∂_i r / r − (4 r³ ∂_i r + 2 a² z δ_iz) / D ].

**`∂_j l_μ`.** `l_t = 1` is constant. From `l_x = (r x + a y)/q` and `l_y = (r y − a x)/q` with `∂_j q = 2r ∂_j r`:

    ∂_j l_x = (x ∂_j r + r δ_jx + a δ_jy − 2 r l_x ∂_j r) / q,
    ∂_j l_y = (y ∂_j r + r δ_jy − a δ_jx − 2 r l_y ∂_j r) / q,
    ∂_j l_z = δ_jz / r − z ∂_j r / r².

Since `l^i = l_i` and `l^t = −1`, `∂_j l^i = ∂_j l_i` and `∂_j l^t = 0`.

**Metric.** `∂_α g_{μν} = ∂_α f l_μ l_ν + f (∂_α l_μ l_ν + l_μ ∂_α l_ν)`, and `∂_α g^{μν}` is the same with `l^μ`
and an overall minus sign.

At `r = 0` (the disk) all derivatives are returned as zero, consistent with `f = 0` there (§ "Behaviour at
r = 0"). The disk is a branch surface of `r(x, y, z)`, across which `∂_z f` is discontinuous.

**Validation.** Richardson-extrapolated central differences (`O(h⁴)`, `h = 10⁻³ r`) agree with every analytic
derivative to `10⁻⁹` of the natural scale. Also checked: `∂g^{μν} = −g^{μρ} ∂g_{ρσ} g^{σν}`. A deliberately
introduced sign error in `∂l_y` is caught by all three derivative tests (mutation check during WP-02).

## Christoffel symbols

Code: `crates/gravography-physics/src/connection.rs`.

    Γ_{ναβ} = ½ (∂_α g_{νβ} + ∂_β g_{να} − ∂_ν g_{αβ}),     Γ^μ_{αβ} = g^{μν} Γ_{ναβ}.

Tested: symmetry in `αβ`; metric compatibility `∂_γ g_{αβ} = Γ^λ_{γα} g_{λβ} + Γ^λ_{γβ} g_{αλ}`; the acceleration
`M / (r² sqrt(1 − 2M/r))` of a Schwarzschild static observer; and agreement with the Hamiltonian flow (below).
Used for observer transport (WP-04). Rays use the Hamiltonian form.

## Hamiltonian right-hand side

Code: `crates/gravography-physics/src/geodesic.rs`.

With `g^{μν} = η^{μν} − f l^μ l^ν` and `(l·p) = l^μ p_μ = −p_t + l_x p_x + l_y p_y + l_z p_z`:

    H        = ½ [ η^{μν} p_μ p_ν − f (l·p)² ],
    dx^μ/dλ  = ∂H/∂p_μ  = η^{μν} p_ν − f (l·p) l^μ,
    dp_μ/dλ  = −∂H/∂x^μ = ½ ∂_μ f (l·p)² + f (l·p) (∂_μ l^ν) p_ν.

This needs only the field gradients (3 + 3 + 9 numbers), never a rank-3 tensor, so it is the form to port to
WGSL. `dp_t/dλ = 0` holds exactly, because `∂_t f = ∂_t l = 0`.

Tested against: the tensor form `−½ ∂_μ g^{αβ} p_α p_β`; finite differences of `H` itself (independent of the
analytic derivatives); and the geodesic equation. With `u^μ = g^{μν} p_ν`,
`du^μ/dλ = (∂_α g^{μν}) u^α p_ν + g^{μν} ṗ_ν` must equal `−Γ^μ_{αβ} u^α u^β`.

## Conserved quantities

Code: `crates/gravography-physics/src/invariants.rs`.

- `E = −p_t`. `∂_t` is the same Killing field in KS and BL, because `t_KS − t_BL` depends on `r` only.
- `L_z = x p_y − y p_x`. At fixed `(t, r, θ)` the Killing field `∂_φ̃` is `∂x/∂φ̃ ∂_x + ∂y/∂φ̃ ∂_y = −y ∂_x + x ∂_y`.
  BL `φ` and `φ̃` differ by a function of `r`, so `p_φ` (BL) `= p_φ̃ = L_z`.
- `H`, with `μ² = −2H` (`0` for photons, `1` for unit-mass particles).
- Carter `Q = p_θ² + cos²θ (a² (μ² − E²) + L_z² / sin²θ)` (Carter 1968; Bardeen 1973).

### Carter constant in Cartesian form

`p_θ` is the same in BL and ingoing KS spheroidal coordinates (the coordinates differ only by functions of `r`).
From `x = sinθ (r cosφ̃ − a sinφ̃)`, `y = sinθ (r sinφ̃ + a cosφ̃)`, `z = r cosθ` at fixed `r, φ̃`:
`∂x/∂θ = x cotθ`, `∂y/∂θ = y cotθ`, `∂z/∂θ = −r sinθ`, so

    p_θ = cotθ (x p_x + y p_y) − r sinθ p_z.

Taken alone, this and `L_z²/sin²θ` are singular on the axis. The combination is not. With the Lagrange identity
`(x p_y − y p_x)² + (x p_x + y p_y)² = (x² + y²)(p_x² + p_y²)` and `x² + y² = sin²θ (r² + a²)`:

    p_θ² + cot²θ L_z² = cos²θ (r² + a²)(p_x² + p_y²) − 2 z (x p_x + y p_y) p_z + r² sin²θ p_z²,

with `cos²θ = z²/r²` and `sin²θ = 1 − cos²θ`. `Q` is this plus `cos²θ a² (μ² − E²)`. It is finite everywhere except
`r = 0`.

**Validation.** Agreement with the textbook BL formula, using `p_BL = Jᵀ p_KS`. For `a = 0`, `Q + L_z² = |x × p|²`.
Bardeen's spherical-photon-orbit constants `ξ = L_z/E` and `η = Q/E²` (Appendix A) give a null BL momentum with
`p_r = 0` that maps back to the same `ξ, η`. The Poisson brackets with `H` vanish: `dH/dλ`, `dE/dλ`,
`dL_z/dλ` and `dQ/dλ` along the Hamiltonian vector field are zero to finite-difference accuracy (property test,
also run with 10⁶ cases).

## Ray tracing

Code: `crates/gravography-physics/src/{ode,tetrad,ray,distant}.rs`.

### Initial conditions

The camera tetrad `{e_(0) = u, e_(1) = forward, e_(2) = left, e_(3) = up}` is built by Gram–Schmidt with
respect to `g` (`Tetrad::gram_schmidt`: `e_0 = u/√(−u·u)`, then `v ← v − Σ_b η_bb g(v, e_b) e_b`, done twice for
robustness). For a unit viewing direction `n` (camera frame), the traced vector is the past-directed null
`k = −e_(0) + nⁱ e_(i)`, and `p = g(k)` is integrated with Hamilton's equations. The physical photon is
`P = −k`, with `−P·u = 1`. With sources at rest at infinity (`u_emit = ∂_t`),
`g = ν_obs/ν_emit = (−P·u)/(−P_t) = 1/p_t` (`sky_frequency_shift`). A static Schwarzschild observer gets
`g = (1 − 2M/r)^{−1/2}` (unit test).

### Integrators

DOPRI5 (Dormand & Prince 1980; tableau in Hairer, Nørsett & Wanner, *Solving ODEs I*) with FSAL, local
extrapolation, scaled RMS error `‖e/(atol + rtol max(|y|, |y_new|))‖ ≤ 1`, and controller
`h_new = h · clamp(0.9 err^{−1/5}, 0.2, 5)`. RK4 is the classical scheme, with steps set only by the heuristic caps.
Both are tested for their convergence order (5 and 4).

### Step caps

Applied on top of the adaptive step and converted to `λ` with the current coordinate rates:

- coordinate displacement `≤ far_step_factor · r` (default 0.1);
- for exterior rays moving inward, **radial** displacement `|Δr| ≤ horizon_step_factor · (r − r_+)` (default
  0.1), with `dr/dλ = ∂_i r ẋⁱ`.

The first version capped the *total* displacement near the horizon. For `a = 0.998`, near-critical prograde rays
orbit at `r_ph ≈ 1.074`, just outside `r_+ ≈ 1.063`, and needed ~10⁴ steps per orbit, exceeding the 10⁵-step budget.
Capping only radial motion fixed this (~5–10·10³ steps) without affecting the approach to the past horizon.

### Termination and classification

Checked after every accepted step, in this order:

1. any non-finite state or RHS → `Error`;
2. `r < r_singular` (1e-3) → `Singular`;
3. `r > r_escape` with `r` increasing → `Escaped`, `r_escape = max(escape_factor · r_cam, escape_min_radius)`;
4. `r < r_+ + capture_margin` with `r` decreasing, or `max|p_i| > 10⁸ max|p(0)|` → `Captured`, where
   `capture_margin = max(horizon_epsilon, κ (r_ph,pro − r_+))` (`TraceParams::capture_margin`; CPU reference:
   `horizon_epsilon = 1e-3`, `κ = 0`; GPU since WP-12: `κ = ½`);
5. accepted + rejected steps `≥ max_steps` (10⁵ on the CPU) → `MaxSteps`.

Rule 4 relies on exterior photons never turning around below the prograde photon orbit, which lies above
`r_+ + 10⁻³` for every allowed spin (`r_ph,pro − r_+ ≈ 0.0074` at `a = 0.999`, `0.011` at `a = 0.998`).

**Why any `κ < 1` is safe (WP-12).** An inward-moving photon turns back only at a *periapsis*: a zero of the
radial potential `R(r) = (r² + a² − aξ)² − Δ (η + (ξ − a)²)` (E = 1, Bardeen 1973) with `R < 0` just below and
`R > 0` just above. Spherical photon orbits are double roots of `R` and fill `[r_ph,pro, r_ph,retro]` (Teo 2003).
A periapsis tends to the spherical orbit with the same `(ξ, η)` as those constants become critical, so the lowest
possible periapsis is `r_ph,pro`. Close to the horizon `R(r_+) = (r_+² + a² − aξ)² ≥ 0`. The delicate case is
`ξ ≈ 2r_+/a`, where `R(r_+) ≈ 0` and `R′(r_+) < 0`: there `R` dips negative just outside the horizon. Photons in
the thin allowed band below the dip come out of the past horizon and fall back into the future horizon, so they
never reach an exterior camera, and the next root above is a periapsis outside the photon region again. Nothing
an exterior camera sees turns around in `(r_+, r_ph,pro)`.

This was checked numerically. A numpy root search over 1.3·10⁶ constants (ξ ∈ [−12, 12] plus a fine band
around `2r_+/a`, η ≤ 60 including the vortical range `η ≥ −(a − |ξ|)²` for `|ξ| < a`) gave these lowest
periapses:

| a | r₊ | r_ph,pro | lowest periapsis |
|---|---|---|---|
| 0.5 | 1.86603 | 2.34730 | 2.34975 |
| 0.9 | 1.43589 | 1.55785 | 1.55903 |
| 0.99 | 1.14107 | 1.16764 | 1.17553 |
| 0.999 | 1.04471 | 1.05208 | 1.05237 |

The unit test `photon_periapsis_never_below_prograde_photon_orbit` repeats the check by sign-change detection.
It covers the grid for a ∈ {0, 0.5, 0.9, 0.99, 0.998, 0.999}, and a positive control finds periapses just above
`r_ph,pro`.

A ray inside `r_+ + κ (r_ph,pro − r_+)` moving inward is therefore captured. Stopping it there instead of
following it toward the past horizon, where `|p|` diverges and steps shrink, saves most of the cost of shadow
pixels. With `κ = ½` the margin is half the distance to the lowest periapsis (0.0037 at `a = 0.999`, 0.5 at
`a = 0`). For a camera inside the horizon,
past-directed rays have `r` increasing in the region between the horizons. They cross the future horizon
outward, which ingoing KS covers regularly, so rule 4 does not fire on the way out.

### Escape direction

The escape direction is the unit spatial part of `dx/dλ` at termination. **No weak-field correction is applied.**
The PRD suggests one as optional, and the standard Newtonian-like correction `n_∞ ∝ n − 2M b⃗/(R(R + s₀))` was
implemented first, but it made the result *worse* (9·10⁻⁴ rad error instead of 6·10⁻⁶ at `R = 100`). In ingoing KS
coordinates the traced outgoing ray, an ingoing photon in forward time, is nearly parallel to the ingoing null
congruence `l`. Radial ingoing null rays are straight lines in these coordinates, so the remaining coordinate
bending is suppressed by `(b/R)²`. Measured convergence is `≈ R⁻⁴` (each doubling of `R` cuts the error 14–20×).
The CPU reference therefore uses `r_escape = max(16 r_cam, 400)`, which keeps the error below `3·10⁻⁷` rad in the
tests (cost: ~8 extra steps per doubling of `R`). The PRD's `k = 2, R_min = 100` gives errors up to `1.4·10⁻³` rad
for sideways rays from a camera at `r ≈ 30`. The GPU defaults are for WP-08/WP-12 to choose; `k ≥ 4` keeps the
error below `10⁻⁴`.

### Distant observer (Bardeen image coordinates)

`Kerr::bardeen_ray`: for an observer at BL `(r_obs, i, φ_obs)` and image coordinates `(α, β)`,
`ξ = −α sin i`, `η = β² + cos²i (α² − a²)` (`E = 1`). The physical BL momentum is `p_t = −1`, `p_φ = ξ`, `p_θ = β`
(since `Θ(i) = η + a² cos²i − ξ² cot²i = β²`), and `p_r = +√R/Δ` with `R = (r² + a² − aξ)² − Δ(η + (ξ − a)²)`.
The traced momentum is minus this, transformed to KS. Because the constants of motion are the exact asymptotic
ones, the traced geodesic is exactly the one seen at infinity. `r_obs` only sets where the trace starts, so tests
built on it (b_c, shadow edges) have no finite-distance error. Sign conventions: `β > 0` toward `+z`; prograde
photons (`ξ > 0`) appear at `α < 0`.

### Validation (PRD §9)

| Test | Result |
|---|---|
| 4 Conservation (a = 0.9, 85 rays, r > r_+ + 0.5) | `|H|/|p|² ≤ 5e-11`, `ΔL_z ≤ 5e-11`, `ΔQ ≤ 7e-11` (relative), `p_t` bit-exact |
| 5 `b_c = 3√3` | bisection error 3e-10 |
| 6 Weak deflection, b = 500…5000 | relative to the 2-term series 4e-5 … 4e-7 (the next series term) |
| 7 Strong deflection vs orbit quadrature, b = 5.3…40 | `Δφ` error 3e-9 … 2e-11 rad |
| 8 Kerr shadow edges, a = 0.5, 0.9, 0.998 | relative 1e-9 … 3e-11; also via a static camera at r = 1e4 (ξ to 1e-6, α ≈ −r sinψ to 1e-3) |
| 12 Escape-direction convergence | ≈ R⁻⁴; default error ≤ 3e-7 rad |

## Observer dynamics

Code: `crates/gravography-physics/src/observer.rs` (and `Tetrad::rotated` in `tetrad.rs`).

### Equations of motion and Fermi–Walker transport

The observer state is the KS event `x^μ`, an orthonormal tetrad `e_(a)^μ` with `e_(0) = u`, and proper time `τ`.
With 4-acceleration `A` (`A·u = 0`), the system integrated in `τ` (`docs/PRD.md` §5.8) is

    dx^μ/dτ     = u^μ,
    de_(a)^μ/dτ = −Γ^μ_{αβ} u^α e_(a)^β + u^μ g(A, e_(a)) − A^μ g(u, e_(a)).

The second line is `De_(a)/dτ = (u ⊗ A − A ⊗ u)·e_(a)`, the Fermi–Walker law. For `e_(0) = u`
(`g(u, u) = −1`, `g(A, u) = 0`) it gives `Du/dτ = A`, the equation of motion with thrust. For spatial legs
(`g(u, e_(i)) = 0`) it gives `De_(i)/dτ = u g(A, e_(i))`: the legs only tilt as needed to stay orthogonal to `u`,
with no rotation, as a gyroscope would. The generator `u ⊗ A − A ⊗ u` is antisymmetric under `g`, so
`d/dτ g(e_a, e_b) = 0` exactly. The unit test `fermi_walker_preserves_inner_products` checks this identity
pointwise, including the metric-derivative term `∂_γ g_{μν} u^γ e_a^μ e_b^ν`.

The state `(x, e_(0..3))` has 20 components and is integrated by DOPRI5 (rtol `1e-10`, atol `1e-12`).
The coordinate displacement per substep is capped at `0.1 r`. After each accepted substep the tetrad is
re-orthonormalised by Gram–Schmidt seeded with `(u, e_1, e_2, e_3)`, which keeps `e_(0) ∥ u` and
`u·u = −1`. The drift before correction is reported (`AdvanceReport::max_constraint_error`). It stays
`< 1e-9` per substep in the tests.

**Thrust.** The user's thrust `Aⁱ` (units `c²/M`) is given in the transported body frame: `A = Aⁱ e_(i)`. It is
re-evaluated from the current legs at every RK stage. The look rotation is a separate SO(3) rotation `R`
(`Tetrad::rotated`, `e'_(i) = Σ_j R_ji e_(j)`). It is not physical. Thrust typed in the view frame `A'` is
`A = R A'` in the body frame. Conversion to g-forces: `a_SI = a · c² / (G M/c²)`, with
`G M_☉/c² = 1476.625 m` (IAU nominal `G M_☉`) and `g₀ = 9.80665 m/s²`.

### Stationary observers

Static observers, ZAMOs and circular orbits all move along the Killing field `K = ∂_t + Ω ∂_φ` with constant
`Ω`. In KS, `∂_φ = −y ∂_x + x ∂_y` (§ "Conserved quantities"), so `K = (1, −Ω y, Ω x, 0)` and `u = N K`,
`N = (−g(K, K))^{−1/2}`. The norm `g(K, K)` is constant along the orbit, because `K` is Killing and `Ω` is
constant. Hence

    du^μ/dτ = N² K^ν ∂_ν K^μ = N² (0, −Ω² x, −Ω² y, 0),     a^μ = du^μ/dτ + Γ^μ_{αβ} u^α u^β,

with `N = u^t`. Special cases:

- **Static** (`Ω = 0`): exists where `g_tt < 0`, i.e. outside the ergosurface. The Schwarzschild check is
  `|a| = M/(r² sqrt(1 − 2M/r))`.
- **ZAMO** (`Ω = ω = 2Mar/((r² + a²)² − Δ a² sin²θ)`): exists for `r > r_+`. Tested: `L_z = u_φ = 0`, `a·u = 0`.
- **Circular geodesic** (`Ω = ±M^{1/2}/(r^{3/2} ± a M^{1/2})`, equatorial): `a = 0` to `1e-13` for
  `a ∈ {0, 0.5, 0.9, 0.999}`. This checks the Christoffel symbols, `K` and the Appendix A formula together.

**Station keeping** (`Motion::Hover`, `Motion::Zamo`): the system above is integrated with `A = a(x)`. After each
substep the position is snapped back onto the orbit circle (cylindrical radius and `z` held, azimuth kept),
and `u` is reset to `N K` before Gram–Schmidt. The user's thrust is ignored in these modes.

### Presets

Preset positions use spheroidal `(r, θ, φ̃)` with `x + iy = (r + ia) sinθ e^{iφ̃}`, `z = r cosθ`, `t = 0`.
`φ̃` is the KS azimuth, not BL `φ`; they differ by `Φ(r)`, which is irrelevant for a starting point. By default
the camera looks at the hole (forward `= −x⃗/|x⃗|`, up `≈ +z`, or `+x` on the axis). The seeds
`(u, f, up × f, f × (up × f))` are positively oriented and Gram–Schmidt is triangular with a positive diagonal,
so `det e_up > 0`, i.e. the tetrad is right-handed (tested).

**Radial infall from rest at infinity** (`E = μ = 1`, `L_z = 0`, `Q = 0`). Kerr's `θ` equation gives
`Θ(θ) = Q − cos²θ (a²(μ² − E²) + L_z²/sin²θ) ≡ 0`, so `θ` stays constant for every starting `θ`. In Kerr the
observer is still dragged in `φ`: these are Doran's free-fall observers. On the axis, and for `a = 0`, the fall is
radial. Radial equation: `R = (E(r² + a²))² − Δ (r² + a²) = 2Mr (r² + a²)`, so `u_r^{BL} = −sqrt(R)/Δ`. The
KS covector follows from `dt_BL = dt_KS − T'(r) dr`:

    u = −dt_KS + c dr,   c = u_r^{BL} + 2Mr/Δ = (2Mr − sqrt(2Mr (r² + a²)))/Δ = −sqrt(2Mr) / (sqrt(2Mr) + sqrt(r² + a²)),

where the last form, obtained by rationalising with `2Mr − (r² + a²) = −Δ`, is regular across both horizons.
`dr = ∂_i r dxⁱ` uses the analytic gradient. Tested: `u·u = −1`, `E = 1`, `L_z = Q = 0`, and
`dr/dτ = −sqrt(2Mr(r² + a²))/Σ` (property test, including inside the horizon).

**Circular orbit**: equatorial, `u = N K` with the Kepler `Ω`, evolved as a free geodesic. It is refused at or
below the photon orbit, where `K` is not timelike. Below the ISCO it is allowed but unstable; the UI should warn.

### Regions and limits

`Region`: inside the horizon if `r ≤ r_+`, in the ergosphere if `r ≤ r_E(θ)` with `cosθ = z/r`, otherwise
outside. The simulation stops (`ObserverError::InnerLimit`, observer left at the last valid substep) below
`r_min = r_− + 0.05`. That is `0.05` near the Schwarzschild singularity, and just outside the inner horizon for
Kerr (`docs/PRD.md` §5.8). One formula covers both, and it is continuous in `a`. It also stops beyond
`r_max = 1000` (`OuterLimit`; PRD §5.9 keeps the GPU camera at `r ≲ 200`, and the app may lower it).

### Spin change while flying

Keep the KS position and `uⁱ`. Solve `g_tt (u^t)² + 2 g_ti uⁱ u^t + g_ij uⁱ uʲ = −1` with the new metric for
`u^t > 0`. In KS, `u^t = −g(u, n)` for the future-directed KS-normal observer `n^μ = −g^{μt}`, so `u^t > 0` means
future-directed. Of the positive roots (inside the ergosphere both can be positive), take the one closest to the old
`u^t`. Then re-run Gram–Schmidt with the old spatial legs. Station-keeping observers are rebuilt with the new
`ω` or refused (e.g. a hover point that is now inside the ergosphere). On error the state is unchanged, and the
app should reset to a safe preset.

### Interactive flight (viewer)

Code: `crates/gravography-app/src/viewer/flight.rs` and `input.rs` (WP-11).

- **Time:** each frame advances `τ` by `Δτ = Δt_wall · w`, with the time warp `w` in `M` of proper time per
  wall-clock second. `Δt_wall` is clamped to 0.1 s. Physical scale: `1 M = G M/c³ ≈ 4.93 µs · (M/M_☉)`, so even
  `w = 1000 M/s` is far slower than real time for a stellar-mass hole.
- **Thrust:** the held keys give a unit direction `d̂'` in the view frame (forward, left, up). Diagonals are
  normalised. The body-frame thrust is `Aⁱ = |A| (R d̂')ⁱ`, with the look rotation `R` as above. Since
  `A = Aⁱ e_(i)` and the spatial legs are orthonormal, the felt acceleration `sqrt(g(A, A))` equals the slider
  value `|A|` (tested).
- **Station keeping vs thrust:** hover and ZAMO observers ignore user thrust. A thrust key (or Space, "cut
  thrust") switches to `Motion::Free`, continuing from the current state.
- **HUD:**
  - speed relative to the local ZAMO: `γ = −g(u, u_ZAMO)`, `β = sqrt(1 − 1/γ²)`, undefined for `r ≤ r_+`;
  - felt acceleration: `sqrt(g(A, A))` of the acceleration actually applied (thrust, or the station-keeping
    acceleration);
  - BL `r, θ, φ`: BL `φ` diverges on the horizons (`Φ(r)` has a log term), so the HUD shows "—" when it is not
    finite;
  - time: the KS `t` (regular everywhere).

  Checks: hover at `r = 30` gives `M/(r² sqrt(1 − 2M/r))`; a circular orbit at `r = 10`, `a = 0` gives
  `β = sqrt(M/(r − 2M))` relative to the static (= ZAMO) observer (Hartle §9.3).
- **Camera changes:** accumulation restarts only if the spin, the KS spatial position or a camera-tetrad component
  changes by more than `1e-9 (1 + |v|)`, compared with the camera of the accumulated image. A Schwarzschild hover
  observer's state only changes at round-off, so it converges (tested over 600 frames). Slow genuine motion
  accumulates past the tolerance and restarts. A Kerr static observer's frame precesses (frame dragging), so its
  view really does rotate slowly.
- **Limits and resets:** the viewer uses `r_max = 200 M` (the GPU f32 range, PRD §5.9) and `r_min = r_− + 0.05`.
  An inner/outer limit, an integration failure or an invalid spin change resets the observer to the last applied
  preset, or to hover at `r = 30` (valid for every `a ≤ 0.999`), and pauses with a message.

### Validation (PRD §9 tests 9–11, plus Fermi–Walker)

| Test | Result |
|---|---|
| 9 SR limit (`M = 1e-12`, β ≤ 0.99, 13 directions, traced) | aberration `cos θ_sky = (cos θ − β)/(1 − β cos θ)` to `1e-9`; `g = 1/(γ(1 − β cos θ))` to `1e-12` |
| 10 Static Schwarzschild observer, r = 2.2…20, 26 directions | `g = (1 − 2/r)^{−1/2}` to `1e-12` (camera and at escape) |
| 11 Circular orbits r = 10, a ∈ {0, 0.9}, pro/retro, 10 orbits | radius drift `< 1e-7`, `Ω` to `≤ 1.1e-8` (bound `5e-8`), constraint drift `< 1e-9` |
| 11 Radial infall, r₀ = 10 | `r(τ)` to `1e-9`, through the horizon to `r_min`; `τ(r₀ → 0) = (2/3) sqrt(r₀³/2)` to `1e-9` |
| Geodetic precession, Schwarzschild r = 6, 10, 30 | `Δφ = 2π (1 − sqrt(1 − 3M/r))` per orbit (Hartle §14.4), sign included, to `1e-7` |
| Frame dragging | ZAMO `dφ/dt = ω > 0`; L = 0 infall dragged to `+φ` |
| Constant thrust, flat space | hyperbolic motion `x = (cosh ατ − 1)/α`, `t = sinh(ατ)/α`, non-rotating frame, to `1e-9` |

The orbit errors are DOPRI5 truncation error. A probe (one orbit, r = 10) showed them falling about 15× per decade
of `rtol`, e.g. the precession `cos Δφ` error went 4e-7, 3e-8, 2e-9, 1e-10, 9e-12 for `rtol = 1e-8 … 1e-12`.

## Colour pipeline and sky

Code: `crates/gravography-sky/src/`.

### Spectra and colour

- **Planck** (`planck.rs`): `B_λ(T) = 2hc²/λ⁵ / (e^{c₂/(λT)} − 1)` and `B_ν(T) = 2hν³/c² / (e^{hν/kT} − 1)`, SI 2019 exact
  `h, c, k`. `exp_m1` keeps the Rayleigh–Jeans end accurate, and an overflowing exponent returns 0.
- **Blackbody under a frequency shift.** `I_ν/ν³` is invariant, so `I_ν,obs(ν) = g³ I_ν,emit(ν/g)`. For a
  blackbody, `g³ B_ν(ν/g, T) = B_ν(ν, gT)` exactly (in wavelength, `g⁵ B_λ(gλ, T) = B_λ(λ, gT)`), so a source of
  temperature `T` and filling factor `w` is observed as `w · B(gT)`. Shading therefore needs one LUT lookup at `gT`
  (`docs/PRD.md` §5.7, §6.3). Tested pointwise, and through the band integral.
- **CIE 1931 2°** (`cmf.rs`): bundled 1 nm table (`THIRD_PARTY.md`). **Band response** (`band.rs`):
  `cᵢ(T) = Σ_λ B_λ(λ, T) rᵢ(λ) Δλ`, a rectangle sum at the table spacing (CIE-recommended); for human vision
  `c = XYZ` in W m⁻² sr⁻¹ (no `K_m` factor; exposure absorbs absolute scale).
- **XYZ → linear sRGB** (`colour.rs`): derived from the sRGB primaries and D65 `(0.3127, 0.3290)` by the standard
  construction (RGB→XYZ columns = primaries scaled so `(1,1,1)` ↦ white), then inverted. It agrees with the IEC
  61966-2-1 4-decimal matrix to < 5e-4. The sRGB transfer function is IEC 61966-2-1.
- **False-colour bands** (WP-13, §6.1). A `Band` is three channel responses plus a display matrix `D` (channels
  → linear sRGB) and luminance weights. The composites `near_infrared` (J, H, K), `near_ultraviolet` (200–260,
  260–320, 320–380 nm) and `soft_xray` (2–8, 1–2, 0.3–1 keV) use top-hat windows from short to long wavelength,
  shown as blue, green and red:
  - `D` is diagonal in (long → R, medium → G, short → B), with `D_cc = 1/c_c(T_ref)`, so a blackbody at the band's
    reference temperature (Wien peak in the band: `T_ref = b/λ_peak`, `b = 2.897771955e-3 m K`) is
    `rgb = (1, 1, 1)`;
  - hotter, or blueshifted, light gets bluer, because the Wien peak moves to shorter wavelengths (tested);
  - luminance weights: `LUMA · D`, so the manual-exposure reference is luminance 1 at `T_ref`;
  - for human vision the weights are `(0, 1, 0)`, so the reference stays exactly `Y` of 5772 K, unchanged since
    WP-06.

  The frequency shift needs nothing new: a blackbody stays a blackbody at `gT`, so the same `w · c(gT)` lookup
  applies in any band. In X-rays, ordinary stars give `S ≈ e^{−10⁵}`, which underflows; `ln S` is clamped to
  −1000 in the table so interpolation stays finite (tested). The star field is band-independent: the catalogue's
  visible flux defines each star's `(w, T)`, and the band only changes the table and `D`. Test 16 includes
  `scenes/fast_observer_infrared.toml`: flux within 0.8% of the CPU reference.

### Blackbody LUT

`BlackbodyLut`: 4096 entries uniform in `ln T` over `[100 K, 10⁷ K]`, each `[ln S, x, y, 0]` (f32) with
`S = c₀ + c₁ + c₂` and `(x, y)` the chromaticity. `ln S` spans ≈ −360 … +40, which f32 holds easily, whereas `S`
itself would underflow at 100 K. Interpolation is linear in `ln T`. The error is dominated by the curvature of
`ln S ≈ −c₂/(λ_eff T)` in `ln T`, i.e. `h²/8 · c₂/(λT)` with `h = ln(10⁵)/4095`. Measured worst case: `Y` < 5e-4
relative and `xy` < 1e-5. Out-of-range temperatures are clamped and flagged (`LutSample::clamped`). The GPU must
sample entry centres: texture coordinate `(u (n − 1) + 0.5)/n` with `u = ln(T/T_min)/ln(T_max/T_min)`.

Validation (PRD §9 tests 13, 14): Planckian-locus chromaticities vs Kim et al.'s published cubic spline, 1700–25000 K,
worst deviation < 1.5e-3 (tolerance 2e-3). Illuminant A (Planckian, 2848 K with `c₂ = 1.435e-2`) gives
(0.447574, 0.407439) vs the published (0.44757, 0.40745). Wien `λ_max T = b` to 1e-7. Stefan–Boltzmann
`π∫B_ν dν = σT⁴` to 1e-9 (T = 100 K … 10⁷ K). RJ/Wien limits.

### Procedural star field

`catalogue.rs`, deterministic from a `u64` seed via SplitMix64 (`rng.rs`). It is implemented in-crate so that output
is bit-stable across dependency updates (NF-3); the reference vector is tested.

- **Temperature:** a spectral-class mixture O 0.5 %, B 3.5 %, A 6 %, F 8 %, G 12 %, K 25 %, M 45 %, log-uniform
  within each class's range (2400–50000 K).
- **Brightness:** the flux in band channel 1 (`Y`) follows `N(> F) ∝ F^{−α}` (α = 3/2, the Euclidean count slope),
  truncated to `[F_min, 10⁴ F_min]` (10 magnitudes). It is sampled by inversion: `F = F_min v^{−1/α}` with
  `v ~ U[R^{−α}, 1]`.
- **Solid angle:** a star is a blackbody disc, `F = Ω_* Y_BB(T)`, so `Ω_* = F / Y_BB(T)` (LUT).
- **Positions:** a fraction `f_gal = 0.6` follows an exponential latitude profile `|b| ~ Exp(h = 0.15 rad)` about a
  galactic pole (default tilted 60° from the spin axis); the rest are isotropic.

**Diffuse band** (`diffuse.rs`): `w(d) = A e^{−|b|/h} (1 − k + k·fbm(f d))`, with 5-octave value noise hashed with
`mix64`, its own temperature (4500 K), and `A = 5e-3` chosen by eye (`examples/sky_preview.rs`).

### Texture sky (approximate)

`texture.rs` (WP-13, §6.5). An equirectangular linear-sRGB image (EXR/HDR) is converted into the same `(w, w·T)`
cube map as the star field:

1. per cube texel, the image is sampled bilinearly in the texel's direction;
2. the colour is converted to XYZ and chromaticity `(x, y)`;
3. `T` is the nearest point of the Planckian locus (256 samples, log-spaced over 1000–40 000 K, 1.5% apart);
4. `w = Y / Y_LUT(T)`.

The renderer then shows `w B(gT)`, which gives exactly the PRD's two requirements: the bolometric intensity scales
as `g⁴` (since `∫ B(gT) ∝ g⁴T⁴`), and the hue shifts along the Planckian locus.

Limitations, stated in the UI:
- off-locus colours (greens, purples) collapse to the nearest blackbody colour;
- spectral detail (emission lines) is lost;
- pixel values are relative.

Tests:
- blackbody colours fit back to their `T` within 1.5%, and to their luminance within 1e-3;
- image orientation: centre `+x`, `u = ¼` is `+y`, top row `+z`;
- a uniform 4000 K texture seen by a static observer at r = 10 renders at `Y(gT)/Y(T)` times its luminance, with
  `g = (1 − 2/r)^{−1/2}`, within 6% (the fit's temperature resolution dominates).

### Sky cube map

`cubemap.rs`, `bake.rs`. The face layout and `(u, v)` convention are the standard GL/Vulkan/Metal/WebGPU cube
convention (table in the module docs), so the GPU samples with the escape direction directly. KS axes are used
as-is (`+z` = spin axis).

- **Texel solid angle** (exact): with face coordinates `a, b ∈ [−1, 1]`, `Ω([a₀,a₁]×[b₀,b₁]) = A(a₀,b₀) − A(a₀,b₁) −
  A(a₁,b₀) + A(a₁,b₁)`, `A(a, b) = atan2(ab, sqrt(a² + b² + 1))`. Sums to 4π (tested).
- **Texels** store `(w, w·T)` (f32). A star is splatted with a bilinear tent over 2×2 texels with weights
  `kᵢ` (Σ kᵢ = 1): `wᵢ += kᵢ Ω_*/Ω_texel,ᵢ`, `(wT)ᵢ += kᵢ Ω_* T/Ω_texel,ᵢ`. **Flux conservation:**
  `Σ w Ω = Σ Ω_*` exactly; tested to 1e-5 (f32) at every mip level, also at production size (2048², 2·10⁵ stars).
  Taps are clamped at face edges (no cross-face splat): < 1 texel position error there, no flux error.
- **Mips:** parent = solid-angle-weighted mean of its 4 children, so `Σ w Ω` is conserved per level.
- **Temperature mixing (approximation, PRD §6.3).** A texel (or mip texel) containing several sources stores
  `T_eff = Σ wᵢTᵢ / Σ wᵢ`, and shading uses `w · c(g T_eff)` instead of `Σ wᵢ c(g Tᵢ)`. `Σ w Ω` (the total solid angle
  of the sources) is exact; the colour and band flux of a mixed texel are those of
  a single blackbody at the mean temperature. The error is second order in the temperature spread within the
  footprint. It matters only where differently coloured stars share a footprint, i.e. at coarse mips, and it
  does not change under lensing, because surface brightness is conserved.
- **Cache** (`cache.rs`): binary dump keyed by FNV-1a of the bake version and `Debug` text of `SkyParams`; a
  mismatching or corrupt file is a miss. The directory is chosen by the app.


## Rendering

`gravography-render` (WP-06). The CPU reference renderer (`cpu.rs`) defines the image the GPU must reproduce.

### Camera and projection

`camera.rs`. A pixel's viewing direction is `n = (n_f, n_l, n_u)` in the camera tetrad (forward, left, up),
`docs/PRD.md` §5.4. Rectilinear (pinhole) projection with vertical field of view `φ`, half-height `t = tan(φ/2)` on
the unit-distance image plane, continuous pixel coordinate `(x, y)` (`y` down):
`n ∝ (1, −(2x/W − 1) t W/H, (1 − 2y/H) t)`. Pixel pitch on the plane is `Δ = 2t/H`, so a pixel subtends
`dΩ = Δ² cos³θ` with `cos θ = n_f` (tested: the sum over pixels equals the pyramid solid angle
`4 asin(sin α sin β)`).

**Equirectangular** (WP-13, FR-R2): longitude `λ = −π (2x/W − 1)` (positive to the left, `+π` at the left
edge), latitude `φ = π (½ − y/H)`, direction `n = (cos φ cos λ, cos φ sin λ, sin φ)`, and
`dΩ = cos φ (2π/W)(π/H)`. Tested: the six axis directions land at the expected pixels, and the per-pixel solid
angles sum to `4π` (relative error 3e-6 at H = 300). The GPU uses the same formulas (`camera.wgsl`). Footprints
come from neighbouring rays, so shading needs no change. Test 16 includes `scenes/kerr_panorama.toml`: flux
within 0.2% of the CPU reference. Mouse-look sensitivity is `180°/H` per pixel.

### Boosted observers

`Tetrad::boosted` (physics crate). An observer moving with velocity `β n̂` (`n̂` in the old spatial frame,
`γ = (1 − β²)^{−1/2}`) has the boosted tetrad
`e'_(0) = γ (e_(0) + β n̂ⁱ e_(i))`, `e'_(j) = e_(j) + n̂_j [(γ − 1) n̂ⁱ e_(i) + γ β e_(0)]`, i.e. the standard pure boost
matrix `Λ` (no Thomas–Wigner rotation) applied in tetrad components. Since `Λ` preserves `η`, the new legs are
orthonormal; legs perpendicular to `n̂` are unchanged. Scene files use it for a camera moving relative to a preset
observer. Tested: Minkowski Doppler factor `sqrt((1 + β)/(1 − β))` straight ahead; orthonormality and
`−g(u, u') = γ` in Kerr (property test).

### Shading

For a sample with escape direction `d` and frequency shift `g = 1/p_t` (`docs/PRD.md` §5.7; computed at the
camera, so it is the same for every integrator), the cube map averaged over the sample's sky footprint gives
`(w̄, (wT)‾)`, and the band response is

`XYZ = w̄ · c(g T̄)`, `T̄ = (wT)‾ / w̄`,

because a blackbody of temperature `T` seen with shift `g` is a blackbody of temperature `gT` (`I_ν,obs =
g³ I_ν,emit(ν/g)`), and `w` scales specific intensity. This is surface brightness: lensing magnification is not
applied explicitly. It appears through the footprint, because a magnified region (small sky footprint per pixel)
samples a fine mip level where a star is concentrated, and a demagnified region averages over many texels. XYZ is
converted to linear sRGB (D65) and averaged over the samples of the pixel.

**Effect toggles** (`docs/PRD.md` §6.4):
- lensing off: `d` is the unit spatial part of `k = −e_(0) + nⁱ e_(i)` (aberration and the coordinate direction of
  the local frame kept, no bending); the footprint is computed the same way;
- frequency shift off: `g = 1`;
- beaming off: `XYZ = w̄ c(gT̄) / g⁴`, since the bolometric intensity of a blackbody `∝ T⁴` grows by `g⁴`.

### Footprint by ray differentials

The sky solid angle covered by one pixel is `|J_x × J_y|` with `J = ∂d/∂(x, y)` the Jacobian of the escape
direction with respect to pixel coordinates (first-order area element on the unit sphere). The CPU renderer
traces two extra rays at `(x + ε, y)` and `(x, y + ε)` (`ε = 0.25` px) and uses forward differences
`J_x ≈ (d(x + ε) − d)/ε`. If a differential ray is captured it retries at `−ε`. If one axis still fails, the footprint
is `|J|²` of the other axis (isotropic). If both fail, it is the unlensed pixel solid angle. The stats count both
cases. With `N` samples per pixel, each sample's footprint is `1/N` of the pixel's.

The mip level is `lod = ½ log₂(Ω / Ω_texel(d))` with the level-0 texel solid angle at `d`,
`Ω_texel = (2/N_face)² |d_major|³` (from `dΩ = ds dt / (1 + s² + t²)^{3/2}`; matches the exact texel solid angle to
`O(N⁻²)`), and the cube map is sampled trilinearly (`CubeMap::sample_trilinear`).

**Approximations.**
- The footprint is isotropic: the area is right but not the shape. Near the critical curve the true footprint is
  a long thin arc; a square of the same area averages a different set of texels. This is unbiased on average but
  the texture is softer than with anisotropic filtering. Supersampling (`samples > 1`) reduces the error.
- Stars are 1–2 texels wide in the cube map (PRD §6.3). Where lensing magnifies the sky strongly (the Einstein ring
  and the photon rings), a pixel sees a single texel. Stars are therefore drawn as arcs of texel width and texel
  surface brightness, not as thin, much brighter arcs. The flux of each image is correct to first order.
- Forward differences have `O(ε)` truncation error. Against the exact Schwarzschild lens map (below) it is
  ≤ 7e-4 relative at `ε = 0.05` px, which is negligible for mip selection.

**Validation** (`crates/gravography-render/tests/render_validation.rs`). A static camera at `d = 10⁴` in
Schwarzschild: the exact map from the local ray angle `θ` to the sky angle `ψ` is obtained by quadrature of the
orbit equation `(du/dφ)² = 1/b² − u² + 2u³` with `b = d sin θ / sqrt(1 − 2/d)`, `ψ = π − Δφ`. The renderer's escape
directions match `ψ` to 7e-12 rad. Its footprint divided by the pixel solid angle matches the inverse
magnification `(sin ψ / sin θ) |dψ/dθ|` to ≤ 7e-4 for `θ/θ_E ∈ {0.7, 0.9, 1.2, 1.5, 2.5}`, both inside the Einstein
ring (secondary images, parity flip) and outside it.

### Exposure and tone mapping

`tonemap.rs`. The HDR image is linear sRGB in LUT units (W m⁻² sr⁻¹ for the human-vision band) and is never
clamped. Display: `encode_sRGB(tonemap(s · rgb))`.

- Auto exposure: `s = 2^EV · 0.18 / L̄`, with `L̄ = exp(mean ln Y)` over pixels with `Y > 0`. Black pixels are
  excluded so that the shadow's size does not change the exposure. The sum runs sequentially, which keeps it
  deterministic. The viewer computes the same `L̄` on the GPU (`stats.wgsl`, WP-12): f32 sums of `ln Y` per
  256-pixel workgroup, combined in f64 on the CPU. It agrees with the CPU value to ≈ 3e-7 relative (test
  `gpu_stats_match_cpu_readbacks`). Headless renders still use the CPU path.
- Manual exposure: `s = 2^EV / Y_ref`, where `Y_ref` is the luminance of a 5772 K blackbody with `w = 1`, `g = 1`.
- ACES fitted (S. Hill): `M_out · RRT_ODT(M_in · rgb)` with the rational fit
  `(v(v + 0.0245786) − 0.000090537) / (v(0.983729 v + 0.4329510) + 0.238081)`.
- Extended Reinhard on luminance with white point 16, which preserves hue.
- EXR output stores `s · rgb` (exposed, not tone-mapped). PNG stores 8-bit sRGB.

### Sampling and determinism

Sample `k` of a pixel is at offset `frac(½ + k (1/φ₂, 1/φ₂²))` (R2 sequence, `φ₂` the plastic number). Sample 0 is
the pixel centre. Pixels are computed independently (rayon over rows) and statistics are integer sums, so the
image is bit-identical between runs (NF-3; tested).

## GPU port (f32)

`crates/gravography-render/src/gpu/` (WP-07). The WGSL modules in `gpu/shaders/` are ports of the Rust physics with
the same names, the same structure and the same order of operations, so that the GPU reproduces the CPU f32 path
up to rounding:

| WGSL module | Rust counterpart |
|---|---|
| `common.wgsl` | status codes (`cpu::status_index`), `max_abs` |
| `kerr_schild.wgsl` | `ks_radius`, `ks_fields`, `ks_field_derivatives` |
| `geodesic.wgsl` | `l_dot_p`, `ks_hamiltonian_rhs` |
| `rk4.wgsl` | `ode::rk4_step` on the phase state `(x, p)` |
| `dopri5.wgsl` | `ode::{dopri5_step, error_norm, StepController::next_h}` (same tableau, constants rounded once to f32) |
| `ray.wgsl` | `spatial_norm`, `radial_rate`, `step_cap`, `escape_direction`, `trace_ray` (DOPRI5 and RK4 branches; `ray_status_after_step` is the Rust loop's termination block) |
| `camera.wgsl` | `Projection::direction` (rectilinear), `Tetrad::to_coordinate`, `ks_g_lo`, `camera_ray_momentum` |
| `gbuffer.wgsl` | G-buffer texel and tile structs |
| `trace_pixels.wgsl` | `CpuRenderer::cast_pixel` (DOPRI5) → G-buffer `{dir, g, status, steps, rejected}` |
| `sky.wgsl` | `dir_to_face_uv`, `CubeLevel::sample_bilinear`, `CubeMap::sample_trilinear`, `BlackbodyLut::lookup`, `Sky::{texel_solid_angle, lod, shade}` |
| `shade.wgsl` | `CpuRenderer::footprint` (neighbour form) + `Projection::pixel_solid_angle`; accumulation |
| `tonemap.wgsl` | `tonemap::{aces_fitted, reinhard_extended, to_srgb8}`, `srgb_encode` |
| `debug.wgsl` | FR-R6 debug views (no CPU counterpart) |
| `tonemap_ops.wgsl` | `tonemap::{aces_fitted, reinhard_extended}`, `ToneMap::apply` (`tone_map_accum`), `srgb_encode` |
| `present.wgsl` | viewer present pass: accumulation buffer → tone map → window (nearest, stretched; sRGB-encoded in the shader for non-sRGB targets) |
| `test_rays.wgsl` | parity entry points (`trace_rk4_main`, `trace_dopri5_main`, `eval_rhs_main`) |

Modules are concatenated in that order (`gpu/shader.rs`); naga validates the result in a unit test without a GPU.

**f32 specifics.**
- WGSL has no `isnan`/`isinf`, and `v != v` can be removed under fast math, so finiteness is tested on the
  exponent bits (`bitcast<u32>(v) & 0x7f800000 != 0x7f800000`).
- The Rust step cap guards divisions with `tiny = 1e-300`, which rounds to 0 in f32. WGSL uses 0 explicitly,
  which matches the CPU f32 path.
- `r_+` is computed on the CPU in f32 (`Kerr::<f32>::outer_horizon`) and uploaded, so both f32 paths use the same
  value.

**Parity (WP-07, RK4 test rays).** 10 cameras × 32×32 rays over a 90° field: Schwarzschild and Kerr
`a ∈ {0.5, 0.9, 0.998}`, outside and inside the horizon, plus a boosted and an orbiting observer. Measured on an
Apple M2 Pro (Metal):
- Ray status agrees for 100% of rays in every pair (GPU, CPU f32, CPU f64).
- Escape directions, 99th percentile: 8e-5 rad (GPU vs CPU f32), 6e-5 rad (GPU vs CPU f64), 4e-5 rad
  (CPU f32 vs f64).
- Escape directions, maximum: 1.6e-3 rad, for near-critical rays around a `r = 4` prograde orbiter, where small
  differences are amplified.
- The Hamiltonian right-hand side on the GPU matches CPU f32 to 5.5e-7 of its natural scale (≈ 5 ulp).

So f32 precision, not the GPU port, dominates the error budget. Both are well inside PRD §9 test 15
(≥ 99.5% status, 1e-3 rad for ≥ 99% of rays).

### GPU right-hand side in vector form

`geodesic.wgsl::ks_hamiltonian_rhs` (WP-12) computes the same `dp_μ/dλ = ½ ∂_μ f (l·p)² + f (l·p) (∂_μ l^ν) p_ν`
as the Rust code. The Rust code builds the matrix `∂_j l_i` and contracts it with `p`; the WGSL contracts
analytically. From `l_x = (r x + a y)/q`, `l_y = (r y − a x)/q`, `l_z = z/r` (`q = r² + a²`) and the product
rule,

    ∂_j l_x = (x ∂_j r + r δ_jx + a δ_jy − 2 r l_x ∂_j r) / q,
    ∂_j l_y = (y ∂_j r + r δ_jy − a δ_jx − 2 r l_y ∂_j r) / q,
    ∂_j l_z = δ_jz / r − z ∂_j r / r²,

so that

    (∂_j l_i) p_i = s ∂_j r + [(r p_x − a p_y, a p_x + r p_y, 0)/q]_j + δ_jz p_z / r,
    s = (x p_x + y p_y − 2 r (l_x p_x + l_y p_y)) / q − z p_z / r².

Every quantity is a scalar or `vec3`. The earlier port kept the Rust structure: a `var` struct with an
`array<vec4, 4>`, plus loops writing dynamically indexed vector components. naga lowers that to thread-local
memory on Metal, which made the trace 9.3× slower (755 → 81 ms for a 1920×1080 sample, M2 Pro). The rewrite is
checked by the existing GPU-vs-CPU RHS and DOPRI5 test-ray comparisons and by PRD test 15, which was unchanged
by it.

### GPU tracer tolerances

`TraceParams::gpu_default()` (WP-08): DOPRI5, `rtol = 1e-5`, `atol = 1e-7`, `h_min = 1e-6`, at most 2000 steps,
momentum blow-up factor `10⁴`. The escape radius, step caps and horizon criterion are those of the CPU reference.

- **Momentum blow-up factor.** Inside the horizon, the rays in the shadow approach the past (white-hole) horizon
  from inside. In ingoing KS their covariant momentum diverges, and the CPU reference (factor `10⁸`) only catches
  them at `r − r_+ ≈ 10⁻⁸`. That is below f32 resolution near `r = 2` (ulp ≈ 2.4e-7). With the CPU factor, the
  GPU stepped such rays across `r_+`, and 72 of 16 384 pixels of the Schwarzschild `r = 1.2` camera came out
  `Escaped` or `Error` instead of `Captured`, far inside the shadow.
- **Why `10⁴` is safe.** Over the parity set, no escaping ray's `|p|/|p₀|` exceeds 12 (Kerr `a = 0.998`, near
  critical). The rays caught by blow-up all reach `10⁸`. A factor of `10⁴` therefore classifies exactly as the
  reference does, with a ≈ 800× margin, and it triggers while `r − r_+` is still resolvable in f32.
- **Tolerance makes no difference.** `rtol = 1e-4` gives the same agreement and only 3% fewer steps (98 vs 101 per
  ray). Away from the critical curve the step count is set by the step caps (`far_step_factor = 0.1`), and the
  error is set by f32. WP-12 should tune the caps rather than `rtol`.

**WP-12 retuning (speed).** With the cheaper RHS, the cost of a sample is proportional to the mean step count
(≈ 0.63 ns per ray-step at 960×540 on an M2 Pro). The `gravography bench --parity` sweeps (speed and PRD test 15
together) showed:

- **The far-field cap set the step count.** `far_step_factor = 0.1` bound before the error control did, so
  `rtol` had no effect at all from 1e-5 to 3e-4. Relaxing the cap alone hands control to DOPRI5 at `rtol = 1e-5`,
  and accuracy drops (p99 9.9e-4 rad at cap 0.3). The chosen pair is cap 0.5 with `rtol = 2e-6`
  (`atol = 2e-8`).
- **Escape radius `max(4 r_cam, 100)`** instead of `max(16 r_cam, 400)`: 6–10% fewer steps, and the parity
  directions are unchanged (p99 2.9e-5 → 4.5e-5 rad at the WP-08 caps).
- **Captured rays dominated Kerr scenes.** The shadow pixels cost about 2× the sky (steps heat map). They now stop
  at the spin-scaled capture margin (`κ = ½`, see "Termination and classification"). The momentum blow-up factor
  is `10³` (escaping rays peak at `|p|/|p₀| ≤ 12`, an 80× margin). Neither changed a single status in test 15.
- **Unchanged:** the error norm keeps `x^t`. Excluding it made no difference.

| Defaults | rtol | far cap | escape | blow-up | κ | mean steps (test 15) | Kerr edge-on 960×540 |
|---|---|---|---|---|---|---|---|
| WP-08 | 1e-5 | 0.1 | 16 r, 400 | 10⁴ | 0 | 100 | 38 ms |
| WP-12 | 2e-6 | 0.5 | 4 r, 100 | 10³ | ½ | 42 | 13.6 ms (1024² tiles) |

Test 15 with the WP-12 defaults, 128² per camera:

| Check | Result | PRD threshold |
|---|---|---|
| Status agreement | 99.999% (2 differ, both next to the critical curve) | ≥ 99.5% |
| Escaped rays within 1e-3 rad | 99.78% | ≥ 99% |
| Angle, p50 / p99 | 4.0e-6 / 1.9e-4 rad | — |

A 1080p pixel at 60° is 9.7e-4 rad, so p99 is 0.2 px. Test 16 (images) is unchanged: flux within 0.4%, and at
most 3.4% of pixels more than 8 levels off, all from the footprint method.

**Test 15** (WP-08, Apple M2 Pro). The 10 parity cameras, 128 × 128 pixels each over a 90° field
(163 840 rays), GPU per-pixel tracer vs CPU f64 reference (`rtol = 1e-10`):

| Check | Result | PRD threshold |
|---|---|---|
| Status agreement | 100% | ≥ 99.5% |
| Disagreements away from a status boundary | 0 | — |
| Escaped rays within 1e-3 rad | 99.97% | ≥ 99% |
| Angle, p50 / p99 | 2.1e-7 / 2.9e-5 rad | — |
| Max `g` relative error | 3.3e-6 | — |
| Mean GPU steps per ray | 100 (most 287, Kerr 0.998 interior) | — |
| Rays hitting the step budget | 0 | — |

The largest angular errors (≤ 1.1e-2 rad) are near-critical rays of the `r = 4` prograde orbiter and of the
Kerr interior cameras, where tiny differences are hugely amplified.

### GPU shading

`gpu/renderer.rs`, `sky.wgsl`, `shade.wgsl`, `tonemap.wgsl` (WP-09). For each sample `k` (jitter `sample_offset(k)`,
as on the CPU), the trace pass writes the G-buffer. The shading pass then computes, per escaped pixel:

- **Footprint from neighbours.** `J_x = d(x+1) − d(x)` from the neighbouring G-buffer texel, which was traced at
  the same jitter. If that neighbour did not escape or is outside the image, the shader uses `−(d(x−1) − d(x))`.
  The same applies to `y`, followed by the CPU's isotropic and unlensed fallbacks. This is
  `CpuRenderer::footprint` with `ε = 1` px, at no extra ray cost.
- **Shading:** `Sky::shade` with the same LOD formula and footprint `/ N`.
- **Accumulation:** the linear sRGB result is added to an accumulation buffer `(Σ rgb, count)`.

The cube map is a 6-layer RG32Float 2D-array texture with all mips, read with `textureLoad`. The shader does the
bilinear interpolation (clamped at face edges) and the linear interpolation between levels itself, exactly as the
CPU does. Binding it as a non-filterable texture also avoids the optional `FLOAT32_FILTERABLE` feature. RG16F is not
usable: `w·T` reaches ~10⁶, beyond the f16 range.

**Tone mapping** runs on the GPU and writes packed RGBA8. The exposure scale comes from the HDR readback with the
CPU's `display_scale`, so it is deterministic. The shader rounds half away from zero with `floor(x + 0.5)`, not
WGSL `round`, to match Rust.

**Debug views** (FR-R6), drawn from the last sample's G-buffer:
- **status:** escaped blue `(0.2, 0.4, 1)`, captured black, singular yellow, max-steps red, error magenta;
- **shift:** `log₂ g` over `[−2, 2]`, from red (redshift) through white (`g = 1`) to blue (blueshift); non-escaped
  pixels are dark grey;
- **steps:** `log(1 + steps)/log(1 + budget)`, a black → purple → orange → pale-yellow heat map.

**Test 16** (`tests/gpu_render.rs`). Each scene in `scenes/` is rendered by the GPU (`gpu_default` tracer) and by
the CPU reference, with a small sky (256² faces, 2·10⁴ stars) at 96 × 54 (default) and 192 × 108 × 2 spp (ignored).
Four metrics are compared:
- the total luminance;
- the mean relative pixel luminance `|ΔY| / (Y + 0.1 Ȳ)`;
- the fraction of pixels whose tone-mapped bytes differ by more than 8 levels;
- the same three against a CPU render with `ε = 1` px (the GPU's footprint method).

Worst measured on an M2 Pro:

| Comparison | Total luminance | Mean relative | Pixels > 8 levels |
|---|---|---|---|
| vs reference (`ε = 0.25`) | 0.46% | 2.0% | 3.3% |
| vs same method (`ε = 1`) | 1.4e-4 | 1.1e-3 | 0.19% |

Bounds (`ε = 0.25`): 1% / 4% / 6%. Bounds (`ε = 1`): 1e-3 / 3e-3 / 0.5%.

All of the remaining difference from the reference is the footprint method. It changes the mip level where the lens
map curves within a pixel (critical curve, photon rings, the strongly lensed interior view). The CPU footprint
(differential rays at ±0.25 px) is the better estimate of the local Jacobian. The GPU's neighbour differences average
the Jacobian over a pixel, which matters only where the map is far from linear across a pixel.
