use super::*;

#[test]
fn flag_beats_env_beats_file_beats_default() {
    assert_eq!(pick(Some(1), Some(2), Some(3), 4), 1);
    assert_eq!(pick(None, Some(2), Some(3), 4), 2);
    assert_eq!(pick(None, None, Some(3), 4), 3);
    assert_eq!(pick(None, None, None, 4), 4);
}

#[test]
fn the_written_file_sets_every_key() {
    let written = Settings::written_default();
    assert_eq!(written.cache_max_size.as_deref(), Some("2GB"));
    assert_eq!(written.master_volume, Some(1.0));
    assert_eq!(written.audio_device.as_deref(), Some("default"));
    assert_eq!(written.content_order.as_ref().map(Vec::len), Some(4));
    // Every field is in the file (`null` ones as keys).
    let keys = serde_json::to_value(Settings::default()).unwrap();
    for key in keys.as_object().unwrap().keys() {
        assert!(WRITTEN.contains(&format!("\"{key}\"")), "{key}");
    }
}

#[test]
fn reads_hand_edits() {
    let edited = "{\n  // more room\n  \"cacheMaxSize\": \"5GB\",\n  \"assetDelivery\": false,\n}";
    let settings = Settings::parse(edited).unwrap();
    assert_eq!(settings.cache_max_size.as_deref(), Some("5GB"));
    assert_eq!(settings.asset_delivery, Some(false));
}
