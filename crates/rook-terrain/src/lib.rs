//! Smooth terrain: `Terrain.SmoothGrid`'s voxels ([`Grid`], decoded in
//! [`grid::decode`]), the surface Roblox builds from them ([`mesh`]) and the
//! grass blades on it ([`grass`]), shared by the renderer, physics and the
//! Terrain API's edits ([`grid::edit`], [`grid::cells`]).

pub mod grass;
pub mod grid;
mod material;
pub mod mesh;
mod random;

pub use grid::{CHUNK_VOXELS, Grid, VOXEL_STUDS, Voxel, chunk_of};
pub use material::{TerrainMaterial, all_packs};
pub use mesh::{ChunkMesh, Facing, Layer, MESH_REACH, Seam, chunks_to_mesh, mesh_chunk};

#[cfg(test)]
#[expect(
    clippy::disallowed_methods,
    reason = "tests set up files on the machine running them"
)]
mod tests;
