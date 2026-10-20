//! Generating the member list from Roblox's Full API dump
//! (`rook-tools gen-members`), and writing it as `data/members.json`.

use super::*;

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
        let mut enum_legacy_names: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();
        for e in dump.enums {
            for item in e.items {
                for legacy in item.legacy_names {
                    enum_legacy_names
                        .entry(e.name.clone())
                        .or_default()
                        .insert(legacy, item.name.clone());
                }
            }
        }
        Ok(Members {
            version: version.to_owned(),
            source: source.to_owned(),
            classes,
            enum_legacy_names,
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
        out += "},\n";
        out += &format!("\"enum_legacy_names\": {}\n", json(&self.enum_legacy_names));
        out += "}\n";
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
        #[serde(default)]
        pub legacy_names: Vec<String>,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "PascalCase")]
    pub(super) struct Class {
        pub name: String,
        pub members: Vec<Member>,
    }

    #[derive(Deserialize)]
    #[serde(rename_all = "PascalCase")]
    pub(super) struct Member {
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
        pub(super) fn params(&self) -> Result<Vec<Param>, GenerateError> {
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

        pub(super) fn returns(&self) -> Result<Vec<TypeRef>, GenerateError> {
            let types: Vec<DumpType> = match &self.return_type {
                None => Vec::new(),
                Some(v @ serde_json::Value::Array(_)) => serde_json::from_value(v.clone())?,
                Some(v) => vec![serde_json::from_value(v.clone())?],
            };
            types.iter().map(DumpType::convert).collect()
        }

        pub(super) fn member_security(&self) -> Result<Security, GenerateError> {
            match &self.security {
                serde_json::Value::String(s) => security(s),
                other => Err(GenerateError::Shape(format!("member security {other}"))),
            }
        }

        /// `Some([read, write])` when either is restricted.
        pub(super) fn property_security(&self) -> Result<Option<[Security; 2]>, GenerateError> {
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
}
