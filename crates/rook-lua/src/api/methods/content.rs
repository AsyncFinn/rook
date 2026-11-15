//! Loading content from scripts: published models (`require`, `InsertService`) and `ContentProvider`'s preloads.

use crate::api::methods::{RawMethod, first};

pub(crate) mod assets;
pub(crate) mod preload;

pub(super) fn native(owner: &str, name: &str) -> Option<RawMethod> {
    first(&[preload::native, assets::native], owner, name)
}
