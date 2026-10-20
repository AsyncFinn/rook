use super::*;
use crate::resolver::install::InstallKind;

fn resolver_with_install(name: &str) -> (Resolver, PathBuf) {
    let dir = crate::tests::temp_dir(name);
    std::fs::create_dir_all(dir.join("content/Textures")).unwrap();
    std::fs::write(dir.join("content/Textures/Face.png"), b"face").unwrap();
    let mut config = Config::empty();
    config.install = Install::at(&dir, InstallKind::Custom);
    config.cache_dir = Some(dir.join("cache"));
    (Resolver::new(config), dir)
}

#[test]
fn files_come_from_the_install_ignoring_case() {
    let (resolver, _dir) = resolver_with_install("resolve-install");
    let face = resolver
        .resolve(&ContentRef::parse("rbxasset://textures/face.png").unwrap())
        .unwrap();
    assert_eq!(&*face.bytes, b"face");
    assert_eq!(face.source.kind, SourceKind::Install);
    let missing = resolver
        .resolve(&ContentRef::parse("rbxasset://textures/none.png").unwrap())
        .unwrap_err();
    assert_eq!(missing.kind, FailureKind::NotFound);
    assert!(
        resolver
            .resolve(&ContentRef::File("../secret".into()))
            .is_err()
    );
}

#[test]
fn folders_come_first_when_ordered_first() {
    let (_, dir) = resolver_with_install("resolve-folders");
    std::fs::create_dir_all(dir.join("override/textures")).unwrap();
    std::fs::write(dir.join("override/textures/face.png"), b"mine").unwrap();
    let mut config = Config::empty();
    config.install = Install::at(&dir, InstallKind::Custom);
    config.folders = vec![dir.join("override")];
    let face = ContentRef::File("textures/face.png".into());
    let resolver = Resolver::new(config.clone());
    assert_eq!(&*resolver.resolve(&face).unwrap().bytes, b"face");
    config.order = vec![ProviderKind::Folders, ProviderKind::Install];
    let resolver = Resolver::new(config);
    let found = resolver.resolve(&face).unwrap();
    assert_eq!(&*found.bytes, b"mine");
    assert_eq!(found.source.kind, SourceKind::Folder);
}

#[test]
fn assets_come_from_the_cache_offline() {
    let (resolver, _dir) = resolver_with_install("resolve-cache");
    let asset = ContentRef::Asset {
        id: 5,
        version: None,
    };
    let offline = resolver.resolve(&asset).unwrap_err();
    assert_eq!(offline.kind, FailureKind::Offline);
    resolver
        .cache()
        .unwrap()
        .put("asset-6", b"cached", "https://cdn/x")
        .unwrap();
    let hit = resolver
        .resolve(&ContentRef::Asset {
            id: 6,
            version: None,
        })
        .unwrap();
    assert_eq!(&*hit.bytes, b"cached");
    assert_eq!(hit.source.kind, SourceKind::Cache);
    // A legacy fonts path stands for its asset.
    resolver
        .cache()
        .unwrap()
        .put("asset-12221462", b"head", "u")
        .unwrap();
    let head = resolver
        .resolve(&ContentRef::parse("rbxasset://fonts/head.mesh").unwrap())
        .unwrap();
    assert_eq!(&*head.bytes, b"head");
}
