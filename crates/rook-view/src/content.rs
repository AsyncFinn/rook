//! What the scene draws from files: mesh files and images loaded off the
//! engine thread ([`loader`]), what parts draw while they load
//! ([`pending`]), parts drawn again as their files arrive, a frame's
//! budget at a time ([`arrivals`]), what didn't load ([`report`]), and what the renderer
//! takes from the user's Roblox install ([`installed`]).

pub(super) mod arrivals;
pub(crate) mod installed;
pub(super) mod loader;
pub(super) mod pending;
pub(super) mod report;
