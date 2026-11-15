// Fills the light voxels ahead of the camera (`lighting.wgsl` samples them),
// one layer per dispatch from the top, only the tiles of it marked
// (`lighting/local/tiles.rs`): each voxel sums what every light
// listed in its cube's bucket of the lights' spatial hash writes into it
// (less what parts block, for lights with `Shadows`), and takes the sky's
// diffuse light (in alpha) from the layer above; and, when the sun shadows
// through the voxels, how much of the sun reaches each one.

import package::voxels::lights::{Light, VOXEL, light_at_voxel};
import package::voxels::voxel_marks::{slot_of, byte_of, by_soil, SOIL_SKY};

struct Params {
    // xyz: the center of voxel (0, 0, 0); w: studs per hash cube.
    origin: vec4<f32>,
    // x: the hash's buckets - 1; yzw: voxels along each axis.
    size: vec4<u32>,
    // xyz: where voxel (0, 0, 0) is kept in `solid`, `soils`, `skies`, the
    // sun cover and the voxels, which are all anchored to the world
    // (`renderer/lighting/local/blockers.rs`), so a volume that moves
    // keeps what it still covers.
    ring: vec4<u32>,
    // x: there are lights to fill; y: the sky's light to carry down; z:
    // `solid` holds the parts; w: the sun's light to take.
    flags: vec4<u32>,
    // xyz: towards the sun (unit).
    sun: vec4<f32>,
    // The sun's cone (`lighting/local/cover.rs`): x: its radius
    // where it starts, in voxels; y: how much wider it gets per voxel it
    // runs; z: the cover's last level.
    cone: vec4<f32>,
};

@group(0) @binding(0) var<uniform> params: Params;
@group(0) @binding(1) var<storage, read> lights: array<Light>;
// Each bucket's (first entry, count) into `entries`, light indices.
@group(0) @binding(2) var<storage, read> buckets: array<vec2<u32>>;
@group(0) @binding(3) var<storage, read> entries: array<u32>;
@group(0) @binding(4) var voxels: texture_storage_3d<rgba16float, write>;
// What one dispatch fills (`renderer/lighting/local.rs`, `LayerUniform`).
struct Layer {
    // x: the layer; y, z: the first tile along x and z the dispatch
    // covers.
    at: vec4<u32>,
    // A bit per tile of the layer, x + (tiles along x) z: whether to fill
    // it.
    tiles: array<vec4<u32>, 2>,
};

// Tiles' width along x and z, in voxels (`tiles.rs`, `TILE`).
const TILE: u32 = 16u;

@group(0) @binding(5) var<uniform> layer: Layer;
// One bit per voxel, at `slot`: a part blocks it.
@group(0) @binding(6) var<storage, read> solid: array<u32>;
// The sky's light in each voxel (at `slot`), or `OPEN` where nothing blocks
// the column above it: kept, so a fill of some tiles takes the layer above
// from the last fill wherever it didn't change.
@group(0) @binding(7) var<storage, read_write> skies: array<f32>;
// How much of the sun reaches each voxel (red).
@group(0) @binding(8) var sun_voxels: texture_storage_3d<rgba8unorm, write>;
// How much of the sun each voxel stops, four layers to a texel, with mips
// averaging each layer across x and z (`sun_cover.wgsl`).
@group(0) @binding(9) var cover: texture_2d_array<f32>;
// A byte per voxel, four to a word at `slot`: how full of solid terrain it
// is (255 full; `lighting/local/terrain.rs`).
@group(0) @binding(10) var<storage, read> soils: array<u32>;
// The cover's, wrapping around as the voxels do.
@group(0) @binding(11) var cover_sampler: sampler;

// In `skies`: open to the sky, so lit by all of it.
const OPEN: f32 = 2.0;

// The bucket a point's cube hashes to (`renderer/lighting/local.rs`, `bucket`).
fn bucket(p: vec3<f32>) -> vec2<u32> {
    let c = vec3<i32>(floor(p / params.origin.w));
    let h = (u32(c.x) * 73856093u) ^ (u32(c.y) * 19349663u) ^ (u32(c.z) * 83492791u);
    return buckets[h & params.size.x];
}

// Where voxel `c` of the volume is kept: the world's voxel index modulo
// the volume's size.
fn kept(c: vec3<u32>) -> vec3<u32> {
    return (c + params.ring.xyz) % params.size.yzw;
}

