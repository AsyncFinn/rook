//! Loading content from scripts: published models (`require`, `InsertService`) and `ContentProvider`'s preloads.

use super::*;

pub(crate) mod assets;
pub(crate) mod preload;

pub(super) fn native(owner: &str, name: &str) -> Option<RawMethod> {
    first(&[preload::native, assets::native], owner, name)
}
