// camera.wgsl: camera rays on the GPU. Ports of gravography_render::camera::Projection::direction (rectilinear),
// gravography_physics::Tetrad::to_coordinate, gravography_physics::camera_ray_momentum and the ray part of
// gravography_render::cpu::CpuRenderer::cast (docs/physics.md § "Camera and projection", § "Shading").

// Effect flags (Rust: gravography_render::frame::Effects).
const FLAG_LENSING: u32 = 1u;
const FLAG_FREQUENCY_SHIFT: u32 = 2u;

// Camera uniform (Rust: gravography_render::gpu::GpuCamera; 112 bytes).
struct Camera {
    position: vec4<f32>,
    // Tetrad legs e_(a)^μ: u, forward, left, up (look rotation and boost already applied).
    e0: vec4<f32>,
    e1: vec4<f32>,
    e2: vec4<f32>,
    e3: vec4<f32>,
    width: u32,
    height: u32,
    // tan(fov/2) of the rectilinear projection (vertical field of view).
    tan_half_fov: f32,
    flags: u32,
    // Sub-pixel sample offset in [0, 1)² (pixel (i, j) is traced at (i + jitter.x, j + jitter.y)).
    jitter: vec2<f32>,
    _pad: vec2<u32>,
}

// Unit direction (forward, left, up) through continuous pixel coordinate (x, y) (Rust: Projection::direction).
fn projection_direction(cam: Camera, x: f32, y: f32) -> vec3<f32> {
    let t = cam.tan_half_fov;
    let w = f32(cam.width);
    let h = f32(cam.height);
    return normalize(vec3<f32>(1.0, -(2.0 * x / w - 1.0) * t * w / h, (1.0 - 2.0 * y / h) * t));
}

// Σ_a c^a e_(a) (Rust: Tetrad::to_coordinate).
fn tetrad_to_coordinate(cam: Camera, c: vec4<f32>) -> vec4<f32> {
    return c.x * cam.e0 + c.y * cam.e1 + c.z * cam.e2 + c.w * cam.e3;
}

// p = g(k), k = −e_(0) + nⁱ e_(i) (Rust: camera_ray_momentum).
fn camera_ray_momentum(params: TraceParams, cam: Camera, n: vec3<f32>) -> vec4<f32> {
    let k = tetrad_to_coordinate(cam, vec4<f32>(-1.0, n));
    return ks_g_lo(params.mass, params.spin, cam.position) * k;
}
