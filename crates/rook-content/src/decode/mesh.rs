//! Reading Roblox mesh files, through the `rbx_mesh` crate (versions 1.00
//! to 5.00; 6.00 and 7.00 aren't read yet), into the one layout Rook draws.
//! `rbx_mesh` corrects version 1.00's doubled positions and flipped UVs;
//! Rook keeps only the highest level of detail (`docs/behavior/meshes/file-meshes.md`)
//! and rejects faces pointing past the vertex list, so a damaged file can't
//! reach the renderer. A union's geometry decodes into the same [`Mesh`]
//! ([`union`]), so unions load and draw through the same path.

mod file;
pub mod union;

use rook_types::NormalId;

/// A decoded mesh, in the file's own units and axes.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Mesh {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub uvs: Vec<[f32; 2]>,
    /// Three vertex indices per triangle.
    pub indices: Vec<u32>,
    /// Union geometry only (empty for mesh files): each vertex's color,
    /// sRGB with alpha, the color of the part it was cut from.
    pub colors: Vec<[u8; 4]>,
    /// Union geometry only: the box face each vertex was cut from, which
    /// images and material textures go by.
    pub faces: Vec<NormalId>,
    /// Terrain only: its material textures are projected onto planes in
    /// the world instead of laid over a box.
    pub terrain: Option<TerrainMapping>,
    /// Terrain seams only (empty elsewhere): each vertex's place in the
    /// blend of its triangle's layers (`rook_terrain::mesh::draws`).
    pub terrain_corners: Vec<TerrainCorner>,
}

/// A terrain seam vertex: the layers (`rook_terrain::Layer::index`) of
/// its triangle's three corners, which corner it is, and which corners'
/// layer the mesh draws (a bit per corner).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TerrainCorner {
    pub layers: [u8; 3],
    pub corner: u8,
    pub drawn: u8,
}

/// How terrain lays its material's textures (Studio's terrain material
/// table, `docs/behavior/terrain/materials.md`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerrainMapping {
    /// Onto whichever of 18 planes (the 6 axes' and the 12 at 45 degrees
    /// between two) faces the surface most.
    Planes,
    /// Onto the axes' planes only, like a box (Brick, WoodPlanks).
    Cube,
    /// Water's surface: no textures, waves and reflections instead.
    Water,
    /// Grass blades (`rook_terrain::grass`): each vertex's UV is its
    /// blade's sway phase and its height above the blade's foot.
    Blades,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum MeshError {
    #[error("not a Roblox mesh file")]
    NotAMesh,
    #[error("mesh version {0} is not supported yet")]
    Unsupported(String),
    /// A union's `MeshData` in a form Rook doesn't read (`CSGMDL3`).
    #[error("union geometry {0} is not supported yet")]
    UnsupportedUnion(String),
    #[error("mesh file is damaged: {0}")]
    Damaged(String),
}

impl MeshError {
    /// Whether the file is fine but Rook can't read its version yet.
    pub fn is_unsupported(&self) -> bool {
        matches!(self, Self::Unsupported(_) | Self::UnsupportedUnion(_))
    }
}

/// The versions `rbx_mesh` reads.
impl Mesh {
    /// Decode a mesh file, a union's asset or its `MeshData`.
    pub fn decode(bytes: &[u8]) -> Result<Self, MeshError> {
        let out = if union::is_union(bytes) {
            union::decode(bytes)?
        } else {
            file::decode(bytes)?
        };
        let count = out.positions.len() as u32;
        if let Some(bad) = out.indices.iter().find(|&&i| i >= count) {
            return Err(MeshError::Damaged(format!(
                "face uses vertex {bad} of {count}"
            )));
        }
        if out.indices.len() % 3 != 0 {
            return Err(MeshError::Damaged("faces aren't whole triangles".into()));
        }
        Ok(out)
    }

    /// The smallest box holding every vertex: (min, max).
    pub fn bounds(&self) -> ([f32; 3], [f32; 3]) {
        let mut lo = [f32::MAX; 3];
        let mut hi = [f32::MIN; 3];
        for p in &self.positions {
            for i in 0..3 {
                lo[i] = lo[i].min(p[i]);
                hi[i] = hi[i].max(p[i]);
            }
        }
        (lo, hi)
    }
}

#[cfg(test)]
#[expect(
    clippy::disallowed_methods,
    reason = "tests set up files on the machine running them"
)]
mod tests;
