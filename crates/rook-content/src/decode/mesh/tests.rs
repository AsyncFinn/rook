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

/// A synthetic version 4.00 file: three vertices, faces `faces`, the LOD
/// table `lods` and `high_quality` high quality levels.
fn version_4(faces: &[[u32; 3]], lods: &[u32], high_quality: u8) -> Vec<u8> {
    let mut b = b"version 4.00
"
    .to_vec();
    b.extend_from_slice(&24u16.to_le_bytes());
    b.extend_from_slice(&3u16.to_le_bytes());
    b.extend_from_slice(&3u32.to_le_bytes());
    b.extend_from_slice(&(faces.len() as u32).to_le_bytes());
    b.extend_from_slice(&(lods.len() as u16).to_le_bytes());
    b.extend_from_slice(&0u16.to_le_bytes());
    b.extend_from_slice(&0u32.to_le_bytes());
    b.extend_from_slice(&0u16.to_le_bytes());
    b.extend_from_slice(&[high_quality, 0]);
    for i in 0..3 {
        for f in [i as f32, 0.0, 0.0, 0.0, 1.0, 0.0, 0.5, 0.5] {
            b.extend_from_slice(&f.to_le_bytes());
        }
        b.extend_from_slice(&[0, 0, 127, 127, 255, 255, 255, 255]);
    }
    for i in faces.iter().flatten().chain(lods) {
        b.extend_from_slice(&i.to_le_bytes());
    }
    b
}

/// Every level in the LOD table is kept, the full one first; a table
/// running backwards or past the faces stops where it goes wrong.
#[test]
fn keeps_every_level_of_detail() {
    let faces = [[0, 1, 2], [0, 2, 1], [1, 2, 0]];
    let mesh = Mesh::decode(&version_4(&faces, &[0, 2, 3], 1)).unwrap();
    assert_eq!(mesh.indices, [0, 1, 2, 0, 2, 1, 1, 2, 0]);
    assert_eq!(mesh.levels.starts, [6]);
    assert_eq!(mesh.levels.high_quality, 1);
    let levels = |m: &Mesh| m.levels().map(|r| (r.start, r.end)).collect::<Vec<_>>();
    assert_eq!(levels(&mesh), [(0, 6), (6, 9)]);
    let broken = Mesh::decode(&version_4(&faces, &[0, 2, 1, 3], 2)).unwrap();
    assert_eq!(levels(&broken), [(0, 6)]);
    let none = Mesh::decode(&version_4(&faces, &[], 0)).unwrap();
    assert_eq!(levels(&none), [(0, 9)]);
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
/// measured in docs/behavior/meshes/file-meshes.md (517 vertices, 846 triangles, about 1.2
/// studs each way).
#[test]
fn reads_the_installed_head_mesh() {
    let Some(install) = crate::resolver::install::newest_from(
        None,
        &crate::resolver::install::Places::from_env(|n| std::env::var_os(n)),
    ) else {
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
