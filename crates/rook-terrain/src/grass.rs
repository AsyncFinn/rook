//! Grass blades: with `Terrain.Decoration` on, Studio grows blades on
//! Grass's tops (`docs/behavior/terrain.md`). Rook plants them on the
//! Grass triangles facing up, at random points fixed by where each
//! triangle is, in tiles of [`TILE_STUDS`] so the player builds blades only
//! near the camera. Their size and density are fitted to Studio 0.742
//! screenshots of a Grass strip beside a 4-stud ruler: each blade one flat
//! triangle up to 3.4 studs tall and about half a stud wide at the foot,
//! leaning every way; how much of the view they cover at each height side
//! on (within 0.06 at every height) and from above fits 2.3 blades per
//! square stud, 0.72 to 3.6 studs tall, leaning up to 52 degrees
//! (Approximation).

use glam::{IVec2, IVec3, Vec3};
use rook_types::{Color3uint8, MaterialSet, linear_to_srgb, srgb_to_linear};
use rustc_hash::FxHashMap;

use crate::random::Random;
use crate::{ChunkMesh, Facing, TerrainMaterial};

/// The side of a square tile of blades, in studs across X and Z.
pub const TILE_STUDS: f32 = 32.0;
/// Blades per square stud of ground.
const DENSITY: f32 = 2.3;
/// The tallest blade, in studs; the shortest is a fifth of it.
pub const BLADE_HEIGHT: f32 = 3.6;
/// A blade's width at its foot.
const BLADE_WIDTH: f32 = 0.45;

/// Grass's top texture's average colour, and how much of it its colour
/// tints (alpha): Studio's `FStringTerrainColorTable`, `avgAlbedo2022`.
const GRASS_AVERAGE: [u8; 4] = [196, 196, 189, 241];

/// How much darker blades are than the ground's average, in linear light
/// (measured in Studio: a blade is one flat colour, close to the ground's
/// average around it, by luminance 0.78 of it in the default colour and
/// 0.97 with Grass red; Rook's blades at 0.7 come out 0.82 of Rook's own
/// ground in the default colour; Approximation).
const BLADE_SHADE: f32 = 0.7;

/// The blades' colour (sRGB) for Grass's `MaterialColors` colour: the
/// ground's average (its texture's luma times [`TerrainMaterial::tint`]
/// where its tint mask is opaque, as the ground is drawn), darkened by
/// [`BLADE_SHADE`] in linear light.
pub fn blade_color(grass: Color3uint8) -> [f32; 3] {
    let [r, g, b, a] = GRASS_AVERAGE.map(|v| f32::from(v) / 255.0);
    let average = srgb_to_linear([r, g, b]);
    let luma = (average[0] + 2.0 * average[1] + average[2]) / 4.0;
    let tint = srgb_to_linear(TerrainMaterial::Grass.tint(grass, MaterialSet::Materials2022));
    linear_to_srgb(
        [0, 1, 2].map(|i| (average[i] + (luma * tint[i] - average[i]) * a) * BLADE_SHADE),
    )
}

/// Grass's upward triangles in world studs, by the tile their centre is in.
pub type GrassTops = FxHashMap<IVec2, Vec<[Vec3; 3]>>;

/// The tile a point is in.
pub fn tile_of(p: Vec3) -> IVec2 {
    IVec2::new(
        (p.x / TILE_STUDS).floor() as i32,
        (p.z / TILE_STUDS).floor() as i32,
    )
}

/// A tile's corner (Y 0), where its blades' mesh is placed.
pub fn tile_origin(tile: IVec2) -> Vec3 {
    Vec3::new(tile.x as f32, 0.0, tile.y as f32) * TILE_STUDS
}

/// Add a chunk's Grass top triangles to `tops`.
pub fn add_grass_tops(chunk: &ChunkMesh, tops: &mut GrassTops) {
    let origin = chunk.origin();
    for s in &chunk.surfaces {
        if s.material != TerrainMaterial::Grass || s.facing != Facing::Top {
            continue;
        }
        for tri in s.indices.as_chunks::<3>().0 {
            let t = tri.map(|i| origin + Vec3::from(s.positions[i as usize]));
            let centre = (t[0] + t[1] + t[2]) / 3.0;
            tops.entry(tile_of(centre)).or_default().push(t);
        }
    }
}

/// One tile's blades, relative to its [`tile_origin`]. Each vertex's UV is
/// (a number from 0 to 1 for its blade: its sway's phase, and which blades
/// thin out first far away; its height above the blade's foot in studs);
/// its normal points straight up, so every blade is one flat colour, as in
/// Studio (observed, on slopes too).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Blades {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub uvs: Vec<[f32; 2]>,
    pub indices: Vec<u32>,
}

impl Blades {
    pub fn count(&self) -> usize {
        self.positions.len() / 3
    }
}

