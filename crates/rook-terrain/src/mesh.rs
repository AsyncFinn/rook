//! The surface Roblox builds from the voxels: surface nets
//! (`docs/behavior/terrain/surface.md`). Each dual cell where solid meets air holds
//! one vertex, the mean of its edges' crossings; each crossing voxel edge
//! gets a quad of the solid voxel's material, split along [`DIAGONALS`].
//! Water is meshed the same way in a second pass and moved down
//! [`WATER_DROP`]. [`ChunkMesh::surfaces`] split the triangles by material
//! for collisions; [`ChunkMesh::draws`] by the layers drawn ([`draws`]).
//! Far terrain is meshed the same way from coarser voxels ([`Detail`],
//! `crate::lod`), its chunks hung with [`skirts`] that hide the cracks
//! where rings of different detail meet.

use glam::{IVec3, Vec3};

use crate::TerrainMaterial;
use crate::grid::{CHUNK_VOXELS, Grid, VOXEL_STUDS, Voxel};

mod deform;
pub mod draws;
mod nets;
mod skirts;

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
    /// The chunk, in chunks of its level.
    pub chunk: IVec3,
    /// Its level of detail: 0 for the 4-stud voxels, each level after
    /// twice as coarse ([`Detail`]).
    pub level: u8,
    /// By material, solids first and water last.
    pub surfaces: Vec<Surface>,
    /// What's drawn, by layer, solids first and water last.
    pub draws: Vec<Draw>,
}

impl ChunkMesh {
    /// Where the chunk's corner is in the world.
    pub fn origin(&self) -> Vec3 {
        chunk_origin_at(self.chunk, self.level)
    }

    pub fn triangles(&self) -> usize {
        self.surfaces.iter().map(|s| s.indices.len() / 3).sum()
    }
}

pub fn chunk_origin(chunk: IVec3) -> Vec3 {
    chunk_origin_at(chunk, 0)
}

/// Where the corner of chunk `chunk` of `level` is in the world.
pub fn chunk_origin_at(chunk: IVec3, level: u8) -> Vec3 {
    (chunk * CHUNK_VOXELS).as_vec3() * Detail::at(level).voxel_studs()
}

/// How a chunk is meshed: at which level of detail, and whether it's hung
/// with skirts (drawn chunks are, since a neighbor may be coarser or
/// finer; collisions use the bare 4-stud surface).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Detail {
    pub level: u8,
    pub skirts: bool,
}

impl Detail {
    /// The 4-stud surface, bare.
    pub const FULL: Detail = Detail {
        level: 0,
        skirts: false,
    };

    /// `level`, as drawn: with skirts.
    pub fn at(level: u8) -> Self {
        Detail {
            level,
            skirts: true,
        }
    }

    /// A voxel's edge in studs at this level.
    pub fn voxel_studs(self) -> f32 {
        VOXEL_STUDS * f32::from(1u16 << self.level)
    }
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
    out.retain(|c| can_have_surface(grid, *c));
    out
}

/// Whether chunk `chunk` of `grid` can have a surface: some of the 27
/// chunks around it differ.
pub fn can_have_surface(grid: &Grid, chunk: IVec3) -> bool {
    !all_uniform(grid, chunk)
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

/// How far water's surface is moved down at voxels `scale` studs on a
/// side: [`WATER_DROP`], and on coarser voxels also what their surface
/// gains by reaching half a coarser voxel out, so open water lies at the
/// same height in every ring and meets itself without a step or a crack
/// (Rook's choice: skirts would show through it; Studio's far water
/// wasn't measured).
fn water_drop(scale: f32) -> f32 {
    WATER_DROP + (scale - VOXEL_STUDS) / 2.0
}

/// Voxels read before a chunk's first one: edges start one voxel before
/// it (for the normals of the faces just outside), and their cells one
/// more.
const PAD: i32 = 2;
/// How far outside a chunk, in voxels, its surface reads: changing a
/// voxel changes the surfaces of the chunks within this many voxels of it.
pub const MESH_REACH: i32 = PAD;

/// Mesh one chunk of the 4-stud voxels, bare (for collisions and tools).
pub fn mesh_chunk(grid: &Grid, chunk: IVec3) -> ChunkMesh {
    mesh_chunk_with(grid, chunk, Detail::FULL, DIAGONALS)
}

/// Mesh chunk `chunk` of `grid`, whose voxels are `detail`'s level's.
pub fn mesh_chunk_at(grid: &Grid, chunk: IVec3, detail: Detail) -> ChunkMesh {
    mesh_chunk_with(grid, chunk, detail, DIAGONALS)
}
