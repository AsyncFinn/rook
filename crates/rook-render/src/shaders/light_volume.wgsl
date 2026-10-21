// Fills the light voxels ahead of the camera (`lighting.wgsl` samples them),
// one layer per dispatch from the top: each voxel sums what every light
// listed in its cube's bucket of the lights' spatial hash writes into it
// (less what parts block, for lights with `Shadows`), and takes the sky's
// diffuse light (in alpha) from the layer above.

import package::lights::{Light, VOXEL, light_at_voxel};

struct Params {
    // xyz: the center of voxel (0, 0, 0); w: studs per hash cube.
    origin: vec4<f32>,
    // x: the hash's buckets - 1; yzw: voxels along each axis.
    size: vec4<u32>,
    // x: there are lights to fill; y: the sky's light to carry down; z:
    // `solid` holds the parts (some light has `Shadows`).
    flags: vec4<u32>,
};

@group(0) @binding(0) var<uniform> params: Params;
@group(0) @binding(1) var<storage, read> lights: array<Light>;
// Each bucket's (first entry, count) into `entries`, light indices.
@group(0) @binding(2) var<storage, read> buckets: array<vec2<u32>>;
@group(0) @binding(3) var<storage, read> entries: array<u32>;
@group(0) @binding(4) var voxels: texture_storage_3d<rgba16float, write>;
// x: the layer this dispatch fills.
@group(0) @binding(5) var<uniform> layer: vec4<u32>;
// One bit per voxel, x + nx * (z + nz * y): a part blocks it.
@group(0) @binding(6) var<storage, read> solid: array<u32>;
// The sky's light in the last two layers (by the layer's parity).
@group(0) @binding(7) var<storage, read_write> rows: array<f32>;
// Whether each column is open to the sky down to the last layer.
@group(0) @binding(8) var<storage, read_write> columns: array<u32>;

// The bucket a point's cube hashes to (`renderer/lighting/local.rs`, `bucket`).
fn bucket(p: vec3<f32>) -> vec2<u32> {
    let c = vec3<i32>(floor(p / params.origin.w));
    let h = (u32(c.x) * 73856093u) ^ (u32(c.y) * 19349663u) ^ (u32(c.z) * 83492791u);
    return buckets[h & params.size.x];
}

fn is_solid(x: u32, y: u32, z: u32) -> bool {
    let i = x + params.size.y * (z + params.size.w * y);
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

// How much of the sky's light reaches voxel (x, y, z), as Studio's voxels
// carry it (fitted to Studio 0.742 under roofs and beside walls,
// docs/behavior/rendering/materials.md, "The sky's diffuse light"): all of it where
// nothing blocks the column above, none in a blocked voxel, and elsewhere
// the open voxels of the layer above within two voxels each way,
// averaged (the outer ring at 0.15). So light spreads in under a roof
// from its edges, further the deeper it goes. Above and around the
// volume is open sky.
fn sky_at(x: u32, y: u32, z: u32) -> f32 {
    let nx = params.size.y;
    let nz = params.size.w;
    let column = x + nx * z;
    let top = y + 1u == params.size.z;
    let here = is_solid(x, y, z);
    let open = !here && (top || columns[column] != 0u);
    columns[column] = u32(open);
    var sky = 0.0;
    if open {
        sky = 1.0;
    } else if !here {
        let above = ((y + 1u) & 1u) * nx * nz;
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
                    sum += w * rows[above + u32(px) + nx * u32(pz)];
                    weight += w;
                }
            }
        }
        if weight > 0.0 {
            sky = sum / weight;
        }
    }
    rows[(y & 1u) * nx * nz + column] = sky;
    return sky;
}

@compute @workgroup_size(8, 8, 1)
fn fill(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= params.size.y || id.y >= params.size.w {
        return;
    }
    let cell = vec3(id.x, layer.x, id.y);
    var sum = vec3(0.0);
    if params.flags.x != 0u {
        let v = params.origin.xyz + vec3<f32>(cell) * VOXEL;
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
    textureStore(voxels, cell, vec4(sum, sky));
}
