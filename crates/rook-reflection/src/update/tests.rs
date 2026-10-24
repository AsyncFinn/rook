use rbx_reflection::{PropertyKind, PropertySerialization, PropertyTag, ReflectionDatabase};
use rbx_types::Variant;

use super::*;
use crate::members::{Security, Tag, TypeKind};

const DUMP: &str = r#"{"Classes": [{"Name": "Instance", "Superclass": "<<<ROOT>>>", "Members": [
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
fn members_generate_and_round_trip() {
    let empty = encode(r#"{"Version": [0, 0, 0, 0], "Classes": {}, "Enums": {}}"#);
    let data = update(&empty, r#"{"Classes": []}"#, DUMP, "0.1.0.1", "unit test").unwrap();
    let back: Members = serde_json::from_str(&data.members).unwrap();
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

const PREVIOUS: &str = r#"{"Version": [0, 1, 0, 0], "Classes": {
"Part": {"Name": "Part", "Tags": [], "Superclass": null, "Properties": {
  "Size": {"Name": "Size", "Scriptability": "ReadWrite", "DataType": {"Value": "Vector3"}, "Tags": [],
           "Kind": {"Canonical": {"Serialization": {"SerializesAs": "size"}}}},
  "Gone": {"Name": "Gone", "Scriptability": "ReadWrite", "DataType": {"Value": "Bool"}, "Tags": [],
           "Kind": {"Canonical": {"Serialization": "Serializes"}}},
  "Legacy": {"Name": "Legacy", "Scriptability": "None", "DataType": {"Value": "Bool"}, "Tags": [],
             "Kind": {"Canonical": {"Serialization": "Serializes"}}}
 }, "DefaultProperties": {"Gone": {"Bool": true}}}
}, "Enums": {}}"#;

const OLD: &str = r#"{"Classes": [{"Name": "Part", "Superclass": "<<<ROOT>>>", "Members": [
    {"MemberType": "Property", "Name": "Size", "Security": {"Read": "None", "Write": "None"},
     "Serialization": {"CanLoad": true, "CanSave": true},
     "ValueType": {"Category": "DataType", "Name": "Vector3"}},
    {"MemberType": "Property", "Name": "Gone", "Security": {"Read": "None", "Write": "None"},
     "Serialization": {"CanLoad": true, "CanSave": true},
     "ValueType": {"Category": "Primitive", "Name": "bool"}}
]}]}"#;

const NEW: &str = r#"{"Classes": [{"Name": "Part", "Superclass": "<<<ROOT>>>", "Members": [
    {"MemberType": "Property", "Name": "Size", "Security": {"Read": "None", "Write": "None"},
     "Serialization": {"CanLoad": true, "CanSave": true}, "Tags": ["Deprecated"],
     "ValueType": {"Category": "DataType", "Name": "Vector3"}},
    {"MemberType": "Property", "Name": "Fresh", "Security": {"Read": "None", "Write": "None"},
     "Serialization": {"CanLoad": true, "CanSave": true}, "Default": "0.5",
     "ValueType": {"Category": "Primitive", "Name": "float"}},
    {"MemberType": "Property", "Name": "Mode", "Security": {"Read": "None", "Write": "None"},
     "Serialization": {"CanLoad": true, "CanSave": true}, "Default": "B",
     "ValueType": {"Category": "Enum", "Name": "PartMode"}}
]}], "Enums": [{"Name": "PartMode", "Items": [{"Name": "A", "Value": 0}, {"Name": "B", "Value": 1}]}]}"#;

#[test]
fn database_follows_the_dump_and_keeps_patches() {
    let data = update(&encode(PREVIOUS), OLD, NEW, "0.2.0.7", "unit test").unwrap();
    let db: ReflectionDatabase = rmp_serde::from_slice(&data.database).unwrap();
    assert_eq!(db.version, [0, 2, 0, 7]);
    let part = &db.classes["Part"];

    // A changed tag follows the dump; the patched serialization stays.
    let size = &part.properties["Size"];
    assert!(size.tags.contains(&PropertyTag::Deprecated));
    assert!(matches!(
        size.kind,
        PropertyKind::Canonical {
            serialization: PropertySerialization::SerializesAs("size")
        }
    ));

    // Removed from the dump: gone, with its default. Never in a dump: kept.
    assert!(!part.properties.contains_key("Gone"));
    assert!(!part.default_properties.contains_key("Gone"));
    assert!(part.properties.contains_key("Legacy"));

    assert_eq!(part.default_properties["Fresh"], Variant::Float32(0.5));
    assert!(matches!(part.default_properties["Mode"], Variant::Enum(e) if e.to_u32() == 1));
    assert_eq!(db.enums["PartMode"].items["B"], 1);

    let r = &data.report;
    assert_eq!(r.properties_added, ["Part.Fresh", "Part.Mode"]);
    assert_eq!(r.properties_removed, ["Part.Gone"]);
    assert_eq!(r.properties_changed, ["Part.Size"]);
    assert_eq!(r.enums_added, ["PartMode"]);
    assert_eq!(r.defaults_from_dump, 2);
}

/// A database written as JSON, encoded as the committed file is.
fn encode(json: &str) -> Vec<u8> {
    let db: ReflectionDatabase = serde_json::from_str(json).unwrap();
    rmp_serde::to_vec(&db).unwrap()
}