/// The blades growing on `triangles` (a tile of [`GrassTops`]): each one
/// triangle from its foot to its tip, seen from both sides (observed:
/// Studio's blades are straight-sided spikes).
pub fn blades(tile: IVec2, triangles: &[[Vec3; 3]]) -> Blades {
    let origin = tile_origin(tile);
    let mut out = Blades::default();
    for t in triangles {
        let area = (t[1] - t[0]).cross(t[2] - t[0]).length() / 2.0;
        let centre = (t[0] + t[1] + t[2]) / 3.0;
        let mut r = Random::at((centre * 16.0).round().as_ivec3() + IVec3::new(0, 7, 0));
        let count = (area * DENSITY + r.next()) as usize;
        for _ in 0..count {
            // A uniform point on the triangle.
            let (mut u, mut v) = (r.next(), r.next());
            if u + v > 1.0 {
                (u, v) = (1.0 - u, 1.0 - v);
            }
            let foot = t[0] + (t[1] - t[0]) * u + (t[2] - t[0]) * v - origin;
            let height = BLADE_HEIGHT * (0.2 + 0.8 * r.next());
            let turn = r.next() * std::f32::consts::TAU;
            let across = Vec3::new(turn.cos(), 0.0, turn.sin()) * (BLADE_WIDTH / 2.0);
            // Leaning any way, up to about 52 degrees.
            let way = r.next() * std::f32::consts::TAU;
            let lean = Vec3::new(way.cos(), 0.0, way.sin()) * r.next() * 1.3 * height;
            let phase = r.next();
            let first = out.positions.len() as u32;
            let points = [
                (foot - across, 0.0),
                (foot + across, 0.0),
                (foot + lean + Vec3::Y * height, height),
            ];
            for (p, up) in points {
                out.positions.push(p.to_array());
                out.normals.push(Vec3::Y.to_array());
                out.uvs.push([phase, up]);
            }
            out.indices.extend([first, first + 1, first + 2]);
            out.indices.extend([first, first + 2, first + 1]);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use glam::IVec3;

    use super::*;
    use crate::{Grid, Voxel, chunks_to_mesh, mesh_chunk};

    /// The Grass tops of a full one-voxel slab `size` voxels square, of
    /// `material`.
    fn slab_tops(material: TerrainMaterial, size: i32) -> GrassTops {
        let mut grid = Grid::new();
        for z in 0..size {
            for x in 0..size {
                grid.set(IVec3::new(x, 0, z), Voxel::new(material, 255));
            }
        }
        let mut tops = GrassTops::default();
        for chunk in chunks_to_mesh(&grid) {
            add_grass_tops(&mesh_chunk(&grid, chunk), &mut tops);
        }
        tops
    }

    #[test]
    fn blades_grow_on_grass_tops_only() {
        // 16 by 16 voxels: two by two tiles, and a top flat but for its
        // edges, which slope down over the outer voxels.
        let tops = slab_tops(TerrainMaterial::Grass, 16);
        assert_eq!(tops.len(), 4);
        let mut count = 0;
        for (tile, triangles) in &tops {
            let b = blades(*tile, triangles);
            count += b.count();
            assert_eq!(b.indices.len(), b.count() * 6);
            let origin = tile_origin(*tile);
            for (p, uv) in b.positions.iter().zip(&b.uvs) {
                let p = origin + Vec3::from(*p);
                // The slab's top is 4 studs above its voxels' centres.
                if uv[1] == 0.0 && (6.0..58.0).contains(&p.x) && (6.0..58.0).contains(&p.z) {
                    assert!((p.y - 6.0).abs() < 1e-3, "foot at {p}");
                }
                assert!(uv[1] <= BLADE_HEIGHT);
            }
        }
        // About one blade per square stud of top, sloped edges and all.
        let area: f32 = tops
            .values()
            .flatten()
            .map(|t| (t[1] - t[0]).cross(t[2] - t[0]).length() / 2.0)
            .sum();
        assert!(
            (count as f32 / area - DENSITY).abs() < 0.1,
            "{count} blades"
        );
        // The same ground grows the same blades.
        let tile = *tops.keys().next().unwrap();
        assert_eq!(blades(tile, &tops[&tile]), blades(tile, &tops[&tile]));
        assert!(slab_tops(TerrainMaterial::Rock, 16).is_empty());
    }

    #[test]
    fn blades_are_darker_than_the_ground() {
        let c = blade_color(Color3uint8::new(111, 126, 62));
        assert!(c.iter().all(|v| (0.0..0.6).contains(v)), "{c:?}");
        // Olive, as Studio's ground is in the default colour: (82, 84, 43).
        assert!((c[0] - c[1]).abs() < 0.02 && c[1] > c[2] + 0.1, "{c:?}");
    }
}
