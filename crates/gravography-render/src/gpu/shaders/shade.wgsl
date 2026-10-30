// shade.wgsl: shading pass (docs/PRD.md §4.3 step 3). Per pixel: footprint from neighbouring G-buffer escape
// directions (finite differences over one pixel, the GPU form of CpuRenderer::footprint), sky sample, XYZ → linear
// sRGB, added to the accumulation buffer (rgb sum, sample count in w).

@group(0) @binding(0) var<uniform> shade: ShadeParams;
@group(0) @binding(1) var<uniform> camera: Camera;
@group(0) @binding(2) var<storage, read> gbuffer: array<GBufferTexel>;
@group(0) @binding(3) var sky_cube: texture_2d_array<f32>;
@group(0) @binding(4) var<storage, read> lut: array<vec4<f32>>;
@group(0) @binding(5) var<storage, read_write> accum: array<vec4<f32>>;

// Solid angle per pixel² at continuous pixel (x, y), rectilinear: Δ² cos³θ (Rust: Projection::pixel_solid_angle).
fn pixel_solid_angle(x: f32, y: f32) -> f32 {
    let pitch = 2.0 * camera.tan_half_fov / f32(camera.height);
    let c = projection_direction(camera, x, y).x;
    return pitch * pitch * c * c * c;
}

struct Axis {
    ok: bool,
    j: vec3<f32>,
}

// Jacobian column along (dx, dy) from the neighbour at +1 pixel, else −1 pixel (Rust: CpuRenderer::footprint
// with ε = 1 px; the neighbour was traced at the same sub-pixel jitter).
fn footprint_axis(px: i32, py: i32, dx: i32, dy: i32, d: vec3<f32>) -> Axis {
    let w = i32(camera.width);
    let h = i32(camera.height);
    for (var k = 0; k < 2; k++) {
        let sign = select(-1, 1, k == 0);
        let nx = px + sign * dx;
        let ny = py + sign * dy;
        if (nx >= 0 && nx < w && ny >= 0 && ny < h) {
            let t = gbuffer[u32(ny * w + nx)];
            if (t.status == STATUS_ESCAPED) {
                return Axis(true, (t.dir_g.xyz - d) / f32(sign));
            }
        }
    }
    return Axis(false, vec3<f32>(0.0));
}

@compute @workgroup_size(8, 8)
fn shade_main(@builtin(global_invocation_id) gid: vec3<u32>) {
    if (gid.x >= camera.width || gid.y >= camera.height) {
        return;
    }
    let i = gid.y * camera.width + gid.x;
    let t = gbuffer[i];
    var rgb = vec3<f32>(0.0);
    if (t.status == STATUS_ESCAPED) {
        let d = t.dir_g.xyz;
        let ax = footprint_axis(i32(gid.x), i32(gid.y), 1, 0, d);
        let ay = footprint_axis(i32(gid.x), i32(gid.y), 0, 1, d);
        var sr: f32;
        if (ax.ok && ay.ok) {
            sr = length(cross(ax.j, ay.j));
        } else if (ax.ok) {
            sr = dot(ax.j, ax.j);
        } else if (ay.ok) {
            sr = dot(ay.j, ay.j);
        } else {
            sr = pixel_solid_angle(f32(gid.x) + camera.jitter.x, f32(gid.y) + camera.jitter.y);
        }
        // Each of the N samples represents 1/N of the pixel area.
        sr = sr / f32(shade.samples);
        rgb = xyz_to_linear_srgb(sky_shade(d, sr, t.dir_g.w));
    }
    if (shade.sample_index == 0u) {
        accum[i] = vec4<f32>(rgb, 1.0);
    } else {
        accum[i] = accum[i] + vec4<f32>(rgb, 1.0);
    }
}
