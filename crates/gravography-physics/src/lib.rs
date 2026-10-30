//! Kerr / Schwarzschild spacetime physics for Gravography.
//!
//! Pure CPU code with no GPU dependencies (`docs/PRD.md` §4.2). Conventions (normative, `docs/PRD.md` §2):
//! units `G = c = M = 1`, signature `(−, +, +, +)`, ingoing Cartesian Kerr–Schild coordinates `(t, x, y, z)`
//! with index 0 = `t`, rays evolved with covariant momentum `p_μ` and traced backward in time.
//!
//! Contents arrive in WP-01 (metric core) onwards.
