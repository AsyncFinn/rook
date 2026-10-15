//! Grass blades: with `Terrain.Decoration` on, Studio grows blades on
//! Grass's tops (`docs/behavior/terrain.md`). Rook plants them on the
//! Grass triangles facing up, at random points fixed by where each
//! triangle is, in tiles of [`TILE_STUDS`] so the player builds blades only
//! near the camera. Their size and density are fitted to Studio 0.742
//! screenshots of a Grass strip beside a 4-stud ruler: blades 1.5 to 3.4
//! studs tall, about half a stud wide at the foot, leaning every way,
//! about one per square stud (Approximation).

use glam::{IVec2, IVec3, Vec3};
use rook_types::{Color3uint8, linear_to_srgb, srgb_to_linear};
use rustc_hash::FxHashMap;

use crate::random::Random;
use crate::{ChunkMesh, Facing, TerrainMaterial};

/// The side of a square tile of blades, in studs across X and Z.
pub const TILE_STUDS: f32 = 32.0;
/// Blades per square stud of ground.
const DENSITY: f32 = 1.0;
/// The tallest blade, in studs; the shortest is 0.45 of it.
pub const BLADE_HEIGHT: f32 = 3.3;
/// A blade's width at its foot.
const BLADE_WIDTH: f32 = 0.45;

/// Grass's top texture's average colour, and how much of it its colour
/// tints (alpha): Studio's `FStringTerrainColorTable`, `avgAlbedo2022`.
const GRASS_AVERAGE: [u8; 4] = [196, 196, 189, 241];

/// How much darker blades are than the ground's average (observed: in
/// Studio a blade is one flat colour about 0.73 of the ground's brightness
/// in sRGB; Approximation).
const BLADE_SHADE: f32 = 0.5;

/// The blades' colour (sRGB) for Grass's `MaterialColors` colour: the
/// ground's average (its texture tinted as a tint mask is), darkened by
/// [`BLADE_SHADE`] in linear light.
pub fn blade_color(grass: Color3uint8) -> [f32; 3] {
    let [r, g, b, a] = GRASS_AVERAGE.map(|v| f32::from(v) / 255.0);
    let average = srgb_to_linear([r, g, b]);
    let tint = srgb_to_linear([grass.r, grass.g, grass.b].map(|v| f32::from(v) / 255.0));
    linear_to_srgb([0, 1, 2].map(|i| average[i] * (1.0 + (tint[i] - 1.0) * a) * BLADE_SHADE))
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
/// thin out first far away; its height above the blade's foot in studs); its normal is the ground's, so blades light as the
/// ground under them does.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Blades {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub uvs: Vec<[f32; 2]>,
    pub indices: Vec<u32>,
}

impl Blades {
    pub fn count(&self) -> usize {
        self.positions.len() / 5
    }
}

/// The blades growing on `triangles` (a tile of [`GrassTops`]): each a
/// tapering, bending strip of five vertices, seen from both sides.
pub fn blades(tile: IVec2, triangles: &[[Vec3; 3]]) -> Blades {
    let origin = tile_origin(tile);
    let mut out = Blades::default();
    for t in triangles {
        let edge = (t[1] - t[0]).cross(t[2] - t[0]);
        let area = edge.length() / 2.0;
        let normal = edge.normalize_or(Vec3::Y);
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
            let height = BLADE_HEIGHT * (0.45 + 0.55 * r.next());
            let turn = r.next() * std::f32::consts::TAU;
            let across = Vec3::new(turn.cos(), 0.0, turn.sin()) * (BLADE_WIDTH / 2.0);
            // Leaning out of its own plane, up to about 30 degrees.
            let lean = Vec3::new(-turn.sin(), 0.0, turn.cos()) * (r.next() - 0.5) * 1.2 * height;
            let phase = r.next();
            let first = out.positions.len() as u32;
            let mid = foot + lean * 0.3 + Vec3::Y * height * 0.5;
            let points = [
                (foot - across, 0.0),
                (foot + across, 0.0),
                (mid - across * 0.6, height * 0.5),
                (mid + across * 0.6, height * 0.5),
                (foot + lean + Vec3::Y * height, height),
            ];
            for (p, up) in points {
                out.positions.push(p.to_array());
                out.normals.push(normal.to_array());
                out.uvs.push([phase, up]);
            }
            for [a, b, c] in [[0, 1, 2], [1, 3, 2], [2, 3, 4]] {
                out.indices.extend([first + a, first + b, first + c]);
                out.indices.extend([first + a, first + c, first + b]);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests;
