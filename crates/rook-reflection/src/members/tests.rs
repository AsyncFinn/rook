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
