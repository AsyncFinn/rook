// Builds the sun cover (`renderer/lighting/local/cover.rs`): how much of
// the sun each light voxel stops, all of it where a part blocks the voxel
// and terrain's share (`SOIL_SUN`) elsewhere; four layers to a texel (array
// layer g holds layers 4g to 4g + 3 in red, green, blue and alpha), at the
// voxels' slots across x and z, with mips that average each layer over
// 2^L x 2^L voxels for the fill pass's cones (`sunlit` in
// `light_volume.wgsl`). One dispatch per level, over the tiles that
// changed.

import package::voxels::voxel_marks::{slot_of, byte_of, by_soil, SOIL_SUN};

struct Job {
    // xyz: voxels along each axis of the volume; w: the level made.
    size: vec4<u32>,
    // xy: the first tile along x and z dispatched (kept).
    at: vec4<u32>,
    // A bit per tile (kept), x + 16 z: whether it changed.
    tiles: array<vec4<u32>, 2>,
};

// Tiles' width along x and z, in voxels (`tiles.rs`, `TILE`).
const TILE: u32 = 16u;

@group(0) @binding(0) var<uniform> job: Job;
// One bit per voxel, at `slot_of`: a part blocks it.
@group(0) @binding(1) var<storage, read> solid: array<u32>;
// A byte per voxel, four to a word at `slot_of`: how full of solid
// terrain it is (`renderer/lighting/local/terrain.rs`).
@group(0) @binding(2) var<storage, read> soils: array<u32>;
// The level below the one made (`down`).
@group(0) @binding(3) var below: texture_2d_array<f32>;
@group(0) @binding(4) var cover: texture_storage_2d_array<rgba8unorm, write>;

// The texel of level `job.size.w` thread `id` makes (x, z, array layer),
// or -1 where its tile didn't change.
fn texel(id: vec3<u32>) -> vec3<i32> {
    let level = job.size.w;
    let n = job.size.xz >> vec2(level);
    let at = (job.at.xy * (TILE >> level) + id.xy) % n;
    let tile = (at << vec2(level)) / TILE;
    let t = tile.x + (job.size.x / TILE) * tile.y;
    if ((job.tiles[t >> 7u][(t >> 5u) & 3u] >> (t & 31u)) & 1u) == 0u {
        return vec3(-1);
    }
    return vec3<i32>(vec3(at, id.z));
}

@compute @workgroup_size(8, 8, 1)
fn level0(@builtin(global_invocation_id) id: vec3<u32>) {
    let t = texel(id);
    if t.x < 0 {
        return;
    }
    let n = job.size.xyz;
    var stops = vec4(0.0);
    for (var c = 0u; c < 4u; c++) {
        let i = slot_of(vec3(u32(t.x), id.z * 4u + c, u32(t.y)), n);
        if ((solid[i >> 5u] >> (i & 31u)) & 1u) != 0u {
            stops[c] = 1.0;
        } else {
            stops[c] = by_soil(SOIL_SUN, byte_of(soils[i >> 2u], i));
        }
    }
    textureStore(cover, t.xy, t.z, stops);
}

@compute @workgroup_size(8, 8, 1)
fn down(@builtin(global_invocation_id) id: vec3<u32>) {
    let t = texel(id);
    if t.x < 0 {
        return;
    }
    let corner = t.xy * 2;
    let sum = textureLoad(below, corner, t.z, 0) + textureLoad(below, corner + vec2(1, 0), t.z, 0)
        + textureLoad(below, corner + vec2(0, 1), t.z, 0) + textureLoad(below, corner + vec2(1, 1), t.z, 0);
    textureStore(cover, t.xy, t.z, sum * 0.25);
}
