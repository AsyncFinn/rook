//! A union's geometry (`UnionOperation`, `NegateOperation`,
//! `IntersectOperation`), read from its `MeshData` blob through `rbx_mesh`'s
//! `union_graphics`, with each vertex's color and the `NormalId` of the
//! box face it was cut from. Where the blob is and its forms are in
//! `docs/behavior/meshes/unions.md`.

pub mod physics;

use rbx_mesh::union_graphics::{Mesh2, NormalId as UnionNormal, UnionGraphics};
use rbx_types::Variant;
use rook_types::NormalId;

use super::{Levels, Mesh, MeshError};

/// What a `MeshData` blob holds, by its first bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum MeshDataKind {
    Empty,
    /// A pointer to the union's asset, without geometry.
    Csgk,
    Csgmdl(u8),
    Unknown,
}

impl MeshDataKind {
    pub fn of(bytes: &[u8]) -> Self {
        // `CSGMDL` XORed with `rbx_mesh`'s obfuscation key, then the version.
        const CSGMDL: &[u8] = b"\x15\x7d\x29\x15\x75\x6c";
        if bytes.is_empty() {
            Self::Empty
        } else if bytes.starts_with(b"CSGK") {
            Self::Csgk
        } else if bytes.starts_with(CSGMDL) && bytes.len() > 6 {
            // The version's first byte, XORed with the key's seventh (48).
            Self::Csgmdl(bytes[6] ^ 0x30)
        } else {
            Self::Unknown
        }
    }

    pub fn name(self) -> String {
        match self {
            Self::Empty => "empty".into(),
            Self::Csgk => "CSGK".into(),
            Self::Csgmdl(v) => format!("CSGMDL{v}"),
            Self::Unknown => "unknown".into(),
        }
    }
}

/// Whether `bytes` start like a union's geometry or its asset (a model
/// file), rather than a mesh file.
pub(super) fn is_union(bytes: &[u8]) -> bool {
    bytes.starts_with(b"<roblox") || MeshDataKind::of(bytes) != MeshDataKind::Unknown
}

/// The `MeshData` held by a union asset (a model whose instance is a
/// `PartOperationAsset`), or by any union in a model.
pub fn asset_mesh_data(bytes: &[u8]) -> Result<Vec<u8>, MeshError> {
    let loaded = rook_formats::load(bytes).map_err(|e| MeshError::Damaged(e.to_string()))?;
    loaded
        .dom
        .descendants()
        .find_map(|inst| mesh_data(&inst.properties))
        .ok_or(MeshError::Damaged(
            "the union asset holds no MeshData".into(),
        ))
}

/// The non-empty `MeshData` among an instance's properties (newer files
/// may hold it in `MeshData2`, a shared string, instead).
fn mesh_data(properties: &rbx_dom_weak::UstrMap<Variant>) -> Option<Vec<u8>> {
    ["MeshData", "MeshData2"].iter().find_map(|name| {
        let bytes: &[u8] = match properties.get(&(*name).into())? {
            Variant::BinaryString(b) => b.as_ref(),
            Variant::SharedString(s) => s.data(),
            _ => return None,
        };
        (!bytes.is_empty()).then(|| bytes.to_vec())
    })
}

/// Decode a union asset or a `MeshData` blob.
pub(super) fn decode(bytes: &[u8]) -> Result<Mesh, MeshError> {
    if bytes.starts_with(b"<roblox") {
        return decode(&asset_mesh_data(bytes)?);
    }
    match MeshDataKind::of(bytes) {
        MeshDataKind::Csgmdl(2 | 4 | 5) => {}
        MeshDataKind::Csgk | MeshDataKind::Empty => {
            return Err(MeshError::Damaged(
                "the union's MeshData has no geometry".into(),
            ));
        }
        other => return Err(MeshError::UnsupportedUnion(other.name())),
    }
    let graphics = rbx_mesh::read_union_graphics_versioned(std::io::Cursor::new(bytes))
        .map_err(|e| MeshError::Damaged(e.to_string()))?;
    Ok(match graphics {
        UnionGraphics::CSGK(_) => {
            return Err(MeshError::Damaged(
                "the union's MeshData has no geometry".into(),
            ));
        }
        UnionGraphics::V2(m) => from_mesh2(&m.mesh),
        UnionGraphics::V4(m) => from_mesh2(&m.mesh),
        UnionGraphics::V5(m) => {
            let n = m.positions.len();
            if [
                m.normals.len(),
                m.colors.len(),
                m.normal_ids.len(),
                m.tex.len(),
            ]
            .iter()
            .any(|&len| len != n)
            {
                return Err(MeshError::Damaged("vertex lists differ in length".into()));
            }
            Mesh {
                positions: m.positions,
                normals: m.normals.iter().map(|q| q.0).collect(),
                uvs: m.tex,
                indices: m.faces.indices,
                levels: Levels::default(),
                colors: m.colors,
                faces: m.normal_ids.iter().map(|f| normal_id(f.0)).collect(),
                terrain: None,
                terrain_corners: Vec::new(),
            }
        }
    })
}

fn from_mesh2(m: &Mesh2) -> Mesh {
    let mut out = Mesh::default();
    for v in &m.vertices {
        out.positions.push(v.pos);
        out.normals.push(v.norm);
        out.uvs.push(v.tex);
        out.colors.push(v.color);
        out.faces.push(normal_id(v.normal_id.0));
    }
    out.indices = m.faces.iter().flat_map(|f| f.iter().map(|v| v.0)).collect();
    out
}

/// `rbx_mesh` numbers faces from 1 in Roblox's order.
fn normal_id(n: UnionNormal) -> NormalId {
    match n {
        UnionNormal::Right => NormalId::Right,
        UnionNormal::Top => NormalId::Top,
        UnionNormal::Back => NormalId::Back,
        UnionNormal::Left => NormalId::Left,
        UnionNormal::Bottom => NormalId::Bottom,
        UnionNormal::Front => NormalId::Front,
    }
}

#[cfg(test)]
mod tests;
