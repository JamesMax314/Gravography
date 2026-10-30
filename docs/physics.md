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

## Sections (to be written)

- Ray initial conditions, termination and classification — WP-03
- Observer dynamics and Fermi–Walker transport — WP-04
- Colour pipeline and sky representation — WP-05
