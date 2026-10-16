//! Reading Roblox mesh files, through the `rbx_mesh` crate (versions 1.00
//! to 5.00; 6.00 and 7.00 aren't read yet), into the one layout Rook draws.
//!
//! `rbx_mesh` already corrects version 1.00's positions (stored at twice
//! their size) and the upside-down V of version 1 UVs. What Rook adds:
//!
//! - From version 3.00 on, a file holds every level of detail in one face
//!   list, and its LOD table gives where each starts. Only the first
//!   (highest-detail) range is kept (Inference from the community format
//!   specification, Maximum_ADHD's "Roblox Mesh Format", DevForum).
//! - Faces pointing past the vertex list are rejected, so a damaged file
//!   can't reach the renderer.
//!
//! Checked against the user's `head.mesh` (version 2.00, 517 vertices, 846
//! triangles, ROADMAP M4) when an install is found.
//!
//! A union's geometry decodes into the same [`Mesh`] ([`union`]), so
//! unions load, upload and draw through the same path as mesh files.

pub mod union;

use rbx_mesh::mesh::{Face2, Mesh as Versioned, Vertices2};
use rook_types::NormalId;

/// A decoded mesh, in the file's own units and axes.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Mesh {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub uvs: Vec<[f32; 2]>,
    /// Three vertex indices per triangle.
    pub indices: Vec<u32>,
    /// Union geometry only (empty for mesh files): each vertex's colour,
    /// sRGB with alpha, the colour of the part it was cut from.
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
/// table, `docs/behavior/terrain.md`).
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
    #[error("mesh file is damaged: {0}")]
    Damaged(String),
}

/// The versions `rbx_mesh` reads.
const VERSIONS: [&str; 8] = [
    "1.00", "1.01", "2.00", "3.00", "3.01", "4.00", "4.01", "5.00",
];

