pub mod grass;
pub mod grid;
pub mod lod;
mod material;
pub mod mesh;
mod random;

pub use grid::{CHUNK_VOXELS, Grid, VOXEL_STUDS, Voxel, chunk_of};
pub use material::{Coloring, Recolor, TerrainMaterial, all_packs};
pub use mesh::{
    ChunkMesh, Detail, Facing, Layer, MESH_REACH, Seam, chunks_to_mesh, mesh_chunk, mesh_chunk_at,
};

#[cfg(test)]
#[expect(
    clippy::disallowed_methods,
    reason = "tests set up files on the machine running them"
)]
mod tests;
