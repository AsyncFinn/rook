//! The reflection database for a new API dump, written as
//! `data/database.msgpack`.
//!
//! A field the dump states (a class's superclass and tags, a property's
//! type, tags, scriptability and whether it saves, the enums) follows the
//! new dump where it differs from the previous one. What rbx-dom's patches
//! made (aliases, properties saved under another name or migrated,
//! serialization-only properties the dump never lists) and the defaults
//! Studio saved carry over. New properties take the default the dump
//! prints, where it parses.

use std::collections::{HashMap, HashSet};
use std::fmt;

use rbx_reflection::{
    ClassDescriptor, DataType, EnumDescriptor, PropertyDescriptor, PropertyKind,
    PropertySerialization, PropertyTag, ReflectionDatabase, Scriptability,
};
use rbx_types::VariantType;
use serde::Serialize;

use super::{defaults, dump};
use crate::members::Security;

/// What changed, for reviewing an update.
#[derive(Debug, Default)]
pub struct Report {
    pub classes_added: Vec<String>,
    pub classes_removed: Vec<String>,
    pub properties_added: Vec<String>,
    pub properties_removed: Vec<String>,
    pub properties_changed: Vec<String>,
    pub enums_added: Vec<String>,
    pub enums_removed: Vec<String>,
    /// New properties given the default the dump prints.
    pub defaults_from_dump: usize,
}

impl fmt::Display for Report {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let lists = [
            ("classes added", &self.classes_added),
            ("classes removed", &self.classes_removed),
            ("properties added", &self.properties_added),
            ("properties removed", &self.properties_removed),
            ("properties changed", &self.properties_changed),
            ("enums added", &self.enums_added),
            ("enums removed", &self.enums_removed),
        ];
        for (what, list) in lists {
            writeln!(f, "{} {what}: {}", list.len(), list.join(", "))?;
        }
        write!(
            f,
            "{} of the new properties have the dump's default",
            self.defaults_from_dump
        )
    }
}

pub(super) fn update<'a>(
    previous: &ReflectionDatabase<'a>,
    old: &'a dump::Root,
    new: &'a dump::Root,
    version: [u32; 4],
    report: &mut Report,
) -> ReflectionDatabase<'a> {
    let old_classes: HashSet<&str> = old.classes.iter().map(|c| c.name.as_str()).collect();
    let old_properties = properties(old);
    let enums: HashMap<&str, &dump::Enum> =
        new.enums.iter().map(|e| (e.name.as_str(), e)).collect();

    let mut db = ReflectionDatabase::new();
    db.version = version;
    for class in &new.classes {
        let name = class.name.as_str();
        let mut desc = previous.classes.get(name).cloned().unwrap_or_else(|| {
            report.classes_added.push(name.to_owned());
            ClassDescriptor::new(name)
        });
        desc.superclass = (class.superclass != "<<<ROOT>>>").then_some(class.superclass.as_str());
        desc.tags = dump::string_tags(&class.tags)
            .filter_map(|t| t.parse().ok())
            .collect();

        let mut properties = HashMap::new();
        for member in class.members.iter().filter(|m| m.member_type == "Property") {
            let key = format!("{name}.{}", member.name);
            let Some(stated) = derive(member) else {
                continue;
            };
            let property = match desc.properties.get(member.name.as_str()) {
                Some(p) => match old_properties.get(&(name, member.name.as_str())) {
                    Some(old) if *old == member => p.clone(),
                    Some(old) => {
                        report.properties_changed.push(key);
                        restate(p.clone(), old, &stated)
                    }
                    None => p.clone(),
                },
                None => {
                    report.properties_added.push(key);
                    let default = member
                        .default
                        .as_deref()
                        .and_then(|text| defaults::parse(text, &stated.data_type, &enums));
                    if let Some(value) = default {
                        report.defaults_from_dump += 1;
                        desc.default_properties.insert(stated.name, value);
                    }
                    stated
                }
            };
            properties.insert(property.name, property);
        }
        for (prop, p) in &desc.properties {
            if properties.contains_key(prop) {
                continue;
            }
            if old_properties.contains_key(&(name, prop)) {
                report.properties_removed.push(format!("{name}.{prop}"));
            } else {
                properties.insert(prop, p.clone());
            }
        }
        desc.properties = properties;
        db.classes.insert(name, desc);
    }
    for (name, desc) in &previous.classes {
        if db.classes.contains_key(name) {
            continue;
        }
        if old_classes.contains(name) {
            report.classes_removed.push((*name).to_owned());
        } else {
            db.classes.insert(name, desc.clone());
        }
    }
    drop_stale_defaults(&mut db);

    let old_enums: HashSet<&str> = old.enums.iter().map(|e| e.name.as_str()).collect();
    for e in &new.enums {
        let mut desc = EnumDescriptor::new(e.name.as_str());
        desc.items = e.items.iter().map(|i| (i.name.as_str(), i.value)).collect();
        if !previous.enums.contains_key(e.name.as_str()) {
            report.enums_added.push(e.name.clone());
        }
        db.enums.insert(&e.name, desc);
    }
    for (name, desc) in &previous.enums {
        if db.enums.contains_key(name) {
            continue;
        }
        if old_enums.contains(name) {
            report.enums_removed.push((*name).to_owned());
        } else {
            db.enums.insert(name, desc.clone());
        }
    }
    for list in [
        &mut report.classes_added,
        &mut report.classes_removed,
        &mut report.properties_added,
        &mut report.properties_removed,
        &mut report.properties_changed,
        &mut report.enums_added,
        &mut report.enums_removed,
    ] {
        list.sort_unstable();
    }
    db
}

