// dopri5.wgsl: port of crates/gravography-physics/src/ode.rs (dopri5_step, error_norm, StepController) for the
// phase-space state (x, p) with the Hamiltonian right-hand side of geodesic.wgsl. Dormand & Prince (1980); tableau
// as in Hairer, Nørsett & Wanner, "Solving ODEs I", Table 5.2. Constants are const-expressions evaluated at high
// precision and rounded once to f32, like `T::lit` on the CPU.

const A21: f32 = 1.0 / 5.0;
const A31: f32 = 3.0 / 40.0;
const A32: f32 = 9.0 / 40.0;
const A41: f32 = 44.0 / 45.0;
const A42: f32 = -56.0 / 15.0;
const A43: f32 = 32.0 / 9.0;
const A51: f32 = 19372.0 / 6561.0;
const A52: f32 = -25360.0 / 2187.0;
const A53: f32 = 64448.0 / 6561.0;
const A54: f32 = -212.0 / 729.0;
const A61: f32 = 9017.0 / 3168.0;
const A62: f32 = -355.0 / 33.0;
const A63: f32 = 46732.0 / 5247.0;
const A64: f32 = 49.0 / 176.0;
const A65: f32 = -5103.0 / 18656.0;
const B1: f32 = 35.0 / 384.0;
const B3: f32 = 500.0 / 1113.0;
const B4: f32 = 125.0 / 192.0;
const B5: f32 = -2187.0 / 6784.0;
const B6: f32 = 11.0 / 84.0;
const E1: f32 = 71.0 / 57600.0;
const E3: f32 = -71.0 / 16695.0;
const E4: f32 = 71.0 / 1920.0;
const E5: f32 = -17253.0 / 339200.0;
const E6: f32 = 22.0 / 525.0;
const E7: f32 = -1.0 / 40.0;

// StepController::dopri5(): safety 0.9, fac_min 0.2, fac_max 5, error order 4.
const CTL_SAFETY: f32 = 0.9;
const CTL_FAC_MIN: f32 = 0.2;
const CTL_FAC_MAX: f32 = 5.0;
const CTL_ORDER: f32 = 4.0;

// Result of one step (Rust: Dopri5Step).
struct Dopri5Step {
    y: Phase,
    err: Phase,
    k_last: Phase,
}

fn rhs_phase(mass: f32, a: f32, y: Phase) -> Phase {
    return ks_hamiltonian_rhs(mass, a, y.x, y.p);
}

// y + h Σ c_j k_j, summing in order (Rust: combine).
fn combine2(y: Phase, h: f32, c1: f32, k1: Phase) -> Phase {
    return Phase(y.x + h * (c1 * k1.x), y.p + h * (c1 * k1.p));
}

// One DOPRI5 step of size h given k1 = f(y) (Rust: dopri5_step).
fn dopri5_step(mass: f32, a: f32, y: Phase, k1: Phase, h: f32) -> Dopri5Step {
    let k2 = rhs_phase(mass, a, combine2(y, h, A21, k1));
    let k3 = rhs_phase(mass, a, Phase(
        y.x + h * (A31 * k1.x + A32 * k2.x),
        y.p + h * (A31 * k1.p + A32 * k2.p)));
    let k4 = rhs_phase(mass, a, Phase(
        y.x + h * (A41 * k1.x + A42 * k2.x + A43 * k3.x),
        y.p + h * (A41 * k1.p + A42 * k2.p + A43 * k3.p)));
    let k5 = rhs_phase(mass, a, Phase(
        y.x + h * (A51 * k1.x + A52 * k2.x + A53 * k3.x + A54 * k4.x),
        y.p + h * (A51 * k1.p + A52 * k2.p + A53 * k3.p + A54 * k4.p)));
    let k6 = rhs_phase(mass, a, Phase(
        y.x + h * (A61 * k1.x + A62 * k2.x + A63 * k3.x + A64 * k4.x + A65 * k5.x),
        y.p + h * (A61 * k1.p + A62 * k2.p + A63 * k3.p + A64 * k4.p + A65 * k5.p)));
    let y_new = Phase(
        y.x + h * (B1 * k1.x + B3 * k3.x + B4 * k4.x + B5 * k5.x + B6 * k6.x),
        y.p + h * (B1 * k1.p + B3 * k3.p + B4 * k4.p + B5 * k5.p + B6 * k6.p));
    let k7 = rhs_phase(mass, a, y_new);
    let err = Phase(
        h * (E1 * k1.x + E3 * k3.x + E4 * k4.x + E5 * k5.x + E6 * k6.x + E7 * k7.x),
        h * (E1 * k1.p + E3 * k3.p + E4 * k4.p + E5 * k5.p + E6 * k6.p + E7 * k7.p));
    return Dopri5Step(y_new, err, k7);
}

fn error_terms(y: vec4<f32>, y_new: vec4<f32>, err: vec4<f32>, rtol: f32, atol: f32) -> f32 {
    let sc = atol + rtol * max(abs(y), abs(y_new));
    let e = err / sc;
    return dot(e, e);
}

// Scaled RMS error over the 8 components (Rust: error_norm, Hairer et al. eq. 4.11).
fn error_norm(y: Phase, y_new: Phase, err: Phase, rtol: f32, atol: f32) -> f32 {
    let sum = error_terms(y.x, y_new.x, err.x, rtol, atol) + error_terms(y.p, y_new.p, err.p, rtol, atol);
    return sqrt(sum / 8.0);
}

// Next step size (Rust: StepController::next_h).
fn step_controller_next_h(h: f32, err: f32) -> f32 {
    var fac = CTL_FAC_MAX;
    if (err > 0.0) {
        fac = CTL_SAFETY * pow(err, -1.0 / (CTL_ORDER + 1.0));
    }
    return h * min(max(fac, CTL_FAC_MIN), CTL_FAC_MAX);
}
