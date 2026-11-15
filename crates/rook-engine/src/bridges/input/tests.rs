use super::keys::legacy_key_string;

#[test]
fn legacy_key_strings() {
    assert_eq!(legacy_key_string("W").as_deref(), Some("w"));
    assert_eq!(legacy_key_string("Seven").as_deref(), Some("7"));
    assert_eq!(legacy_key_string("Space").as_deref(), Some(" "));
    assert_eq!(legacy_key_string("LeftShift"), None);
}
