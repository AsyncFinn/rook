//! Parts in the physics world: joints and assemblies, models' pivots and bounds, collision groups, and scene queries.

use crate::api::methods::{RawMethod, first, mutate};

mod assemblies;
mod collision_groups;
pub(crate) mod models;
mod queries;

pub(super) fn native(owner: &str, name: &str) -> Option<RawMethod> {
    first(
        &[
            models::native,
            assemblies::native,
            collision_groups::native,
            queries::native,
        ],
        owner,
        name,
    )
}
