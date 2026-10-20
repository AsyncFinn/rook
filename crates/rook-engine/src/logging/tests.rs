use super::*;

#[test]
fn verbose_takes_a_subsystem_but_not_a_file() {
    let args = |a: &[&str]| fix_verbose_args(a.iter().map(std::string::ToString::to_string));
    assert_eq!(
        args(&["rook-player", "--verbose", "place.rbxl"]),
        ["rook-player", "--verbose", "place.rbxl"]
    );
    assert_eq!(
        args(&["rook-player", "--verbose", "content", "place.rbxl"]),
        ["rook-player", "--verbose=content", "place.rbxl"]
    );
    let content = Filter::parse("content", Level::INFO).unwrap();
    assert!(content.shows("rook_content::loader", Level::TRACE));
    assert!(!content.shows("rook_terrain", Level::DEBUG));
    assert!(!content.shows("wgpu_core", Level::INFO));
}
