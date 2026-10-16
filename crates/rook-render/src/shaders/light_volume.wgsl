// Fills the light voxels around the camera (`lighting.wgsl` samples them):
// each voxel sums what every light listed in its cube's bucket of the
// lights' spatial hash writes into it.

import package::lights::{Light, VOXEL, light_at_voxel};

struct Params {
    // xyz: the centre of voxel (0, 0, 0); w: studs per hash cube.
    origin: vec4<f32>,
    // x: the hash's buckets - 1; yzw: voxels along each axis.
    size: vec4<u32>,
};

@group(0) @binding(0) var<uniform> params: Params;
@group(0) @binding(1) var<storage, read> lights: array<Light>;
// Each bucket's (first entry, count) into `entries`, light indices.
@group(0) @binding(2) var<storage, read> buckets: array<vec2<u32>>;
@group(0) @binding(3) var<storage, read> entries: array<u32>;
@group(0) @binding(4) var voxels: texture_storage_3d<rgba16float, write>;

// The bucket a point's cube hashes to (`renderer/lights.rs`, `bucket`).
fn bucket(p: vec3<f32>) -> vec2<u32> {
    let c = vec3<i32>(floor(p / params.origin.w));
    let h = (u32(c.x) * 73856093u) ^ (u32(c.y) * 19349663u) ^ (u32(c.z) * 83492791u);
    return buckets[h & params.size.x];
}

@compute @workgroup_size(4, 4, 4)
fn fill(@builtin(global_invocation_id) id: vec3<u32>) {
    if any(id >= params.size.yzw) {
        return;
    }
    let v = params.origin.xyz + vec3<f32>(id) * VOXEL;
    let range = bucket(v);
    var sum = vec3(0.0);
    for (var i = 0u; i < range.y; i++) {
        sum += light_at_voxel(lights[entries[range.x + i]], v);
    }
    textureStore(voxels, id, vec4(sum, 1.0));
}
