//! Class, property and enum database for Rook.
//!
//! Built from Rook's copy of the reflection database (`data/database.msgpack`,
//! rbx-dom's format), which tracks the live Roblox version: `rook-tools
//! gen-api` updates it from each new API dump ([`update`]). It's compiled
//! in, so behavior doesn't depend on the machine Rook runs on.
//!
//! At first use the database is compiled into dense numeric IDs
//! ([`ClassId`], [`PropertyId`], [`EnumId`]). Each class gets a full property
//! list including inherited properties, and per-class defaults resolved
//! through the superclass chain.
//!
//! Functions, events and callbacks come from [`members()`], a compact list
//! generated from the API dump of the same Roblox version.

mod database;
pub mod members;
mod overlay;
pub mod update;

pub use database::{
    ClassId, Database, EnumId, Property, PropertyFlags, PropertyId, PropertyKind, PropertyType,
    Scriptability, Serialization,
};
pub use members::Members;
pub use overlay::{Overlay, OverlayError};

use std::sync::LazyLock;

/// An API data file, compressed by `build.rs`. The data parsed from it
/// lives as long as the process and may borrow from it, as it did from the
/// exe when embedded raw.
fn unpack(packed: &[u8]) -> &'static [u8] {
    zstd::decode_all(packed)
        .expect("build.rs compressed it")
        .leak()
}

static UPSTREAM: LazyLock<rbx_reflection::ReflectionDatabase<'static>> = LazyLock::new(|| {
    let bytes = unpack(include_bytes!(concat!(
        env!("OUT_DIR"),
        "/database.msgpack.zst"
    )));
    rmp_serde::from_slice(bytes)
        .expect("data/database.msgpack is valid; regenerate with `rook-tools gen-api`")
});

static DATABASE: LazyLock<Database> = LazyLock::new(|| Database::build(&UPSTREAM));

/// The process-wide Rook database, built on first use.
pub fn database() -> &'static Database {
    &DATABASE
}

/// The database in rbx-dom's format, for crates (like `rbx_binary` and
/// `rbx_xml`) that take one directly.
pub fn rbx_database() -> &'static rbx_reflection::ReflectionDatabase<'static> {
    &UPSTREAM
}

static MEMBERS: LazyLock<Members> = LazyLock::new(|| {
    let bytes = unpack(include_bytes!(concat!(
        env!("OUT_DIR"),
        "/members.json.zst"
    )));
    serde_json::from_slice(bytes)
        .expect("data/members.json is valid; regenerate with `rook-tools gen-api`")
});

/// Functions, events and callbacks for the same Roblox version as the database.
pub fn members() -> &'static Members {
    &MEMBERS
}

/// The value of an enum's item by name, accepting the legacy names Roblox
/// still resolves (`Enum.KeyCode.Unknown` is `Enum.KeyCode.None`).
pub fn enum_item_value(id: EnumId, name: &str) -> Option<u32> {
    let info = database().enum_info(id);
    let lookup = |n: &str| info.items.iter().find(|(i, _)| *i == n).map(|(_, v)| *v);
    lookup(name).or_else(|| {
        let current = members().enum_legacy_names.get(info.name)?.get(name)?;
        lookup(current)
    })
}

/// The Roblox version Rook's API data was built from, as
/// `major.minor.patch.build`.
pub fn roblox_version() -> String {
    let [a, b, c, d] = rbx_database().version;
    format!("{a}.{b}.{c}.{d}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use members::{Security, Tag};

    #[test]
    fn member_list_matches_database_version() {
        assert_eq!(members().version, roblox_version());
    }

    #[test]
    fn member_list_covers_the_class_database() {
        let missing: Vec<_> = database()
            .classes()
            .iter()
            .filter(|c| !members().classes.contains_key(c.name))
            .map(|c| c.name)
            .collect();
        assert!(missing.is_empty(), "classes without members: {missing:?}");
    }

    #[test]
    fn instance_members() {
        let inst = &members().classes["Instance"];
        let wfc = inst
            .functions
            .iter()
            .find(|f| f.name == "WaitForChild")
            .unwrap();
        assert!(wfc.tags.contains(&Tag::CustomLuaState));
        assert_eq!(wfc.security, Security::None);
        assert!(inst.events.iter().any(|e| e.name == "ChildAdded"));
        let remove = inst.functions.iter().find(|f| f.name == "remove").unwrap();
        assert_eq!(remove.preferred.as_deref(), Some("Remove"));
    }
}
