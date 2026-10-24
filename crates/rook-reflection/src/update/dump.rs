//! The subset of Roblox's Full API dump format the update reads.

use serde::Deserialize;

use super::UpdateError;
use crate::members::{Param, Security, Tag, TypeKind, TypeRef};

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
pub(super) struct Root {
    pub classes: Vec<Class>,
    #[serde(default)]
    pub enums: Vec<Enum>,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
pub(super) struct Enum {
    pub name: String,
    pub items: Vec<EnumItem>,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
pub(super) struct EnumItem {
    pub name: String,
    pub value: u32,
    #[serde(default)]
    pub legacy_names: Vec<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
pub(super) struct Class {
    pub name: String,
    pub superclass: String,
    #[serde(default)]
    pub tags: Vec<serde_json::Value>,
    pub members: Vec<Member>,
}

#[derive(Deserialize, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub(super) struct Member {
    pub member_type: String,
    pub name: String,
    #[serde(default)]
    parameters: Vec<DumpParam>,
    #[serde(default)]
    return_type: Option<serde_json::Value>,
    #[serde(default)]
    pub security: serde_json::Value,
    #[serde(default)]
    pub tags: Vec<serde_json::Value>,
    /// Properties only.
    #[serde(default)]
    pub value_type: Option<DumpType>,
    #[serde(default)]
    pub serialization: Option<Serialization>,
    /// Properties only: the value as Studio prints it, or a placeholder
    /// such as `__api_dump_class_not_creatable__`.
    #[serde(default)]
    pub default: Option<String>,
}

#[derive(Deserialize, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub(super) struct Serialization {
    pub can_save: bool,
}

#[derive(Deserialize, PartialEq)]
#[serde(rename_all = "PascalCase")]
struct DumpParam {
    name: String,
    #[serde(rename = "Type")]
    ty: DumpType,
    default: Option<String>,
}

#[derive(Deserialize, PartialEq)]
#[serde(rename_all = "PascalCase")]
pub(super) struct DumpType {
    pub category: String,
    pub name: String,
}

impl DumpType {
    fn convert(&self) -> Result<TypeRef, UpdateError> {
        let kind = TypeKind::from_api_dump(&self.category)
            .ok_or_else(|| UpdateError::Shape(format!("type category {}", self.category)))?;
        let (name, nullable) = match self.name.strip_suffix('?') {
            Some(n) => (n, true),
            None => (self.name.as_str(), false),
        };
        Ok(TypeRef {
            kind,
            name: name.to_owned(),
            nullable,
        })
    }
}

fn security(v: &str) -> Result<Security, UpdateError> {
    serde_json::from_value(serde_json::Value::String(v.to_owned()))
        .map_err(|_| UpdateError::Shape(format!("security level {v}")))
}

/// The plain string tags of a class or member.
pub(super) fn string_tags(tags: &[serde_json::Value]) -> impl Iterator<Item = &str> {
    tags.iter().filter_map(|t| t.as_str())
}

impl Member {
    pub(super) fn params(&self) -> Result<Vec<Param>, UpdateError> {
        self.parameters
            .iter()
            .map(|p| {
                Ok(Param {
                    name: p.name.clone(),
                    ty: p.ty.convert()?,
                    default: p.default.clone(),
                })
            })
            .collect()
    }

    pub(super) fn returns(&self) -> Result<Vec<TypeRef>, UpdateError> {
        let types: Vec<DumpType> = match &self.return_type {
            None => Vec::new(),
            Some(v @ serde_json::Value::Array(_)) => serde_json::from_value(v.clone())?,
            Some(v) => vec![serde_json::from_value(v.clone())?],
        };
        types.iter().map(DumpType::convert).collect()
    }

    pub(super) fn member_security(&self) -> Result<Security, UpdateError> {
        match &self.security {
            serde_json::Value::String(s) => security(s),
            other => Err(UpdateError::Shape(format!("member security {other}"))),
        }
    }

    /// A property's `[read, write]` security.
    pub(super) fn property_security_pair(&self) -> Result<[Security; 2], UpdateError> {
        let get = |key: &str| match self.security.get(key) {
            Some(serde_json::Value::String(s)) => security(s),
            _ => Err(UpdateError::Shape(format!(
                "property security of {}",
                self.name
            ))),
        };
        Ok([get("Read")?, get("Write")?])
    }

    /// `Some([read, write])` when either is restricted.
    pub(super) fn property_security(&self) -> Result<Option<[Security; 2]>, UpdateError> {
        let pair = self.property_security_pair()?;
        Ok((pair != [Security::None; 2]).then_some(pair))
    }

    /// String tags Rook cares about; others (replication, threading) are dropped.
    pub(super) fn tags(&self) -> Vec<Tag> {
        self.tags
            .iter()
            .filter_map(|t| serde_json::from_value(t.clone()).ok())
            .collect()
    }

    pub(super) fn preferred(&self) -> Option<String> {
        self.tags.iter().find_map(|t| {
            t.get("PreferredDescriptorName")
                .and_then(|v| v.as_str())
                .map(str::to_owned)
        })
    }
}
