# Gravography — Physics notes

Derivations and conventions behind the code. Every equation implemented in code points here or to a reference
in `docs/PRD.md` Appendix B (`CLAUDE.md` §5). Maintained by the agent that implements or changes the physics.

## Conventions

Normative conventions are in `docs/PRD.md` §2: units `G = c = M = 1`, signature `(−, +, +, +)`, ingoing
Cartesian Kerr–Schild coordinates `(t, x, y, z)` (index 0 = `t`), covariant ray momenta `p_μ`, Hamiltonian
`H = ½ g^{μν} p_μ p_ν`, rays traced backward in time, `g = ν_obs / ν_emit`.

## Sections (to be written)

- Kerr–Schild metric and the cancellation-free `r(x, y, z)` — WP-01
- Boyer–Lindquist ↔ Kerr–Schild transforms — WP-01
- Analytic derivatives of `g^{μν}`, Christoffel symbols — WP-02
- Conserved quantities `E`, `L_z`, Carter `Q` (Cartesian → spheroidal Jacobian) — WP-02
- Ray initial conditions, termination and classification — WP-03
- Observer dynamics and Fermi–Walker transport — WP-04
- Colour pipeline and sky representation — WP-05
