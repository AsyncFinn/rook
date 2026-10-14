//! Rook-owned data layered over the upstream database: implementation
//! status, behavior-provenance IDs, and Rook extension members.
//!
//! The format is JSON so tooling can generate and check it. Every entry must
//! name a class or property that exists in the database (or be marked as a
//! Rook extension), so stale entries are caught when the database is bumped.

use std::collections::BTreeMap;

use serde::Deserialize;

use crate::Database;

/// How much of a class or member Rook implements.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Status {
    /// Exists and round-trips, but has no behavior (the default for everything).
    DataOnly,
    Partial,
    Implemented,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ClassEntry {
    pub status: Status,
    /// Behavior-note IDs (e.g. `GUI-012`) documenting where behavior came from.
    #[serde(default)]
    pub provenance: Vec<String>,
    /// True for classes that exist only in Rook's extensions profile.
    #[serde(default)]
    pub rook_extension: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PropertyEntry {
    pub status: Status,
    #[serde(default)]
    pub provenance: Vec<String>,
    #[serde(default)]
    pub rook_extension: bool,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Overlay {
    #[serde(default)]
    pub classes: BTreeMap<String, ClassEntry>,
    /// Keyed by `Class.Property`.
    #[serde(default)]
    pub properties: BTreeMap<String, PropertyEntry>,
}

#[derive(Debug, thiserror::Error)]
pub enum OverlayError {
    #[error("overlay is not valid JSON: {0}")]
    Parse(#[from] serde_json::Error),
    #[error("overlay entry `{0}` names a class that doesn't exist")]
    UnknownClass(String),
    #[error("overlay key `{0}` must be written as `Class.Property`")]
    MalformedPropertyKey(String),
    #[error("overlay entry `{0}` names a property that doesn't exist")]
    UnknownProperty(String),
}

impl Overlay {
    /// Rook's built-in overlay.
    pub fn builtin() -> Result<Self, OverlayError> {
        Self::parse(include_str!("../data/overlay.json"))
    }

    pub fn parse(json: &str) -> Result<Self, OverlayError> {
        Ok(serde_json::from_str(json)?)
    }

    /// Check that every non-extension entry names something in the database.
    pub fn validate(&self, db: &Database) -> Result<(), OverlayError> {
        for (name, entry) in &self.classes {
            if !entry.rook_extension && db.class_by_name(name).is_none() {
                return Err(OverlayError::UnknownClass(name.clone()));
            }
        }
        for (key, entry) in &self.properties {
            if entry.rook_extension {
                continue;
            }
            let (class, prop) = key
                .split_once('.')
                .ok_or_else(|| OverlayError::MalformedPropertyKey(key.clone()))?;
            let class = db
                .class_by_name(class)
                .ok_or_else(|| OverlayError::UnknownClass(key.clone()))?;
            if db.find_property(class.id, prop).is_none() {
                return Err(OverlayError::UnknownProperty(key.clone()));
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database;

    #[test]
    fn builtin_overlay_is_valid() {
        Overlay::builtin().unwrap().validate(database()).unwrap();
    }

    #[test]
    fn rejects_unknown_names() {
        let o = Overlay::parse(r#"{"classes":{"NotAClass":{"status":"partial"}}}"#).unwrap();
        assert!(matches!(
            o.validate(database()),
            Err(OverlayError::UnknownClass(_))
        ));
        let o = Overlay::parse(r#"{"properties":{"Part.Nope":{"status":"partial"}}}"#).unwrap();
        assert!(matches!(
            o.validate(database()),
            Err(OverlayError::UnknownProperty(_))
        ));
        let o = Overlay::parse(
            r#"{"classes":{"RookDebugService":{"status":"partial","rook_extension":true}}}"#,
        )
        .unwrap();
        o.validate(database()).unwrap();
    }
}
