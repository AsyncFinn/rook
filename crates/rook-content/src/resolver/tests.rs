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

/// A resolver with these files in its overrides folder, and no network.
fn resolver_with_overrides(name: &str, files: &[&str]) -> Resolver {
    let dir = crate::tests::temp_dir(name).join("overrides");
    std::fs::create_dir_all(&dir).unwrap();
    for file in files {
        std::fs::write(dir.join(file), file.as_bytes()).unwrap();
    }
    let mut config = Config::empty();
    config.overrides_dir = Some(dir);
    Resolver::new(config)
}

#[test]
fn an_override_answers_its_asset_id_at_any_version() {
    let resolver = resolver_with_overrides("override-answers", &["5.ogg"]);
    for reference in [
        "rbxassetid://5",
        "rbxassetid://5&version=2",
        "http://www.roblox.com/asset/?id=5",
    ] {
        let found = resolver
            .resolve(&ContentRef::parse(reference).unwrap())
            .unwrap();
        assert_eq!(&*found.bytes, b"5.ogg", "{reference}");
        assert_eq!(found.source.kind, SourceKind::Override);
    }
}

#[test]
fn an_override_beats_a_refusal_the_cache_remembers() {
    let dir = crate::tests::temp_dir("override-refused");
    let key = ContentRef::Asset {
        id: 5,
        version: None,
    };
    let cache = Cache::open(dir.join("cache")).unwrap();
    cache
        .put_failure(
            &key.cache_key().unwrap(),
            &Failure::new(FailureKind::Forbidden, "private"),
        )
        .unwrap();
    std::fs::create_dir_all(dir.join("overrides")).unwrap();
    std::fs::write(dir.join("overrides/5.ogg"), b"mine").unwrap();
    let mut config = Config::empty();
    config.cache_dir = Some(dir.join("cache"));
    config.overrides_dir = Some(dir.join("overrides"));
    assert_eq!(
        &*Resolver::new(config).resolve(&key).unwrap().bytes,
        b"mine"
    );
}

#[test]
fn other_ids_and_names_are_not_overrides() {
    let resolver = resolver_with_overrides("override-others", &["5.ogg", "notes.txt", "5x.ogg"]);
    let other = ContentRef::Asset {
        id: 6,
        version: None,
    };
    assert_eq!(
        resolver.resolve(&other).unwrap_err().kind,
        FailureKind::Offline
    );
    assert_eq!(resolver.overrides.len(), 1);
}

#[test]
fn two_files_for_one_id_give_the_first_by_name() {
    let resolver = resolver_with_overrides("override-twice", &["5.wav", "5.mp3"]);
    let found = resolver
        .resolve(&ContentRef::Asset {
            id: 5,
            version: None,
        })
        .unwrap();
    assert_eq!(&*found.bytes, b"5.mp3");
}

#[test]
fn overrides_left_out_of_the_order_are_not_read() {
    let dir = crate::tests::temp_dir("override-off").join("overrides");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("5.ogg"), b"mine").unwrap();
    let mut config = Config::empty();
    config.order = vec![ProviderKind::Delivery];
    config.overrides_dir = Some(dir);
    assert!(Resolver::new(config).overrides.is_empty());
}

#[test]
fn the_overrides_folder_is_made_for_the_user() {
    let dir = crate::tests::temp_dir("override-made").join("overrides");
    let mut config = Config::empty();
    config.overrides_dir = Some(dir.clone());
    Resolver::new(config);
    assert!(dir.is_dir());
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
