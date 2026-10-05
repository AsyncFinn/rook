//! Class, property and enum database for Raven.
//!
//! Built from the pinned `rbx_reflection_database` bundled with the crate.
//! Raven always uses the bundled copy (never a locally installed override)
//! so behavior doesn't depend on the machine it runs on.
//!
//! At first use the database is compiled into dense numeric IDs
//! ([`ClassId`], [`PropertyId`], [`EnumId`]). Each class gets a full property
//! list including inherited properties, and per-class defaults resolved
//! through the superclass chain.

mod database;
mod overlay;

pub use database::{
    Class, ClassFlags, ClassId, Database, EnumId, EnumInfo, Property, PropertyFlags, PropertyId,
    PropertyKind, PropertyType, Scriptability, Serialization,
};
pub use overlay::{Overlay, OverlayError};

use std::sync::LazyLock;

static DATABASE: LazyLock<Database> =
    LazyLock::new(|| Database::build(rbx_reflection_database::get_bundled()));

/// The process-wide Raven database, built on first use.
pub fn database() -> &'static Database {
    &DATABASE
}

/// The pinned upstream database, for crates (like `rbx_binary` and `rbx_xml`)
/// that take one directly.
pub fn rbx_database() -> &'static rbx_reflection::ReflectionDatabase<'static> {
    rbx_reflection_database::get_bundled()
}
