//! Roblox mesh files (versions 1.00 to 5.00), read through `rbx_mesh`.

use rbx_mesh::mesh::{Face2, Mesh as Versioned, Vertices2};

use super::*;

const VERSIONS: [&str; 8] = [
    "1.00", "1.01", "2.00", "3.00", "3.01", "4.00", "4.01", "5.00",
];

pub(super) fn decode(bytes: &[u8]) -> Result<Mesh, MeshError> {
    let version = version(bytes).ok_or(MeshError::NotAMesh)?;
    if !VERSIONS.contains(&version) {
        return Err(MeshError::Unsupported(version.to_owned()));
    }
    let mesh = rbx_mesh::read_mesh_versioned(std::io::Cursor::new(bytes))
        .map_err(|e| MeshError::Damaged(e.to_string()))?;
    let out = match mesh {
        Versioned::V1(m) => {
            let mut out = Mesh::default();
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
