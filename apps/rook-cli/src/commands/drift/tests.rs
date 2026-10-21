use super::*;

#[test]
fn depth_counts_folders_below_src() {
    assert_eq!(code::folders_below_src("crates/a/src/lib.rs"), Some(0));
    assert_eq!(code::folders_below_src("crates/a/src/x/y/z/f.rs"), Some(3));
    assert_eq!(code::folders_below_src("crates/a/build.rs"), None);
}
