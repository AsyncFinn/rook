//! Smooth terrain: `Terrain.SmoothGrid`'s voxels ([`Grid`], decoded in
//! [`decode`]) and the surface Roblox builds from them ([`mesh`]), shared by
//! the renderer and, later, physics and the Terrain API.

mod decode;
mod grid;
mod material;
pub mod mesh;

pub use decode::DecodeError;
pub use grid::{CHUNK_VOXELS, Grid, VOXEL_STUDS, Voxel, chunk_of};
pub use material::TerrainMaterial;
pub use mesh::{ChunkMesh, Surface, chunk_origin, chunks_to_mesh, mesh_chunk};

#[cfg(test)]
mod tests;
