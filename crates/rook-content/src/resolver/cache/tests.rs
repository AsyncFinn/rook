use super::*;

#[test]
fn stores_and_verifies() {
    let cache = Cache::open(crate::tests::temp_dir("cache")).unwrap();
    assert!(matches!(cache.get("asset-1"), Cached::Miss));
    let path = cache
        .put("asset-1", b"mesh bytes", "https://cdn/x")
        .unwrap();
    let Cached::Hit { bytes, url, .. } = cache.get("asset-1") else {
        panic!("not cached");
    };
    assert_eq!(&*bytes, b"mesh bytes");
    assert_eq!(url, "https://cdn/x");
    // Same bytes under another reference share the blob.
    assert_eq!(cache.put("asset-2", b"mesh bytes", "u").unwrap(), path);

    // A damaged blob is dropped and refetched.
    std::fs::write(&path, b"mesh bytez").unwrap();
    assert!(matches!(cache.get("asset-1"), Cached::Miss));
    assert!(!path.exists());
    // A partial one too.
    cache.put("asset-1", b"mesh bytes", "u").unwrap();
    std::fs::write(&path, b"mesh").unwrap();
    assert!(matches!(cache.get("asset-1"), Cached::Miss));
}

/// Reading marks a blob used; trimming drops the least recently used
/// first, with the references left behind.
#[test]
fn trims_least_recently_used() {
    let cache = Cache::open(crate::tests::temp_dir("cache-trim")).unwrap();
    let old = SystemTime::now() - Duration::from_secs(10 * 24 * 60 * 60);
    for (key, bytes) in [
        ("asset-1", [1u8; 100]),
        ("asset-2", [2; 100]),
        ("asset-3", [3; 100]),
    ] {
        let path = cache.put(key, &bytes, "u").unwrap();
        std::fs::File::options()
            .write(true)
            .open(&path)
            .unwrap()
            .set_modified(old)
            .unwrap();
    }
    // asset-1 is read, so it's the most recently used.
    assert!(matches!(cache.get("asset-1"), Cached::Hit { .. }));
    assert_eq!(cache.stats(), (3, 300, 3));
    assert_eq!(cache.trim(150).1, 200);
    assert!(matches!(cache.get("asset-1"), Cached::Hit { .. }));
    assert!(matches!(cache.get("asset-2"), Cached::Miss));
    assert_eq!(cache.stats(), (1, 100, 1));
    // Not used for a day: pruned by age.
    assert_eq!(cache.prune_older_than(Duration::from_secs(60)), (0, 0));
    cache.clear().unwrap();
    assert_eq!(cache.stats(), (0, 0, 0));
}

/// A legacy cache moves whole, or merges into an existing one.
#[test]
fn moves_a_legacy_cache_once() {
    let root = crate::tests::temp_dir("cache-move");
    let old = Cache::open(root.join("old/Rook/cache")).unwrap();
    old.put("asset-1", b"one", "u").unwrap();
    let to = root.join("new/Rook/cache");
    assert_eq!(Cache::move_legacy(&old.dir, &to).unwrap(), 2);
    assert!(!root.join("old/Rook").exists());
    let moved = Cache::open(&to).unwrap();
    assert!(matches!(moved.get("asset-1"), Cached::Hit { .. }));
    // Again into the existing cache: merged, nothing overwritten.
    let again = Cache::open(root.join("old/Rook/cache")).unwrap();
    again.put("asset-2", b"two", "u").unwrap();
    again.put("asset-1", b"other", "u").unwrap();
    Cache::move_legacy(&again.dir, &to).unwrap();
    assert!(matches!(moved.get("asset-2"), Cached::Hit { .. }));
    let Cached::Hit { bytes, .. } = moved.get("asset-1") else {
        panic!("lost");
    };
    assert_eq!(&*bytes, b"one");
    assert_eq!(Cache::move_legacy(&root.join("nothing"), &to).unwrap(), 0);
}

#[test]
fn sizes() {
    assert_eq!(parse_size("2GB"), Some(2 << 30));
    assert_eq!(parse_size("500 MB"), Some(500 << 20));
    assert_eq!(parse_size("750k"), Some(750 << 10));
    assert_eq!(parse_size("1.5g"), Some(3 << 29));
    assert_eq!(parse_size("300"), Some(300 << 20));
    assert_eq!(parse_size("lots"), None);
}

#[test]
fn remembers_lasting_failures_for_a_while() {
    let cache = Cache::open(crate::tests::temp_dir("cache-fail")).unwrap();
    let missing = Failure::new(FailureKind::NotFound, "Request asset was not found");
    cache.put_failure("asset-1", &missing).unwrap();
    let Cached::Failed(f) = cache.get("asset-1") else {
        panic!("not remembered");
    };
    assert_eq!(f, missing);
    let later = SystemTime::now() + NEGATIVE_TTL + Duration::from_secs(1);
    assert!(matches!(cache.get_at("asset-1", later), Cached::Miss));
    // Network errors aren't remembered.
    cache
        .put_failure("asset-2", &Failure::new(FailureKind::Network, "timed out"))
        .unwrap();
    assert!(matches!(cache.get("asset-2"), Cached::Miss));
}
