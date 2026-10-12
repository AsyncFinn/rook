//! Class, property and enum database for Rook.
//!
//! Built from the pinned `rbx_reflection_database` bundled with the crate.
//! Rook always uses the bundled copy (never a locally installed override)
//! so behavior doesn't depend on the machine it runs on.
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

pub use database::{
    Class, ClassFlags, ClassId, Database, EnumId, EnumInfo, Property, PropertyFlags, PropertyId,
    PropertyKind, PropertyType, Scriptability, Serialization,
};
pub use members::Members;
pub use overlay::{Overlay, OverlayError};

use std::sync::LazyLock;

static DATABASE: LazyLock<Database> =
    LazyLock::new(|| Database::build(rbx_reflection_database::get_bundled()));

/// The process-wide Rook database, built on first use.
pub fn database() -> &'static Database {
    &DATABASE
}

/// The pinned upstream database, for crates (like `rbx_binary` and `rbx_xml`)
/// that take one directly.
pub fn rbx_database() -> &'static rbx_reflection::ReflectionDatabase<'static> {
    rbx_reflection_database::get_bundled()
}

static MEMBERS: LazyLock<Members> = LazyLock::new(|| {
    serde_json::from_str(include_str!("../data/members.json"))
        .expect("data/members.json is valid; regenerate with `rook gen-members`")
});

/// Functions, events and callbacks for the pinned Roblox version.
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

/// The pinned Roblox version, as `major.minor.patch.build`.
pub fn roblox_version() -> String {
    let [a, b, c, d] = rbx_database().version;
    format!("{a}.{b}.{c}.{d}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use members::{Security, Tag};

    #[test]
    fn member_list_matches_pinned_version() {
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
