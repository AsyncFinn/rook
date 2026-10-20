use super::*;
use crate::resolver::install::{Install, InstallKind};
use crate::{Config, FailureKind};

#[test]
fn resolves_off_thread_and_shares_duplicates() {
    let dir = crate::tests::temp_dir("loader");
    std::fs::create_dir_all(dir.join("content/textures")).unwrap();
    std::fs::write(dir.join("content/textures/face.png"), b"face").unwrap();
    let mut config = Config::empty();
    config.install = Install::at(&dir, InstallKind::Custom);
    let mut loader = Loader::new(Arc::new(Resolver::new(config)));
    assert!(loader.poll().is_empty());

    let face = ContentRef::File("textures/face.png".into());
    let a = loader.request(face.clone());
    let b = loader.request(face);
    let c = loader.request(ContentRef::Asset {
        id: 5,
        version: None,
    });
    assert_eq!(loader.queue_size(), 3);
    let results = loader.wait_all();
    assert_eq!(loader.queue_size(), 0);
    assert_eq!(results.len(), 3);
    assert_eq!(results[0].0, a);
    assert_eq!(results[1].0, b);
    assert_eq!(&*results[1].1.as_ref().unwrap().bytes, b"face");
    assert_eq!(results[2].0, c);
    assert_eq!(
        results[2].1.as_ref().unwrap_err().kind,
        FailureKind::Offline
    );
}

/// Asked for a mesh, the worker decodes it; bytes that aren't one fail
/// as invalid, and the same file asked for as bytes still resolves.
#[test]
fn decodes_meshes_on_the_workers() {
    let dir = crate::tests::temp_dir("loader-mesh");
    std::fs::create_dir_all(dir.join("content/meshes")).unwrap();
    let text =
        "version 1.00\n1\n[2,0,0][0,1,0][0,0,0][0,2,0][0,1,0][0,0,0][0,0,2][0,1,0][0,0,0]\n";
    std::fs::write(dir.join("content/meshes/tri.mesh"), text).unwrap();
    std::fs::write(dir.join("content/meshes/face.png"), b"\x89PNG").unwrap();
    let mut config = Config::empty();
    config.install = Install::at(&dir, InstallKind::Custom);
    let mut loader = Loader::new(Arc::new(Resolver::new(config)));
    let tri = ContentRef::File("meshes/tri.mesh".into());
    loader.request_as(tri.clone(), Want::Mesh);
    loader.request_as(ContentRef::File("meshes/face.png".into()), Want::Mesh);
    loader.request(tri);
    let results = loader.wait_all();
    let mesh = results[0].1.as_ref().unwrap().mesh.as_ref().unwrap();
    assert_eq!(mesh.indices, [0, 1, 2]);
    assert_eq!(
        results[1].1.as_ref().unwrap_err().kind,
        FailureKind::Invalid
    );
    assert!(results[2].1.as_ref().unwrap().mesh.is_none());
}

/// Asked for a sound, the worker decodes it and frees the file, so the
/// engine thread never pays for freeing it.
#[test]
fn decodes_sounds_and_frees_the_file() {
    let dir = crate::tests::temp_dir("loader-audio");
    std::fs::create_dir_all(dir.join("content/sounds")).unwrap();
    let wav = crate::decode::audio::tests::wav(22050, 1, 2205);
    std::fs::write(dir.join("content/sounds/tone.wav"), &wav).unwrap();
    let mut config = Config::empty();
    config.install = Install::at(&dir, InstallKind::Custom);
    let mut loader = Loader::new(Arc::new(Resolver::new(config)));
    loader.request_as(ContentRef::File("sounds/tone.wav".into()), Want::Audio);
    let results = loader.wait_all();
    let fetched = results[0].1.as_ref().unwrap();
    assert!(fetched.audio.is_some());
    assert!(fetched.bytes.is_empty());
    assert_eq!(fetched.hash, blake3::hash(&wav));
}
