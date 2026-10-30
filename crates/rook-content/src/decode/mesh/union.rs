//! A union's geometry (`UnionOperation`, `NegateOperation`,
//! `IntersectOperation`), read from its `MeshData` blob ([`graphics`]),
//! with each vertex's color and the `NormalId` of the box face it was cut
//! from. Where the blob is and its forms are in
//! `docs/behavior/meshes/unions.md`.

mod edgebreaker;
mod graphics;
pub mod physics;

use rbx_types::Variant;

use super::{Mesh, MeshError};

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
        if bytes.is_empty() {
            Self::Empty
        } else if bytes.starts_with(b"CSGK") {
            Self::Csgk
        } else if bytes.starts_with(&graphics::MAGIC) && bytes.len() > 6 {
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
        MeshDataKind::Csgmdl(2 | 4) => graphics::csgmdl2(bytes),
        MeshDataKind::Csgmdl(5) => graphics::csgmdl5(bytes),
        MeshDataKind::Csgk | MeshDataKind::Empty => Err(MeshError::Damaged(
            "the union's MeshData has no geometry".into(),
        )),
        other => Err(MeshError::UnsupportedUnion(other.name())),
    }
}

#[cfg(test)]
mod tests;
