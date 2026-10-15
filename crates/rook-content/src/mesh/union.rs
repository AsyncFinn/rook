//! A union's geometry (`UnionOperation`, `NegateOperation`,
//! `IntersectOperation`), read through `rbx_mesh`'s `union_graphics`.
//!
//! A union keeps its geometry in a `MeshData` blob: in the union itself in
//! old files, or, from about 2015 on, in a model asset its `AssetId` names,
//! whose one `PartOperationAsset` holds `MeshData` (and `PhysicsData` and
//! `ChildData`, which drawing doesn't need). The blob is one of:
//!
//! - `CSGK` and a 32-digit hex id: no geometry, only a pointer to the
//!   asset;
//! - `CSGMDL` versions 2 and 4 (XOR-obfuscated, full vertices) and 5
//!   (packed, delta-coded indices), which `rbx_mesh` reads.
//!
//! Each vertex also carries its colour (the colour of the part it came
//! from) and the `NormalId` of the box face it was cut from; both are kept.
//! Version 5 holds more than one index range; only the first is drawn (the
//! others are Inference: further levels of detail).

pub mod physics;

use rbx_mesh::union_graphics::{Mesh2, NormalId as UnionNormal, UnionGraphics};
use rbx_types::Variant;
use rook_types::NormalId;

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
        other => return Err(MeshError::Unsupported(other.name())),
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
                colors: m.colors,
                faces: m.normal_ids.iter().map(|f| normal_id(f.0)).collect(),
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
mod tests {
    use super::*;

    #[test]
    fn kinds() {
        assert_eq!(MeshDataKind::of(b""), MeshDataKind::Empty);
        assert_eq!(
            MeshDataKind::of(b"CSGKf8a5e7d8f564986f0ae7e4bc02ef2ac4"),
            MeshDataKind::Csgk
        );
        for v in [2u8, 4, 5] {
            let mut b = b"\x15\x7d\x29\x15\x75\x6c".to_vec();
            b.push(b'0' ^ v);
            assert_eq!(MeshDataKind::of(&b), MeshDataKind::Csgmdl(v));
        }
        assert_eq!(MeshDataKind::of(b"version 2.00"), MeshDataKind::Unknown);
    }

    /// A synthetic `CSGMDL2` blob: one triangle, XOR-obfuscated after its
    /// magic by absolute position, as Roblox writes it.
    fn csgmdl2() -> Vec<u8> {
        const KEY: [u8; 31] = [
            86, 46, 110, 88, 49, 32, 48, 4, 52, 105, 12, 119, 12, 1, 94, 0, 26, 96, 55, 105, 29,
            82, 43, 7, 79, 36, 89, 101, 83, 4, 122,
        ];
        let mut b = b"CSGMDL".to_vec();
        b.extend_from_slice(&2u32.to_le_bytes());
        b.extend_from_slice(&[0; 32]);
        b.extend_from_slice(&3u32.to_le_bytes());
        b.extend_from_slice(&84u32.to_le_bytes());
        for (i, face) in [(0u32, 6u32), (1, 2), (2, 1)] {
            for f in [i as f32, 0.5, -0.5, 0.0, 0.0, -1.0] {
                b.extend_from_slice(&f.to_le_bytes());
            }
            b.extend_from_slice(&[200, 100, 50, 255]);
            b.extend_from_slice(&face.to_le_bytes());
            for f in [0.25f32, -0.75] {
                b.extend_from_slice(&f.to_le_bytes());
            }
            b.extend_from_slice(&[0; 16]);
            b.extend_from_slice(&[0; 12]);
            b.extend_from_slice(&[0; 16]);
        }
        b.extend_from_slice(&3u32.to_le_bytes());
        for i in [0u32, 1, 2] {
            b.extend_from_slice(&i.to_le_bytes());
        }
        for (i, byte) in b.iter_mut().enumerate() {
            *byte ^= KEY[i % KEY.len()];
        }
        b
    }

    #[test]
    fn reads_csgmdl2() {
        let bytes = csgmdl2();
        assert_eq!(MeshDataKind::of(&bytes), MeshDataKind::Csgmdl(2));
        let mesh = Mesh::decode(&bytes).unwrap();
        assert_eq!(mesh.positions[1], [1.0, 0.5, -0.5]);
        assert_eq!(mesh.normals[0], [0.0, 0.0, -1.0]);
        assert_eq!(mesh.uvs[2], [0.25, -0.75]);
        assert_eq!(mesh.colors[0], [200, 100, 50, 255]);
        assert_eq!(
            mesh.faces,
            [NormalId::Front, NormalId::Top, NormalId::Right]
        );
        assert_eq!(mesh.indices, [0, 1, 2]);
    }

    #[test]
    fn rejects_unions_without_geometry() {
        assert!(matches!(
            decode(b"CSGKf8a5e7d8f564986f0ae7e4bc02ef2ac4"),
            Err(MeshError::Damaged(_))
        ));
    }
}
