// ray.wgsl: port of crates/gravography-physics/src/ray.rs (trace_ray with Integrator::Dopri5 and ::Rk4), including
// the step caps and the termination/classification order (docs/physics.md § "Termination and classification").

// Tracing parameters (Rust: gravography_render::gpu::GpuTraceParams; 64 bytes, uniform).
struct TraceParams {
    mass: f32,
    spin: f32,
    r_plus: f32,
    horizon_step_factor: f32,
    far_step_factor: f32,
    h_max: f32,
    escape_factor: f32,
    escape_min_radius: f32,
    horizon_epsilon: f32,
    momentum_blowup: f32,
    r_singular: f32,
    max_steps: u32,
    ray_count: u32,
    rtol: f32,
    atol: f32,
    h_min: f32,
}

// Outcome of one ray (Rust: GpuRayResult; 64 bytes).
struct RayResult {
    x: vec4<f32>,
    p: vec4<f32>,
    dir: vec4<f32>,
    status: u32,
    steps: u32,
    lambda: f32,
    rejected: u32,
}

// Euclidean norm of the spatial part (Rust: spatial_norm).
fn spatial_norm(v: vec4<f32>) -> f32 {
    return sqrt(v.y * v.y + v.z * v.z + v.w * v.w);
}

// dr/dλ (Rust: radial_rate).
fn radial_rate(a: f32, r: f32, x: vec4<f32>, dx: vec4<f32>) -> f32 {
    let r2 = r * r;
    let d = r2 * r2 + a * a * x.w * x.w;
    return r * (r2 * (x.y * dx.y + x.z * dx.z) + (r2 + a * a) * x.w * dx.w) / d;
}

// Heuristic step cap (Rust: step_cap). The Rust guard `tiny = 1e-300` is 0 in f32, so it is 0 here too.
fn step_cap(params: TraceParams, a: f32, r: f32, r_plus: f32, x: vec4<f32>, dx: vec4<f32>) -> f32 {
    let speed = max(spatial_norm(dx), 0.0);
    var h = params.far_step_factor * r / speed;
    let dr = radial_rate(a, r, x, dx);
    if (r > r_plus && dr < 0.0) {
        h = min(h, params.horizon_step_factor * (r - r_plus) / max(-dr, 0.0));
    }
    return min(h, params.h_max);
}

// Unit spatial part of dx/dλ (Rust: escape_direction).
fn escape_direction(dx: vec4<f32>) -> vec3<f32> {
    let inv = 1.0 / spatial_norm(dx);
    return vec3<f32>(dx.y * inv, dx.z * inv, dx.w * inv);
}

fn finish(status: u32, y: Phase, dx: vec4<f32>, steps: u32, rejected: u32, lambda: f32) -> RayResult {
    var dir = vec4<f32>(0.0);
    if (status == STATUS_ESCAPED) {
        dir = vec4<f32>(escape_direction(dx), 0.0);
    }
    return RayResult(y.x, y.p, dir, status, steps, lambda, rejected);
}

// Sentinel for "keep integrating" from ray_status_after_step.
const STATUS_RUNNING: u32 = 0xffffffffu;

// Termination checks after an accepted step, in the Rust order (trace_ray loop): non-finite → Error;
// r < r_singular → Singular; r > r_escape and increasing → Escaped; inside r_+ + ε and decreasing, or momentum
// blow-up → Captured.
fn ray_status_after_step(params: TraceParams, y: Phase, k1: Phase, r_prev: f32, r_escape: f32, p_limit: f32) -> u32 {
    if (!(is_finite_vec4(y.x) && is_finite_vec4(y.p) && is_finite_vec4(k1.x) && is_finite_vec4(k1.p))) {
        return STATUS_ERROR;
    }
    let r = ks_radius(params.spin, y.x.y, y.x.z, y.x.w);
    if (r < params.r_singular) {
        return STATUS_SINGULAR;
    } else if (r > r_escape && r > r_prev) {
        return STATUS_ESCAPED;
    } else if ((r < params.r_plus + params.horizon_epsilon && r < r_prev) || max_abs4(y.p) > p_limit) {
        return STATUS_CAPTURED;
    }
    return STATUS_RUNNING;
}

