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
4. `r < r_+ + horizon_epsilon` (1e-3) with `r` decreasing, or `max|p_i| > 10⁸ max|p(0)|` → `Captured`;
5. accepted + rejected steps `≥ max_steps` (10⁵ on the CPU) → `MaxSteps`.

Rule 4 relies on exterior photons never turning around below the prograde photon orbit, which lies above
`r_+ + 10⁻³` for every allowed spin (`r_ph,pro − r_+ ≈ 0.0074` at `a = 0.999`, `0.011` at `a = 0.998`). For a camera inside the horizon,
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

## Sections (to be written)

- Observer dynamics and Fermi–Walker transport — WP-04
- Colour pipeline and sky representation — WP-05