fn properties(dump: &dump::Root) -> HashMap<(&str, &str), &dump::Member> {
    dump.classes
        .iter()
        .flat_map(|c| {
            c.members
                .iter()
                .filter(|m| m.member_type == "Property")
                .map(move |m| ((c.name.as_str(), m.name.as_str()), m))
        })
        .collect()
}

/// Update the fields of `p` that the dump changed since `old`.
fn restate<'a>(
    mut p: PropertyDescriptor<'a>,
    old: &'a dump::Member,
    new: &PropertyDescriptor<'a>,
) -> PropertyDescriptor<'a> {
    let Some(was) = derive(old) else {
        return new.clone();
    };
    if !same(&was.data_type, &new.data_type) {
        p.data_type = new.data_type.clone();
    }
    if was.tags != new.tags {
        p.tags = new.tags.clone();
    }
    if !same(&was.scriptability, &new.scriptability) {
        p.scriptability = new.scriptability;
    }
    if !same(&was.kind, &new.kind) {
        p.kind = new.kind.clone();
    }
    p
}

fn same<T: Serialize>(a: &T, b: &T) -> bool {
    serde_json::to_value(a).ok() == serde_json::to_value(b).ok()
}

/// A property as the dump alone describes it (rbx-dom's rules), or `None`
/// for a type rbx-dom has no value type for.
fn derive(member: &dump::Member) -> Option<PropertyDescriptor<'_>> {
    let ty = member.value_type.as_ref()?;
    let data_type = match ty.category.as_str() {
        "Enum" => DataType::Enum(ty.name.as_str()),
        "Class" => DataType::Value(VariantType::Ref),
        _ => DataType::Value(variant_type(&ty.name)?),
    };
    let tags: HashSet<PropertyTag> = dump::string_tags(&member.tags)
        .filter_map(|t| t.parse().ok())
        .collect();
    let open = |s: Security| matches!(s, Security::None | Security::PluginSecurity);
    let [read, write] = member.property_security_pair().ok()?;
    let read = open(read);
    let write = open(write) && !tags.contains(&PropertyTag::ReadOnly);
    let scriptability = match (read, write) {
        _ if tags.contains(&PropertyTag::NotScriptable) => Scriptability::None,
        (true, true) => Scriptability::ReadWrite,
        (true, false) => Scriptability::Read,
        (false, true) => Scriptability::Write,
        (false, false) => Scriptability::None,
    };
    let saves = !tags.contains(&PropertyTag::ReadOnly)
        && member.serialization.as_ref().is_some_and(|s| s.can_save);
    let mut p = PropertyDescriptor::new(member.name.as_str(), data_type);
    p.scriptability = scriptability;
    p.tags = tags;
    p.kind = PropertyKind::Canonical {
        serialization: if saves {
            PropertySerialization::Serializes
        } else {
            PropertySerialization::DoesNotSerialize
        },
    };
    Some(p)
}

/// Defaults are listed per class, inherited ones included; keep those
/// whose property the class still has.
fn drop_stale_defaults(db: &mut ReflectionDatabase) {
    let names: Vec<&str> = db.classes.keys().copied().collect();
    for name in names {
        let mut known = HashSet::new();
        let mut cur = Some(name);
        while let Some(c) = cur.and_then(|c| db.classes.get(c)) {
            known.extend(c.properties.keys().copied());
            cur = c.superclass;
        }
        let desc = db.classes.get_mut(name).expect("listed class");
        desc.default_properties.retain(|p, _| known.contains(p));
    }
}

fn variant_type(name: &str) -> Option<VariantType> {
    Some(match name {
        "Axes" => VariantType::Axes,
        "BinaryString" => VariantType::BinaryString,
        "BrickColor" => VariantType::BrickColor,
        "CFrame" => VariantType::CFrame,
        "Color3" => VariantType::Color3,
        "Color3uint8" => VariantType::Color3uint8,
        "ColorSequence" => VariantType::ColorSequence,
        "Content" => VariantType::Content,
        "ContentId" => VariantType::ContentId,
        "Faces" => VariantType::Faces,
        "Font" => VariantType::Font,
        "Instance" => VariantType::Ref,
        "NetAssetRef" => VariantType::NetAssetRef,
        "NumberRange" => VariantType::NumberRange,
        "NumberSequence" => VariantType::NumberSequence,
        "OptionalCoordinateFrame" => VariantType::OptionalCFrame,
        "PhysicalProperties" => VariantType::PhysicalProperties,
        "Ray" => VariantType::Ray,
        "Rect" => VariantType::Rect,
        "Region3" => VariantType::Region3,
        "Region3int16" => VariantType::Region3int16,
        "SecurityCapabilities" => VariantType::SecurityCapabilities,
        "SharedString" => VariantType::SharedString,
        "UDim" => VariantType::UDim,
        "UDim2" => VariantType::UDim2,
        "UniqueId" => VariantType::UniqueId,
        "Vector2" => VariantType::Vector2,
        "Vector2int16" => VariantType::Vector2int16,
        "Vector3" => VariantType::Vector3,
        "Vector3int16" => VariantType::Vector3int16,
        "bool" => VariantType::Bool,
        "double" => VariantType::Float64,
        "float" => VariantType::Float32,
        "int" => VariantType::Int32,
        "int64" => VariantType::Int64,
        "string" | "ProtectedString" => VariantType::String,
        _ => return None,
    })
}