// Traces one ray with fixed-step RK4 and the heuristic step caps (Rust: trace_ray, Integrator::Rk4).
fn trace_ray_rk4(params: TraceParams, x0: vec4<f32>, p0: vec4<f32>) -> RayResult {
    let mass = params.mass;
    let a = params.spin;
    let r_plus = params.r_plus;
    let r_escape = max(params.escape_factor * ks_radius(a, x0.y, x0.z, x0.w), params.escape_min_radius);
    let p_limit = params.momentum_blowup * max_abs4(p0);

    var y = Phase(x0, p0);
    var k1 = ks_hamiltonian_rhs(mass, a, y.x, y.p);
    var r_prev = ks_radius(a, x0.y, x0.z, x0.w);
    var lambda = 0.0;
    var steps = 0u;
    var h = 0.1 * step_cap(params, a, r_prev, r_plus, x0, k1.x);

    loop {
        if (steps >= params.max_steps) {
            return finish(STATUS_MAX_STEPS, y, k1.x, steps, 0u, lambda);
        }
        let h_try = min(h, step_cap(params, a, r_prev, r_plus, y.x, k1.x));
        let y_new = rk4_step(mass, a, y, h_try);
        // No error control: the heuristic cap alone sets the next step.
        h = params.h_max;
        y = y_new;
        k1 = ks_hamiltonian_rhs(mass, a, y.x, y.p);
        lambda = lambda + h_try;
        steps += 1u;

        let status = ray_status_after_step(params, y, k1, r_prev, r_escape, p_limit);
        if (status != STATUS_RUNNING) {
            return finish(status, y, k1.x, steps, 0u, lambda);
        }
        r_prev = ks_radius(a, y.x.y, y.x.z, y.x.w);
    }
    // Unreachable: every exit returns from inside the loop.
    return finish(STATUS_ERROR, y, k1.x, steps, 0u, lambda);
}

// Traces one ray with adaptive DOPRI5 and the heuristic step caps (Rust: trace_ray, Integrator::Dopri5): FSAL,
// rejected steps count toward max_steps, a step is accepted anyway once h ≤ h_min.
fn trace_ray_dopri5(params: TraceParams, x0: vec4<f32>, p0: vec4<f32>) -> RayResult {
    let mass = params.mass;
    let a = params.spin;
    let r_plus = params.r_plus;
    let r_escape = max(params.escape_factor * ks_radius(a, x0.y, x0.z, x0.w), params.escape_min_radius);
    let p_limit = params.momentum_blowup * max_abs4(p0);

    var y = Phase(x0, p0);
    var k1 = ks_hamiltonian_rhs(mass, a, y.x, y.p);
    var r_prev = ks_radius(a, x0.y, x0.z, x0.w);
    var lambda = 0.0;
    var steps = 0u;
    var rejected = 0u;
    var h = 0.1 * step_cap(params, a, r_prev, r_plus, x0, k1.x);

    loop {
        if (steps + rejected >= params.max_steps) {
            return finish(STATUS_MAX_STEPS, y, k1.x, steps, rejected, lambda);
        }
        let h_try = min(h, step_cap(params, a, r_prev, r_plus, y.x, k1.x));
        let s = dopri5_step(mass, a, y, k1, h_try);
        let err = error_norm(y, s.y, s.err, params.rtol, params.atol);
        if (!is_finite_f32(err)) {
            return finish(STATUS_ERROR, s.y, k1.x, steps, rejected, lambda);
        }
        let accept = err <= 1.0 || h_try <= params.h_min;
        h = max(step_controller_next_h(h_try, err), params.h_min);
        if (!accept) {
            rejected += 1u;
            continue;
        }
        y = s.y;
        k1 = s.k_last;
        lambda = lambda + h_try;
        steps += 1u;
        let status = ray_status_after_step(params, y, k1, r_prev, r_escape, p_limit);
        if (status != STATUS_RUNNING) {
            return finish(status, y, k1.x, steps, rejected, lambda);
        }
        r_prev = ks_radius(a, y.x.y, y.x.z, y.x.w);
    }
    return finish(STATUS_ERROR, y, k1.x, steps, rejected, lambda);
}