// Where voxel `c` of the volume is kept in `solid`, `soils` and `skies`.
fn slot(c: vec3<u32>) -> u32 {
    return slot_of(kept(c), params.size.yzw);
}

fn is_solid(x: u32, y: u32, z: u32) -> bool {
    let i = slot(vec3(x, y, z));
    return ((solid[i >> 5u] >> (i & 31u)) & 1u) != 0u;
}

// Whether a part blocks the voxel `p` is in; outside the volume nothing
// does.
fn blocks(p: vec3<f32>) -> bool {
    let c = round((p - params.origin.xyz) / VOXEL);
    let n = vec3<f32>(params.size.yzw);
    if any(c < vec3(0.0)) || any(c >= n) {
        return false;
    }
    return is_solid(u32(c.x), u32(c.y), u32(c.z));
}

// How much of `light` reaches the voxel centered at `v` past the parts
// (Approximation of Studio's voxel shadows, docs/behavior/lighting/local-lights.md):
// nine parallel rays, from `v` to the light and from the voxels around
// `v` across the ray to as far around the light, weighted 1-2-1 each
// way; a ray is blocked by any blocked voxel it crosses, but not by the
// light's own part. So a wall blocks all of it, a one-voxel post about
// half, and shadows have soft edges.
fn unblocked(light: Light, v: vec3<f32>) -> f32 {
    let to = light.position.xyz - v;
    let d = length(to);
    if d < VOXEL {
        return 1.0;
    }
    let dir = to / d;
    let up = select(vec3(0.0, 1.0, 0.0), vec3(1.0, 0.0, 0.0), abs(dir.y) > 0.9);
    let a = normalize(cross(dir, up)) * VOXEL;
    let b = cross(dir, a);
    var open = 0.0;
    for (var i = -1; i <= 1; i++) {
        for (var j = -1; j <= 1; j++) {
            let offset = a * f32(i) + b * f32(j);
            let start = v + offset;
            let end = light.position.xyz + offset;
            let steps = ceil(d / VOXEL);
            var clear = true;
            for (var k = 0.0; k < steps; k += 1.0) {
                let p = mix(start, end, k / steps);
                if all(p >= light.part_min.xyz) && all(p <= light.part_max.xyz) {
                    break;
                }
                if blocks(p) {
                    clear = false;
                    break;
                }
            }
            if clear {
                open += select(2.0, 1.0, i != 0) * select(2.0, 1.0, j != 0);
            }
        }
    }
    return open / 16.0;
}

// How full of solid terrain voxel `c` is, 0 to 1.
fn soil_at(c: vec3<i32>) -> f32 {
    let i = slot(vec3<u32>(c));
    return byte_of(soils[i >> 2u], i);
}

// How much of the sun voxel layer `y` stops around `p` (across x and z, in
// voxels, voxel k's center at k + 0.5), averaged over 2^lod voxels each
// way. The cover wraps around as the voxels do, so a footprint stays a
// texel inside the volume's faces, not blending in the far side's.
fn cover_at(p: vec2<f32>, y: u32, lod: f32) -> f32 {
    let n = vec2<f32>(params.size.yw);
    let texel = exp2(ceil(lod));
    let q = clamp(p, vec2(texel * 0.5), n - texel * 0.5);
    let uv = (q + vec2<f32>(params.ring.xz)) / n;
    let k = (y + params.ring.y) % params.size.z;
    return textureSampleLevel(cover, cover_sampler, uv, k / 4u, lod)[k % 4u];
}

// How much of the sun reaches voxel `cell`, as Studio's voxels shadow it
// (fitted to Studio 0.747, docs/behavior/lighting/sun.md): through a cone
// towards the sun that widens with distance (`params.cone`), so a
// blocker's shadow softens the further it falls, and narrow ones far up
// all but vanish. Each layer of voxels the cone crosses, from the one
// above, stops the share of the cone's footprint in it its cover holds; a
// run of layers stopping some counts as one blocker, the one stopping
// most (a slab's shadow is the same however thick), and blockers apart
// each take their share. Outside the volume nothing blocks, as in Studio,
// where a part above the grid casts nothing.
fn sunlit(cell: vec3<u32>) -> f32 {
    let d = params.sun.xyz;
    let n = vec2<f32>(params.size.yw);
    let up = max(abs(d.y), 1e-3);
    let step = select(-1, 1, d.y >= 0.0);
    // Across x and z per layer.
    let along = d.xz / up;
    let start = vec2<f32>(cell.xz) + 0.5;
    var through = 1.0;
    var run = 0.0;
    for (var k = 1; ; k++) {
        let y = i32(cell.y) + step * k;
        let at = start + along * f32(k);
        if y < 0 || y >= i32(params.size.z) || any(at < vec2(0.0)) || any(at >= n) {
            break;
        }
        // The footprint is at least as wide as the ray's step from layer
        // to layer, so a low sun's skips no voxel.
        let r = max(params.cone.x + params.cone.y * f32(k) / up, 0.5 * length(along));
        let lod = clamp(log2(2.0 * r), 0.0, params.cone.z);
        let stops = cover_at(at, u32(y), lod);
        if stops > 0.0 {
            run = max(run, stops);
        } else if run > 0.0 {
            through *= 1.0 - run;
            run = 0.0;
            if through < 1.0 / 512.0 {
                return 0.0;
            }
        }
    }
    return through * (1.0 - run);
}

