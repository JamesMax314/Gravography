// test_rays.wgsl: compute entry points for the GPU ↔ CPU parity harness (WP-07). One invocation per test ray.

// Input ray (Rust: GpuRay; 32 bytes).
struct Ray {
    x: vec4<f32>,
    p: vec4<f32>,
}

@group(0) @binding(0) var<uniform> params: TraceParams;
@group(0) @binding(1) var<storage, read> rays: array<Ray>;
@group(0) @binding(2) var<storage, read_write> results: array<RayResult>;
// Right-hand side evaluations for eval_rhs (dx, dp per ray).
@group(0) @binding(3) var<storage, read_write> rhs_out: array<Phase>;

@compute @workgroup_size(64)
fn trace_rk4_main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let i = gid.x;
    if (i >= params.ray_count) {
        return;
    }
    results[i] = trace_ray_rk4(params, rays[i].x, rays[i].p);
}

@compute @workgroup_size(64)
fn eval_rhs_main(@builtin(global_invocation_id) gid: vec3<u32>) {
    let i = gid.x;
    if (i >= params.ray_count) {
        return;
    }
    rhs_out[i] = ks_hamiltonian_rhs(params.mass, params.spin, rays[i].x, rays[i].p);
}
