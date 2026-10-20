//! Functions, events and callbacks of every class, plus property security.
//!
//! `rbx_reflection_database` only describes properties, so Rook keeps its
//! own compact list of the rest, generated from Roblox's Full API dump for
//! the same pinned version and committed as `data/members.json`.
//!
//! Regenerate with `rook-tools gen-members <Full-API-Dump.json>`. The file has
//! one class per line so updates diff cleanly.
//!
//! Types are written `<kind>:<name>`, where the kind is `p` (primitive),
//! `d` (data type), `c` (class), `e` (enum) or `g` (group, such as `Tuple`).
//! A trailing `?` on the name means the value may be nil.

mod generate;

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// The committed member list.
#[derive(Debug, Serialize, Deserialize)]
pub struct Members {
    /// Roblox version the list was generated from.
    pub version: String,
    /// Where the API dump came from.
    pub source: String,
    pub classes: BTreeMap<String, ClassMembers>,
    /// Old enum item names that still resolve, as enum → old name → item
    /// (`Enum.KeyCode.Unknown` is `Enum.KeyCode.None` today).
    #[serde(default)]
    pub enum_legacy_names: BTreeMap<String, BTreeMap<String, String>>,
}

/// Members declared directly on one class (not inherited ones).
#[derive(Debug, Default, Serialize, Deserialize)]
pub struct ClassMembers {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub functions: Vec<Function>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub events: Vec<Event>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub callbacks: Vec<Function>,
    /// Properties whose read or write security is not `None`, as
    /// `[read, write]`. Properties not listed are open to scripts.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub property_security: BTreeMap<String, [Security; 2]>,
}

/// A function or callback.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Function {
    pub name: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub params: Vec<Param>,
    /// Usually one type; a few functions return several values.
    pub returns: Vec<TypeRef>,
    #[serde(default, skip_serializing_if = "Security::is_none")]
    pub security: Security,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<Tag>,
    /// The replacement Roblox suggests for a deprecated member.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preferred: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    pub name: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub params: Vec<Param>,
    #[serde(default, skip_serializing_if = "Security::is_none")]
    pub security: Security,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<Tag>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preferred: Option<String>,
}

/// `[name, type]` or `[name, type, default]`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Param {
    pub name: String,
    pub ty: TypeRef,
    /// Default value as written in the API dump (Lua-ish source text).
    pub default: Option<String>,
}

impl Serialize for Param {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let ty = self.ty.to_string();
        match &self.default {
            Some(d) => [self.name.as_str(), &ty, d].serialize(s),
            None => [self.name.as_str(), &ty].serialize(s),
        }
    }
}

impl<'de> Deserialize<'de> for Param {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let parts = Vec::<String>::deserialize(d)?;
        let mut it = parts.into_iter();
        let (Some(name), Some(ty)) = (it.next(), it.next()) else {
            return Err(serde::de::Error::custom("parameter needs a name and type"));
        };
        Ok(Param {
            name,
            ty: ty.parse().map_err(serde::de::Error::custom)?,
            default: it.next(),
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TypeKind {
    Primitive,
    DataType,
    Class,
    Enum,
    Group,
}

impl TypeKind {
    fn code(self) -> char {
        match self {
            Self::Primitive => 'p',
            Self::DataType => 'd',
            Self::Class => 'c',
            Self::Enum => 'e',
            Self::Group => 'g',
        }
    }

    fn from_api_dump(category: &str) -> Option<Self> {
        Some(match category {
            "Primitive" => Self::Primitive,
            "DataType" => Self::DataType,
            "Class" => Self::Class,
            "Enum" => Self::Enum,
            "Group" => Self::Group,
            _ => return None,
        })
    }
}

/// A parameter, return or event argument type.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TypeRef {
    pub kind: TypeKind,
    /// Name without the nullable marker.
    pub name: String,
    pub nullable: bool,
}

impl std::fmt::Display for TypeRef {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let q = if self.nullable { "?" } else { "" };
        write!(f, "{}:{}{q}", self.kind.code(), self.name)
    }
}

impl std::str::FromStr for TypeRef {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, String> {
        let (code, name) = s.split_once(':').ok_or_else(|| format!("bad type {s:?}"))?;
        let kind = match code {
            "p" => TypeKind::Primitive,
            "d" => TypeKind::DataType,
            "c" => TypeKind::Class,
            "e" => TypeKind::Enum,
            "g" => TypeKind::Group,
            _ => return Err(format!("bad type kind in {s:?}")),
        };
        let (name, nullable) = match name.strip_suffix('?') {
            Some(n) => (n, true),
            None => (name, false),
        };
        Ok(TypeRef {
            kind,
            name: name.to_owned(),
            nullable,
        })
    }
}

impl Serialize for TypeRef {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.collect_str(self)
    }
}

impl<'de> Deserialize<'de> for TypeRef {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        String::deserialize(d)?
            .parse()
            .map_err(serde::de::Error::custom)
    }
}

/// Who may use a member. Game scripts run at `None`; plugins and the
/// command bar add `PluginSecurity`. The rest are Roblox-internal.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Security {
    #[default]
    None,
    PluginSecurity,
    LocalUserSecurity,
    RobloxScriptSecurity,
    RobloxSecurity,
    NotAccessibleSecurity,
}

impl Security {
    pub fn is_none(&self) -> bool {
        *self == Self::None
    }

    /// The capability Roblox's permission errors name: the level without
    /// "Security" (observed: "lacking capability NotAccessible" for
    /// MeshPart.MeshId, "RobloxScript" for InsertService:LoadLocalAsset).
    pub fn capability(self) -> &'static str {
        match self {
            Self::None => "None",
            Self::PluginSecurity => "Plugin",
            Self::LocalUserSecurity => "LocalUser",
            Self::RobloxScriptSecurity => "RobloxScript",
            // Observed in Studio 0.742, writing AudioPlayer.IsPlaying.
            Self::RobloxSecurity => "RobloxEngine",
            Self::NotAccessibleSecurity => "NotAccessible",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Tag {
    Deprecated,
    Hidden,
    NotBrowsable,
    /// The function may yield the calling thread.
    Yields,
    CanYield,
    NoYield,
    /// Implemented directly against the Lua state (for example `WaitForChild`).
    CustomLuaState,
}

#[cfg(test)]
mod tests;
