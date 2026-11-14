// How the light voxels' marks are kept (`renderer/lighting/local/blockers.rs`),
// and what terrain in a voxel does to light: shared by the fill pass
// (`light_volume.wgsl`) and the sun cover (`sun_cover.wgsl`).

// Voxels across a brick (`PIECE` in `renderer/lighting/local/blockers.rs`).
const BRICK: u32 = 16u;

// Where the voxel kept at `r` (the world's voxel index modulo the volume's
// size `n`) is in the blockers' bits, the terrain bytes and the sky's
// light: in its brick (16 x 16 voxels across, the volume's height, laid
// out together), x + 16 * (z + 16 * y) within; as `slot_of` in
// `renderer/lighting/local/blockers.rs`.
fn slot_of(r: vec3<u32>, n: vec3<u32>) -> u32 {
    let brick = r.x / BRICK + n.x / BRICK * (r.z / BRICK);
    let within = r.x % BRICK + BRICK * (r.z % BRICK + BRICK * r.y);
    return brick * BRICK * BRICK * n.y + within;
}

// Byte `i` of a word's four (kept four to a word at slot `4w + i`), 0 to 1.
fn byte_of(word: u32, i: u32) -> f32 {
    return f32((word >> ((i & 3u) * 8u)) & 255u) / 255.0;
}

// How terrain stops light, by how full of it a voxel is (fitted to Studio,
// docs/behavior/lighting/sun.md and docs/behavior/rendering/materials.md):
// between these occupancies, linearly in between.
const SOIL_AT = array<f32, 11>(0.0, 0.05, 0.1, 0.15, 0.25, 0.35, 0.5, 0.75, 0.9, 0.95, 1.0);
// How much of the sun it stops: all of it from 0.35 up, as a part does.
const SOIL_SUN = array<f32, 11>(0.0, 0.06, 0.15, 0.23, 0.54, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0);
// How much of the sky's light it lets through: little is stopped until the
// voxel is nearly full, and all of it at full.
const SOIL_SKY = array<f32, 11>(1.0, 1.0, 1.0, 1.0, 1.0, 0.98, 0.94, 0.7, 0.38, 0.22, 0.0);

// `table` at occupancy `soil`, between the knots at `SOIL_AT`.
fn by_soil(table: array<f32, 11>, soil: f32) -> f32 {
    var at = SOIL_AT;
    var values = table;
    var i = 1u;
    while i < 10u && soil > at[i] {
        i += 1u;
    }
    let t = clamp((soil - at[i - 1u]) / (at[i] - at[i - 1u]), 0.0, 1.0);
    return mix(values[i - 1u], values[i], t);
}
