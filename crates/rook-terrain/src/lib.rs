//! Smooth terrain: `Terrain.SmoothGrid`'s voxels ([`Grid`], decoded in
//! [`decode`]), the surface Roblox builds from them ([`mesh`]) and the
//! grass blades on it ([`grass`]), shared by the renderer, physics and the
//! Terrain API's edits ([`edit`], [`cells`]).

pub mod cells;
mod decode;
pub mod edit;
pub mod grass;
mod grid;
mod material;
pub mod mesh;
mod random;

pub use decode::DecodeError;
pub use grid::{CHUNK_VOXELS, Grid, VOXEL_STUDS, Voxel, chunk_of};
pub use material::{Edges, Look, TerrainMaterial, Texture};
pub use mesh::{ChunkMesh, Facing, MESH_REACH, Surface, chunk_origin, chunks_to_mesh, mesh_chunk};

#[cfg(test)]
mod tests;