// How much of the sky's light reaches voxel (x, y, z), as Studio's voxels
// carry it (fitted to Studio 0.742 under roofs and beside walls,
// docs/behavior/rendering/materials.md, "The sky's diffuse light"): all of it where
// nothing blocks the column above, none in a blocked voxel, and elsewhere
// the open voxels of the layer above within two voxels each way,
// averaged (the outer ring at 0.15). So light spreads in under a roof
// from its edges, further the deeper it goes. A voxel holding terrain
// lets its share through (`SOIL_SKY`) of what comes from above. Above and
// around the volume is open sky.
fn sky_at(x: u32, y: u32, z: u32) -> f32 {
    let nx = params.size.y;
    let nz = params.size.w;
    let at = slot(vec3(x, y, z));
    let top = y + 1u == params.size.z;
    let here = is_solid(x, y, z);
    let through = by_soil(SOIL_SKY, soil_at(vec3<i32>(vec3(x, y, z))));
    let open = top || skies[slot(vec3(x, y + 1u, z))] == OPEN;
    if !here && open && through == 1.0 {
        skies[at] = OPEN;
        return 1.0;
    }
    var sky = 0.0;
    if !here && open {
        sky = through;
    } else if !here {
        var sum = 0.0;
        var weight = 0.0;
        for (var dz = -2; dz <= 2; dz++) {
            for (var dx = -2; dx <= 2; dx++) {
                let w = select(1.0, 0.15, abs(dx) == 2) * select(1.0, 0.15, abs(dz) == 2);
                let px = i32(x) + dx;
                let pz = i32(z) + dz;
                if px < 0 || pz < 0 || px >= i32(nx) || pz >= i32(nz) {
                    sum += w;
                    weight += w;
                } else if !is_solid(u32(px), y + 1u, u32(pz)) {
                    sum += w * min(skies[slot(vec3(u32(px), y + 1u, u32(pz)))], 1.0);
                    weight += w;
                }
            }
        }
        if weight > 0.0 {
            sky = through * sum / weight;
        }
    }
    skies[at] = sky;
    return sky;
}

@compute @workgroup_size(8, 8, 1)
fn fill(@builtin(global_invocation_id) id: vec3<u32>) {
    let x = layer.at.y * TILE + id.x;
    let z = layer.at.z * TILE + id.y;
    if x >= params.size.y || z >= params.size.w {
        return;
    }
    let tile = x / TILE + params.size.y / TILE * (z / TILE);
    if ((layer.tiles[tile >> 7u][(tile >> 5u) & 3u] >> (tile & 31u)) & 1u) == 0u {
        return;
    }
    let cell = vec3(x, layer.at.x, z);
    let v = params.origin.xyz + vec3<f32>(cell) * VOXEL;
    var sum = vec3(0.0);
    if params.flags.x != 0u {
        let range = bucket(v);
        for (var i = 0u; i < range.y; i++) {
            let light = lights[entries[range.x + i]];
            var lit = light_at_voxel(light, v);
            if light.kind.z > 0.5 && params.flags.z != 0u && any(lit > vec3(0.0)) {
                lit *= unblocked(light, v);
            }
            sum += lit;
        }
    }
    var sky = 1.0;
    if params.flags.y != 0u {
        sky = sky_at(cell.x, cell.y, cell.z);
    }
    textureStore(voxels, kept(cell), vec4(sum, sky));
    if params.flags.w != 0u {
        textureStore(sun_voxels, kept(cell), vec4(sunlit(cell), 0.0, 0.0, 1.0));
    }
}
