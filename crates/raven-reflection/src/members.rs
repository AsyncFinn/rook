//! Functions, events and callbacks of every class, plus property security.
//!
//! `rbx_reflection_database` only describes properties, so Raven keeps its
//! own compact list of the rest, generated from Roblox's Full API dump for
//! the same pinned version and committed as `data/members.json`.
//!
//! Regenerate with `raven gen-members <Full-API-Dump.json>`. The file has
//! one class per line so updates diff cleanly.
//!
//! Types are written `<kind>:<name>`, where the kind is `p` (primitive),
//! `d` (data type), `c` (class), `e` (enum) or `g` (group, such as `Tuple`).
//! A trailing `?` on the name means the value may be nil.

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

#[derive(Debug, thiserror::Error)]
pub enum GenerateError {
    #[error("API dump is not valid JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("unexpected API dump content: {0}")]
    Shape(String),
}

impl Members {
    /// Build the list from a Full API dump.
    pub fn from_api_dump(dump: &str, version: &str, source: &str) -> Result<Self, GenerateError> {
        let dump: dump::Root = serde_json::from_str(dump)?;
        let mut classes = BTreeMap::new();
        for class in dump.classes {
            let mut out = ClassMembers::default();
            for member in class.members {
                let tags = member.tags();
                let preferred = member.preferred();
                match member.member_type.as_str() {
                    "Property" => {
                        if let Some(sec) = member.property_security()? {
                            out.property_security.insert(member.name, sec);
                        }
                    }
                    "Function" | "Callback" => {
                        let f = Function {
                            params: member.params()?,
                            returns: member.returns()?,
                            security: member.member_security()?,
                            name: member.name.clone(),
                            tags,
                            preferred,
                        };
                        if member.member_type == "Function" {
                            out.functions.push(f);
                        } else {
                            out.callbacks.push(f);
                        }
                    }
                    "Event" => out.events.push(Event {
                        params: member.params()?,
                        security: member.member_security()?,
                        name: member.name,
                        tags,
                        preferred,
                    }),
                    other => return Err(GenerateError::Shape(format!("member type {other}"))),
                }
            }
            classes.insert(class.name, out);
        }
        Ok(Members {
            version: version.to_owned(),
            source: source.to_owned(),
            classes,
        })
    }

    /// Serialize with one class per line.
    pub fn to_json(&self) -> String {
        let mut out = String::from("{\n");
        out += &format!("\"version\": {},\n", json(&self.version));
        out += &format!("\"source\": {},\n", json(&self.source));
        out += "\"classes\": {\n";
        let n = self.classes.len();
        for (i, (name, members)) in self.classes.iter().enumerate() {
            let comma = if i + 1 < n { "," } else { "" };
            out += &format!("{}: {}{comma}\n", json(name), json(members));
        }
        out += "}\n}\n";
        out
    }
}

fn json<T: Serialize + ?Sized>(v: &T) -> String {
    serde_json::to_string(v).expect("member list serializes")
}

/// The subset of the API dump format the generator reads.
mod dump {
    use super::{GenerateError, Param, Security, Tag, TypeKind, TypeRef};
    use serde::Deserialize;

