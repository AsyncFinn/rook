//! Members Roblox answers from its servers: HttpService, users, text filtering and DataStores, through [`requests`] where Rook has an endpoint.

use crate::api::methods::{RawMethod, first};

pub(crate) mod data_stores;
pub(crate) mod filtering;
pub(crate) mod http;
mod json;
pub(crate) mod requests;
mod users;

pub(super) fn native(owner: &str, name: &str) -> Option<RawMethod> {
    first(
        &[
            data_stores::native,
            http::native,
            users::native,
            filtering::native,
        ],
        owner,
        name,
    )
}

pub(super) fn luau_source(owner: &str, name: &str) -> Option<&'static str> {
    first(&[data_stores::luau_source, http::luau_source], owner, name)
}
