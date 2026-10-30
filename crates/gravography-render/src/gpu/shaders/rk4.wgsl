// rk4.wgsl: port of crates/gravography-physics/src/ode.rs rk4_step for the phase-space state, with the
// Hamiltonian right-hand side of geodesic.wgsl.

// y + h k (Rust: axpy).
fn phase_axpy(y: Phase, h: f32, k: Phase) -> Phase {
    return Phase(y.x + h * k.x, y.p + h * k.p);
}

// One classical RK4 step of size h (Rust: rk4_step with f = ks_hamiltonian_rhs).
fn rk4_step(mass: f32, a: f32, y: Phase, h: f32) -> Phase {
    let k1 = ks_hamiltonian_rhs(mass, a, y.x, y.p);
    let y2 = phase_axpy(y, h * 0.5, k1);
    let k2 = ks_hamiltonian_rhs(mass, a, y2.x, y2.p);
    let y3 = phase_axpy(y, h * 0.5, k2);
    let k3 = ks_hamiltonian_rhs(mass, a, y3.x, y3.p);
    let y4 = phase_axpy(y, h, k3);
    let k4 = ks_hamiltonian_rhs(mass, a, y4.x, y4.p);
    let sixth = h / 6.0;
    return Phase(
        y.x + sixth * (k1.x + 2.0 * (k2.x + k3.x) + k4.x),
        y.p + sixth * (k1.p + 2.0 * (k2.p + k3.p) + k4.p),
    );
}
