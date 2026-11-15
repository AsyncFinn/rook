//! Every Instance's own methods: the tree, attributes and tags, services, and bindables.

use super::*;

mod attributes;
pub(crate) mod bindables;
mod services;
pub(crate) mod tree;

pub(super) fn native(owner: &str, name: &str) -> Option<RawMethod> {
    first(
        &[
            tree::native,
            attributes::native,
            services::native,
            bindables::native,
        ],
        owner,
        name,
    )
}

pub(super) fn luau_source(owner: &str, name: &str) -> Option<&'static str> {
    first(
        &[
            tree::luau_source,
            services::luau_source,
            bindables::luau_source,
        ],
        owner,
        name,
    )
}