impl Mesh {
    /// Decode a mesh file, a union's asset or its `MeshData`.
    pub fn decode(bytes: &[u8]) -> Result<Self, MeshError> {
        let out = if union::is_union(bytes) {
            union::decode(bytes)?
        } else {
            Self::decode_file(bytes)?
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

    fn decode_file(bytes: &[u8]) -> Result<Self, MeshError> {
        let version = version(bytes).ok_or(MeshError::NotAMesh)?;
        if !VERSIONS.contains(&version) {
            return Err(MeshError::Unsupported(version.to_owned()));
        }
        let mesh = rbx_mesh::read_mesh_versioned(std::io::Cursor::new(bytes))
            .map_err(|e| MeshError::Damaged(e.to_string()))?;
        let out = match mesh {
            Versioned::V1(m) => {
                let mut out = Self::default();
                for v in &m.vertices {
                    out.positions.push(v.pos);
                    out.normals.push(v.norm);
                    out.uvs.push([v.tex[0], v.tex[1]]);
                }
                // Version 1 lists each triangle's three corners in turn.
                out.indices = (0..out.positions.len() as u32).collect();
                out
            }
            Versioned::V2(m) => from_parts(&m.vertices, &m.faces, &[]),
            Versioned::V3(m) => from_parts(&m.vertices, &m.faces, &lods(&m.lods)),
            Versioned::V4(m) => from_parts(&Vertices2::Full(m.vertices), &m.faces, &lods(&m.lods)),
            Versioned::V5(m) => from_parts(&Vertices2::Full(m.vertices), &m.faces, &lods(&m.lods)),
        };
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

/// The version in a mesh file's first line (`version 2.00`).
fn version(bytes: &[u8]) -> Option<&str> {
    let line = bytes.split(|&b| b == b'\n').next()?;
    let line = std::str::from_utf8(line.get(..line.len().min(32))?).ok()?;
    line.trim_end_matches('\r').strip_prefix("version ")
}

fn lods(lods: &[rbx_mesh::mesh::Lod3]) -> Vec<u32> {
    lods.iter().map(|l| l.0).collect()
}

/// Vertices and the faces of the first level of detail (`lods` gives each
/// level's first face; none means every face).
fn from_parts(vertices: &Vertices2, faces: &[Face2], lods: &[u32]) -> Mesh {
    let mut out = Mesh::default();
    let mut push = |pos: [f32; 3], norm: [f32; 3], tex: [f32; 2]| {
        out.positions.push(pos);
        out.normals.push(norm);
        out.uvs.push(tex);
    };
    match vertices {
        Vertices2::Full(all) => all.iter().for_each(|v| push(v.pos, v.norm, v.tex)),
        Vertices2::Truncated(all) => all.iter().for_each(|v| push(v.pos, v.norm, v.tex)),
    }
    let first = match lods {
        [start, end, ..] if start <= end => {
            (*start as usize).min(faces.len())..(*end as usize).min(faces.len())
        }
        _ => 0..faces.len(),
    };
    out.indices = faces[first]
        .iter()
        .flat_map(|f| f.0.iter().map(|v| v.0))
        .collect();
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A synthetic version 2.00 file: one triangle with 40-byte vertices.
    fn triangle() -> Vec<u8> {
        let mut b = b"version 2.00\n".to_vec();
        b.extend_from_slice(&12u16.to_le_bytes());
        b.extend_from_slice(&[40, 12]);
        b.extend_from_slice(&3u32.to_le_bytes());
        b.extend_from_slice(&1u32.to_le_bytes());
        for i in 0..3 {
            for f in [i as f32, 0.0, 0.0, 0.0, 1.0, 0.0, 0.5, 0.5] {
                b.extend_from_slice(&f.to_le_bytes());
            }
            b.extend_from_slice(&[0, 0, 127, 127, 255, 255, 255, 255]);
        }
        for i in [0u32, 1, 2] {
            b.extend_from_slice(&i.to_le_bytes());
        }
        b
    }

    #[test]
    fn reads_version_2() {
        let mesh = Mesh::decode(&triangle()).unwrap();
        assert_eq!(mesh.positions, [[0.0; 3], [1.0, 0.0, 0.0], [2.0, 0.0, 0.0]]);
        assert_eq!(mesh.normals[1], [0.0, 1.0, 0.0]);
        assert_eq!(mesh.uvs[2], [0.5, 0.5]);
        assert_eq!(mesh.indices, [0, 1, 2]);
        assert_eq!(mesh.bounds(), ([0.0; 3], [2.0, 0.0, 0.0]));
    }

    /// Version 1.00 is text at twice the size, with V upside down.
    #[test]
    fn reads_version_1() {
        let text =
            "version 1.00\n1\n[2,0,0][0,1,0][0,0.25,0][0,2,0][0,1,0][1,1,0][0,0,2][0,1,0][0,0,0]\n";
        let mesh = Mesh::decode(text.as_bytes()).unwrap();
        assert_eq!(
            mesh.positions,
            [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]
        );
        assert_eq!(mesh.uvs, [[0.0, 0.75], [1.0, 0.0], [0.0, 1.0]]);
        assert_eq!(mesh.indices, [0, 1, 2]);
    }

    #[test]
    fn rejects_damaged_files() {
        let full = triangle();
        assert!(matches!(
            Mesh::decode(&full[..full.len() - 1]),
            Err(MeshError::Damaged(_))
        ));
        assert_eq!(Mesh::decode(b"\x89PNG"), Err(MeshError::NotAMesh));
        assert_eq!(
            Mesh::decode(b"version 7.00\n...."),
            Err(MeshError::Unsupported("7.00".into()))
        );
        let mut bad_index = full;
        let n = bad_index.len();
        bad_index[n - 4..].copy_from_slice(&9u32.to_le_bytes());
        assert!(matches!(
            Mesh::decode(&bad_index),
            Err(MeshError::Damaged(_))
        ));
    }

    /// The user's own head mesh, when an install is found: the facts
    /// measured in ROADMAP M4 (517 vertices, 846 triangles, about 1.2
    /// studs each way).
    #[test]
    fn reads_the_installed_head_mesh() {
        let Some(install) = crate::resolver::install::newest() else {
            eprintln!("skipped: no Roblox install found");
            return;
        };
        let path = install.content[0].join("avatar/heads/head.mesh");
        let Ok(bytes) = std::fs::read(&path) else {
            eprintln!("skipped: {} not found", path.display());
            return;
        };
        let mesh = Mesh::decode(&bytes).unwrap();
        assert_eq!(mesh.positions.len(), 517);
        assert_eq!(mesh.indices.len(), 846 * 3);
        let (lo, hi) = mesh.bounds();
        assert!((hi[1] - lo[1] - 1.2024).abs() < 1e-3, "{lo:?} {hi:?}");
    }
}
