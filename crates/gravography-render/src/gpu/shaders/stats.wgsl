// stats.wgsl: frame statistics on the GPU (WP-12), replacing the viewer's blocking image readbacks.
// Per workgroup of 256 pixels: Σ ln Y and the count of lit pixels (Rust: tonemap::log_average_luminance, which
// skips Y ≤ 0 and non-finite Y), reduced in shared memory and written as one partial; ray-status counts of the
// G-buffer, counted in workgroup atomics and added to five global counters.

struct StatsParams {
    pixel_count: u32,
    _pad0: u32,
    _pad1: u32,
    _pad2: u32,
}

@group(0) @binding(0) var<uniform> params: StatsParams;
@group(0) @binding(1) var<storage, read> accum: array<vec4<f32>>;
@group(0) @binding(2) var<storage, read> gbuffer: array<GBufferTexel>;
@group(0) @binding(3) var<storage, read_write> partials: array<vec2<f32>>;
@group(0) @binding(4) var<storage, read_write> status_counts: array<atomic<u32>, 5>;

// Rec. 709 luminance weights (Rust: tonemap::LUMA).
const LUMA = vec3<f32>(0.2126, 0.7152, 0.0722);

var<workgroup> wg_log: array<vec2<f32>, 256>;
var<workgroup> wg_status: array<atomic<u32>, 5>;

@compute @workgroup_size(256)
fn stats_main(
    @builtin(global_invocation_id) gid: vec3<u32>,
    @builtin(local_invocation_index) li: u32,
    @builtin(workgroup_id) wid: vec3<u32>,
    @builtin(num_workgroups) nwg: vec3<u32>,
) {
    if (li < 5u) {
        atomicStore(&wg_status[li], 0u);
    }
    // 2D dispatch (x fastest) so large images stay within the per-dimension workgroup limit.
    let group = wid.y * nwg.x + wid.x;
    let i = group * 256u + li;
    var v = vec2<f32>(0.0);
    if (i < params.pixel_count) {
        let a = accum[i];
        let y = dot(a.xyz / max(a.w, 1.0), LUMA);
        if (y > 0.0 && is_finite_f32(y)) {
            v = vec2<f32>(log(y), 1.0);
        }
    }
    wg_log[li] = v;
    workgroupBarrier();
    if (i < params.pixel_count) {
        atomicAdd(&wg_status[min(gbuffer[i].status, 4u)], 1u);
    }
    for (var stride = 128u; stride > 0u; stride = stride >> 1u) {
        if (li < stride) {
            wg_log[li] = wg_log[li] + wg_log[li + stride];
        }
        workgroupBarrier();
    }
    if (li == 0u) {
        partials[group] = wg_log[0];
    }
    if (li < 5u) {
        let c = atomicLoad(&wg_status[li]);
        if (c > 0u) {
            atomicAdd(&status_counts[li], c);
        }
    }
}
