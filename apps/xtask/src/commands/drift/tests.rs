use super::*;

#[test]
fn depth_counts_folders_below_src() {
    assert_eq!(code::folders_below_src("crates/a/src/lib.rs"), Some(0));
    assert_eq!(code::folders_below_src("crates/a/src/x/y/z/f.rs"), Some(3));
    assert_eq!(code::folders_below_src("crates/a/build.rs"), None);
}

#[test]
fn package_name_reads_the_package_table() {
    let manifest = "[package]\nname = \"rook-dom\"\n\n[dependencies]\nname = \"x\"\n";
    assert_eq!(code::package_name(manifest), Some("rook-dom"));
    assert_eq!(code::package_name("[dependencies]\nname = \"x\"\n"), None);
}
