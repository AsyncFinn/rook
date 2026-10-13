//! Reading Roblox mesh files. Only version 2.00, the version of the head
//! mesh, is read so far; M4 step 3 (file meshes) adds versions 1 and 3-5.
//!
//! Version 2.00, as documented by Maximum_ADHD's "Roblox Mesh Format"
//! (DevForum, 2019) and checked against the user's `head.mesh` (517
//! vertices, 846 triangles, ROADMAP M4): the line `version 2.00\n`, then a
//! little-endian header (u16 header size, u8 vertex size, u8 face size, u32
//! vertex count, u32 face count), the vertices (position, normal and UV as
//! f32s, then 4 tangent bytes and, in 40-byte vertices, 4 colour bytes),
//! then the faces (three u32 vertex indices each).

/// A decoded mesh, in the file's own units and axes.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Mesh {
    pub positions: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub uvs: Vec<[f32; 2]>,
    /// Three vertex indices per triangle.
    pub indices: Vec<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum MeshError {
    #[error("not a Roblox mesh file")]
    NotAMesh,
    #[error("mesh version {0} is not supported yet")]
    Unsupported(String),
    #[error("mesh file is cut short or damaged")]
    Truncated,
}

impl Mesh {
    /// Decode a mesh file.
    pub fn decode(bytes: &[u8]) -> Result<Self, MeshError> {
        let line_end = bytes
            .iter()
            .take(32)
            .position(|&b| b == b'\n')
            .ok_or(MeshError::NotAMesh)?;
        let line = std::str::from_utf8(&bytes[..line_end])
            .map_err(|_| MeshError::NotAMesh)?
            .trim_end_matches('\r');
        let version = line.strip_prefix("version ").ok_or(MeshError::NotAMesh)?;
        match version {
            "2.00" => decode_v2(&bytes[line_end + 1..]),
            other => Err(MeshError::Unsupported(other.to_owned())),
        }
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

struct Reader<'a>(&'a [u8]);

impl Reader<'_> {
    fn take<const N: usize>(&mut self) -> Result<[u8; N], MeshError> {
        let (head, rest) = self.0.split_first_chunk().ok_or(MeshError::Truncated)?;
        self.0 = rest;
        Ok(*head)
    }

    fn skip(&mut self, n: usize) -> Result<(), MeshError> {
        self.0 = self.0.get(n..).ok_or(MeshError::Truncated)?;
        Ok(())
    }

    fn u32(&mut self) -> Result<u32, MeshError> {
        self.take().map(u32::from_le_bytes)
    }

    fn f32(&mut self) -> Result<f32, MeshError> {
        self.take().map(f32::from_le_bytes)
    }

    fn vec3(&mut self) -> Result<[f32; 3], MeshError> {
        Ok([self.f32()?, self.f32()?, self.f32()?])
    }
}

fn decode_v2(body: &[u8]) -> Result<Mesh, MeshError> {
    let mut r = Reader(body);
    let header_size = usize::from(u16::from_le_bytes(r.take()?));
    let [vertex_size] = r.take()?;
    let [face_size] = r.take()?;
    let vertices = r.u32()? as usize;
    let faces = r.u32()? as usize;
    // The header counts its own 12 bytes; vertices hold at least position,
    // normal and UV (32 bytes), faces three indices.
    if header_size < 12 || vertex_size < 32 || face_size < 12 {
        return Err(MeshError::NotAMesh);
    }
    r.skip(header_size - 12)?;
    let needed = vertices * usize::from(vertex_size) + faces * usize::from(face_size);
    if r.0.len() < needed {
        return Err(MeshError::Truncated);
    }
    let mut mesh = Mesh {
        positions: Vec::with_capacity(vertices),
        normals: Vec::with_capacity(vertices),
        uvs: Vec::with_capacity(vertices),
        indices: Vec::with_capacity(faces * 3),
    };
    for _ in 0..vertices {
        mesh.positions.push(r.vec3()?);
        mesh.normals.push(r.vec3()?);
        mesh.uvs.push([r.f32()?, r.f32()?]);
        r.skip(usize::from(vertex_size) - 32)?;
    }
    for _ in 0..faces {
        for _ in 0..3 {
            let index = r.u32()?;
            if index as usize >= vertices {
                return Err(MeshError::Truncated);
            }
            mesh.indices.push(index);
        }
        r.skip(usize::from(face_size) - 12)?;
    }
    Ok(mesh)
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

    #[test]
    fn rejects_damaged_files() {
        let full = triangle();
        assert_eq!(
            Mesh::decode(&full[..full.len() - 1]),
            Err(MeshError::Truncated)
        );
        assert_eq!(Mesh::decode(b"PNG"), Err(MeshError::NotAMesh));
        assert_eq!(
            Mesh::decode(b"version 4.00\n...."),
            Err(MeshError::Unsupported("4.00".into()))
        );
        let mut bad_index = full;
        let n = bad_index.len();
        bad_index[n - 4..].copy_from_slice(&9u32.to_le_bytes());
        assert_eq!(Mesh::decode(&bad_index), Err(MeshError::Truncated));
    }

    /// The user's own head mesh, when an install is found: the facts
    /// measured in ROADMAP M4 (517 vertices, 846 triangles, about 1.2
    /// studs each way).
    #[test]
    fn reads_the_installed_head_mesh() {
        let Some(install) = crate::install::newest() else {
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
