//! Camera's methods: rays and projection, what moves it, and what stands between it and what it looks at.

use crate::api::methods::{RawMethod, first, mutate};

pub(crate) mod moves;
pub(crate) mod occlusion;
pub(crate) mod rays;

pub(super) fn native(owner: &str, name: &str) -> Option<RawMethod> {
    first(
        &[rays::native, moves::native, occlusion::native],
        owner,
        name,
    )
}
