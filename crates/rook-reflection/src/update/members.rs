//! The member list for a new API dump, written as `data/members.json`.

use std::collections::BTreeMap;

use serde::Serialize;

use super::{UpdateError, dump};
use crate::members::{ClassMembers, Event, Function, Members};

impl Members {
    /// Build the list from a Full API dump.
    pub(super) fn from_api_dump(
        dump: &dump::Root,
        version: &str,
        source: &str,
    ) -> Result<Self, UpdateError> {
        let mut classes = BTreeMap::new();
        for class in &dump.classes {
            let mut out = ClassMembers::default();
            for member in &class.members {
                let tags = member.tags();
                let preferred = member.preferred();
                match member.member_type.as_str() {
                    "Property" => {
                        if let Some(sec) = member.property_security()? {
                            out.property_security.insert(member.name.clone(), sec);
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
                        name: member.name.clone(),
                        tags,
                        preferred,
                    }),
                    other => return Err(UpdateError::Shape(format!("member type {other}"))),
                }
            }
            classes.insert(class.name.clone(), out);
        }
        let mut enum_legacy_names: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();
        for e in &dump.enums {
            for item in &e.items {
                for legacy in &item.legacy_names {
                    enum_legacy_names
                        .entry(e.name.clone())
                        .or_default()
                        .insert(legacy.clone(), item.name.clone());
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
    pub(super) fn to_json(&self) -> String {
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

pub(super) fn json<T: Serialize + ?Sized>(v: &T) -> String {
    serde_json::to_string(v).expect("API data serializes")
}
