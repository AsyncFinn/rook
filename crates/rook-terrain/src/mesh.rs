//! The surface Roblox builds from the voxels: surface nets
//! (`docs/behavior/terrain/surface.md`). Each dual cell where solid meets air holds
//! one vertex, the mean of its edges' crossings; each crossing voxel edge
//! gets a quad of the solid voxel's material, split along [`DIAGONALS`].
//! Water is meshed the same way in a second pass and moved down
//! [`WATER_DROP`]. [`ChunkMesh::surfaces`] split the triangles by material
//! for collisions; [`ChunkMesh::draws`] by the layers drawn ([`draws`]).

use glam::{IVec3, Vec3};

use crate::TerrainMaterial;
use crate::grid::{CHUNK_VOXELS, Grid, VOXEL_STUDS, Voxel};

mod deform;
pub mod draws;
mod nets;

use deform::deform;
pub use draws::{Corner, Draw, Layer, Seam};
pub(crate) use nets::mesh_chunk_with;

/// One material's part of a chunk's surface, relative to the chunk's
/// corner ([`ChunkMesh::origin`]).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Surface {
    pub material: TerrainMaterial,
    /// Which of the material's textures its triangles take.
    pub facing: Facing,
    pub positions: Vec<[f32; 3]>,
    /// Smooth: the mean of the faces around each vertex, across chunks
    /// and materials; flat for the materials with hard edges.
    pub normals: Vec<[f32; 3]>,
    /// Three per triangle, counter-clockwise seen from outside.
    pub indices: Vec<u32>,
}

/// Which way a triangle faces, for its material's top, side and bottom
/// textures: up when its normal is closer to one of the five upward planes
/// Studio projects textures on (straight up and the four at 45 degrees)
/// than to a vertical one, that is more than 22.5 degrees up (observed: a
/// Grass ball shows grass down to about that far above its middle).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Facing {
    Top,
    #[default]
    Side,
    Bottom,
}

impl Facing {
    /// sin(22.5 degrees).
    const LIMIT: f32 = 0.382_683_43;

    pub fn of(normal: Vec3) -> Self {
        if normal.y > Self::LIMIT {
            Facing::Top
        } else if normal.y < -Self::LIMIT {
            Facing::Bottom
        } else {
            Facing::Side
        }
    }
}

/// The surface of one chunk: the quads of the voxel edges starting in it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ChunkMesh {
    pub chunk: IVec3,
    /// By material, solids first and water last.
    pub surfaces: Vec<Surface>,
    /// What's drawn, by layer, solids first and water last.
    pub draws: Vec<Draw>,
}

impl ChunkMesh {
    /// Where the chunk's corner is in the world.
    pub fn origin(&self) -> Vec3 {
        chunk_origin(self.chunk)
    }

    pub fn triangles(&self) -> usize {
        self.surfaces.iter().map(|s| s.indices.len() / 3).sum()
    }
}

pub fn chunk_origin(chunk: IVec3) -> Vec3 {
    (chunk * CHUNK_VOXELS).as_vec3() * VOXEL_STUDS
}

/// Which diagonal splits the quad of an edge along each axis (X, Y, Z):
/// true joins the cell before it on the first other axis and after it on
/// the second to the opposite one (the other axes in X, Y, Z order turned
/// to start after the edge's: Y and Z for X, Z and X for Y, X and Y for Z).
/// Measured: see `tests::diagonals`.
pub const DIAGONALS: [[bool; 2]; 3] = [[true; 2]; 3];

/// The chunks that can have a surface: every stored chunk, and the empty
/// ones just below stored ones on each axis (they own the edges into the
/// stored chunk). Chunks whose whole neighborhood is one uniform voxel
/// have none and are left out.
pub fn chunks_to_mesh(grid: &Grid) -> Vec<IVec3> {
    let mut out: Vec<IVec3> = grid
        .chunks()
        .flat_map(|c| [c, c - IVec3::X, c - IVec3::Y, c - IVec3::Z])
        .collect();
    out.sort_by_key(|c| (c.x, c.y, c.z));
    out.dedup();
    out.retain(|c| !all_uniform(grid, *c));
    out
}

/// Whether the 27 chunks around `chunk` are all the same uniform voxel.
fn all_uniform(grid: &Grid, chunk: IVec3) -> bool {
    let Some(first) = grid.uniform(chunk) else {
        return false;
    };
    (-1..=1).all(|z| {
        (-1..=1).all(|y| (-1..=1).all(|x| grid.uniform(chunk + IVec3::new(x, y, z)) == Some(first)))
    })
}

/// How far water's surface sits below where solid terrain's would: half a
/// voxel (Studio's terrain material table gives Water `"water": 0.5`;
/// measured: a lone full water voxel spans 3.333 to 0.667 studs below its
/// center, a full one-voxel slab 6 below to 2 above, its edge sloping as a
/// solid slab's does, 2 studs lower).
pub const WATER_DROP: f32 = VOXEL_STUDS / 2.0;

/// Voxels read before a chunk's first one: edges start one voxel before
/// it (for the normals of the faces just outside), and their cells one
/// more.
const PAD: i32 = 2;
/// How far outside a chunk, in voxels, its surface reads: changing a
/// voxel changes the surfaces of the chunks within this many voxels of it.
pub const MESH_REACH: i32 = PAD;

/// Mesh one chunk.
pub fn mesh_chunk(grid: &Grid, chunk: IVec3) -> ChunkMesh {
    mesh_chunk_with(grid, chunk, DIAGONALS)
}
