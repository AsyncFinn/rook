use rook_types::NormalId;

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
        86, 46, 110, 88, 49, 32, 48, 4, 52, 105, 12, 119, 12, 1, 94, 0, 26, 96, 55, 105, 29, 82,
        43, 7, 79, 36, 89, 101, 83, 4, 122,
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