    #[derive(Deserialize)]
    #[serde(rename_all = "PascalCase")]
    pub struct Root {
        pub classes: Vec<Class>,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "PascalCase")]
    pub struct Class {
        pub name: String,
        pub members: Vec<Member>,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "PascalCase")]
    pub struct Member {
        pub member_type: String,
        pub name: String,
        #[serde(default)]
        parameters: Vec<DumpParam>,
        #[serde(default)]
        return_type: Option<serde_json::Value>,
        #[serde(default)]
        security: serde_json::Value,
        #[serde(default)]
        tags: Vec<serde_json::Value>,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "PascalCase")]
    struct DumpParam {
        name: String,
        #[serde(rename = "Type")]
        ty: DumpType,
        default: Option<String>,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "PascalCase")]
    struct DumpType {
        category: String,
        name: String,
    }

    impl DumpType {
        fn convert(&self) -> Result<TypeRef, GenerateError> {
            let kind = TypeKind::from_api_dump(&self.category)
                .ok_or_else(|| GenerateError::Shape(format!("type category {}", self.category)))?;
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

    fn security(v: &str) -> Result<Security, GenerateError> {
        serde_json::from_value(serde_json::Value::String(v.to_owned()))
            .map_err(|_| GenerateError::Shape(format!("security level {v}")))
    }

    impl Member {
        pub fn params(&self) -> Result<Vec<Param>, GenerateError> {
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

        pub fn returns(&self) -> Result<Vec<TypeRef>, GenerateError> {
            let types: Vec<DumpType> = match &self.return_type {
                None => Vec::new(),
                Some(v @ serde_json::Value::Array(_)) => serde_json::from_value(v.clone())?,
                Some(v) => vec![serde_json::from_value(v.clone())?],
            };
            types.iter().map(DumpType::convert).collect()
        }

        pub fn member_security(&self) -> Result<Security, GenerateError> {
            match &self.security {
                serde_json::Value::String(s) => security(s),
                other => Err(GenerateError::Shape(format!("member security {other}"))),
            }
        }

        /// `Some([read, write])` when either is restricted.
        pub fn property_security(&self) -> Result<Option<[Security; 2]>, GenerateError> {
            let get = |key: &str| match self.security.get(key) {
                Some(serde_json::Value::String(s)) => security(s),
                _ => Err(GenerateError::Shape(format!(
                    "property security of {}",
                    self.name
                ))),
            };
            let pair = [get("Read")?, get("Write")?];
            Ok((pair != [Security::None; 2]).then_some(pair))
        }

        /// String tags Raven cares about; others (replication, threading) are dropped.
        pub fn tags(&self) -> Vec<Tag> {
            self.tags
                .iter()
                .filter_map(|t| serde_json::from_value(t.clone()).ok())
                .collect()
        }

        pub fn preferred(&self) -> Option<String> {
            self.tags.iter().find_map(|t| {
                t.get("PreferredDescriptorName")
                    .and_then(|v| v.as_str())
                    .map(str::to_owned)
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DUMP: &str = r#"{"Classes": [{"Name": "Instance", "Superclass": "Object", "Members": [
        {"MemberType": "Function", "Name": "FindFirstChild",
         "Parameters": [{"Name": "name", "Type": {"Category": "Primitive", "Name": "string"}},
                        {"Default": "false", "Name": "recursive", "Type": {"Category": "Primitive", "Name": "bool"}}],
         "ReturnType": {"Category": "Class", "Name": "Instance"}, "Security": "None", "ThreadSafety": "Safe"},
        {"MemberType": "Function", "Name": "children", "Parameters": [],
         "ReturnType": {"Category": "DataType", "Name": "Instances"}, "Security": "None",
         "Tags": ["Deprecated", {"PreferredDescriptorName": "GetChildren", "ThreadSafety": "Safe"}]},
        {"MemberType": "Event", "Name": "ChildAdded",
         "Parameters": [{"Name": "child", "Type": {"Category": "Class", "Name": "Instance"}}], "Security": "None"},
        {"MemberType": "Property", "Name": "Name", "Security": {"Read": "None", "Write": "None"},
         "ValueType": {"Category": "Primitive", "Name": "string"}},
        {"MemberType": "Property", "Name": "UniqueId", "Security": {"Read": "RobloxSecurity", "Write": "RobloxSecurity"},
         "ValueType": {"Category": "DataType", "Name": "UniqueId"}}
    ]}]}"#;

    #[test]
    fn generates_and_round_trips() {
        let members = Members::from_api_dump(DUMP, "test", "unit test").unwrap();
        let text = members.to_json();
        let back: Members = serde_json::from_str(&text).unwrap();
        let inst = &back.classes["Instance"];

        let ffc = &inst.functions[0];
        assert_eq!(ffc.name, "FindFirstChild");
        assert_eq!(ffc.params[1].default.as_deref(), Some("false"));
        assert_eq!(ffc.returns[0].to_string(), "c:Instance");

        let children = &inst.functions[1];
        assert_eq!(children.tags, [Tag::Deprecated]);
        assert_eq!(children.preferred.as_deref(), Some("GetChildren"));

        assert_eq!(inst.events[0].params[0].ty.kind, TypeKind::Class);
        assert_eq!(inst.property_security.len(), 1);
        assert_eq!(
            inst.property_security["UniqueId"],
            [Security::RobloxSecurity; 2]
        );
    }

    #[test]
    fn nullable_types_parse() {
        let t: TypeRef = "p:string?".parse().unwrap();
        assert!(t.nullable);
        assert_eq!(t.name, "string");
        assert_eq!(t.to_string(), "p:string?");
    }
}
