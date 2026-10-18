use super::*;

fn part(dom: &mut Dom, name: &str, parent: InstanceId) -> InstanceId {
    let p = dom.create_by_name("Part").unwrap();
    dom.set_name(p, name.into()).unwrap();
    dom.set_parent(p, Some(parent)).unwrap();
    p
}

fn setup() -> (Dom, Dom, Replicator) {
    let mut server = Dom::new();
    for s in ["Workspace", "ServerStorage", "ReplicatedStorage", "Players"] {
        server.get_service(s).unwrap();
    }
    (server, Dom::new(), Replicator::new())
}

#[test]
fn initial_copy_skips_server_only_containers() {
    let (mut server, mut client, mut r) = setup();
    let ws = server.find_service("Workspace").unwrap();
    let ss = server.find_service("ServerStorage").unwrap();
    let a = part(&mut server, "A", ws);
    part(&mut server, "Secret", ss);
    let v = server.create_by_name("ObjectValue").unwrap();
    server.set_parent(v, Some(ws)).unwrap();
    server
        .set_property_by_name(v, "Value", Value::Ref(Some(a.to_ref())))
        .unwrap();

    r.initial(&server, &mut client);
    let cws = client.find_service("Workspace").unwrap();
    let ca = client.find_first_child(cws, "A", false).unwrap();
    assert!(client.find_service("ServerStorage").is_none());
    let cv = client.find_first_child(cws, "ObjectValue", false).unwrap();
    assert_eq!(
        client.property_by_name(cv, "Value"),
        Some(Value::Ref(Some(ca.to_ref())))
    );
}

#[test]
fn changes_follow_the_server() {
    let (mut server, mut client, mut r) = setup();
    let ws = server.find_service("Workspace").unwrap();
    let ss = server.find_service("ServerStorage").unwrap();
    let rs = server.find_service("ReplicatedStorage").unwrap();
    r.initial(&server, &mut client);
    let cws = client.find_service("Workspace").unwrap();

    // A model built in ServerStorage arrives whole when moved to Workspace.
    let model = server.create_by_name("Model").unwrap();
    server.set_parent(model, Some(ss)).unwrap();
    part(&mut server, "Inner", model);
    r.sync(&server, &mut client);
    assert!(client.find_first_child(cws, "Model", false).is_none());
    server.set_parent(model, Some(ws)).unwrap();
    r.sync(&server, &mut client);
    let cm = client.find_first_child(cws, "Model", false).unwrap();
    assert!(client.find_first_child(cm, "Inner", false).is_some());

    // Property changes and renames.
    let inner = server.find_first_child(model, "Inner", false).unwrap();
    server
        .set_property_by_name(inner, "Anchored", Value::Bool(true))
        .unwrap();
    server.set_name(model, "Renamed".into()).unwrap();
    r.sync(&server, &mut client);
    let ci = client.find_first_child(cm, "Inner", false).unwrap();
    assert_eq!(
        client.property_by_name(ci, "Anchored"),
        Some(Value::Bool(true))
    );
    assert_eq!(client.name(cm), "Renamed");

    // Moving between replicated containers keeps the same client object.
    server.set_parent(model, Some(rs)).unwrap();
    r.sync(&server, &mut client);
    let crs = client.find_service("ReplicatedStorage").unwrap();
    assert_eq!(client.parent(cm), Some(crs));

    // Leaving the replicated tree removes it on the client.
    server.set_parent(model, Some(ss)).unwrap();
    r.sync(&server, &mut client);
    assert!(client.is_destroyed(cm));
    assert_eq!(r.client_id(model), None);
}

#[test]
fn properties_serialized_under_other_names_replicate() {
    let (mut server, mut client, mut r) = setup();
    let ws = server.find_service("Workspace").unwrap();
    let p = part(&mut server, "P", ws);
    let size = server.property_id(p, "Size").unwrap();
    let color = server.property_id(p, "Color").unwrap();
    server
        .set_property(
            p,
            size,
            Value::Vector3(rook_types::Vector3::new(9.0, 8.0, 7.0)),
        )
        .unwrap();
    server
        .set_property(
            p,
            color,
            Value::Color3(rook_types::Color3::new(1.0, 0.0, 0.0)),
        )
        .unwrap();
    r.initial(&server, &mut client);
    let cws = client.find_service("Workspace").unwrap();
    let cp = client.find_first_child(cws, "P", false).unwrap();
    assert_eq!(client.property(cp, size), server.property(p, size));
    assert_eq!(client.property(cp, color), server.property(p, color));
}

#[test]
fn server_script_source_is_withheld() {
    let (mut server, mut client, mut r) = setup();
    let ws = server.find_service("Workspace").unwrap();
    for class in ["Script", "LocalScript"] {
        let s = server.create_by_name(class).unwrap();
        server
            .set_property_by_name(s, "Source", Value::String("print(1)".into()))
            .unwrap();
        server.set_parent(s, Some(ws)).unwrap();
    }
    r.initial(&server, &mut client);
    let cws = client.find_service("Workspace").unwrap();
    let script = client.find_first_child(cws, "Script", false).unwrap();
    let local = client.find_first_child(cws, "LocalScript", false).unwrap();
    assert_eq!(
        client.property_by_name(script, "Source"),
        Some(Value::String(String::new()))
    );
    assert_eq!(
        client.property_by_name(local, "Source"),
        Some(Value::String("print(1)".into()))
    );
}

/// A reference set before its target becomes visible (as
/// `Player.Character` is set before the character enters Workspace)
/// points at the target once it arrives, in a later sync or the same one.
#[test]
fn references_wait_for_their_target() {
    let (mut server, mut client, mut r) = setup();
    let ws = server.find_service("Workspace").unwrap();
    let ss = server.find_service("ServerStorage").unwrap();
    r.initial(&server, &mut client);
    let v = server.create_by_name("ObjectValue").unwrap();
    server.set_parent(v, Some(ws)).unwrap();
    let target = part(&mut server, "Target", ss);
    server
        .set_property_by_name(v, "Value", Value::Ref(Some(target.to_ref())))
        .unwrap();
    r.sync(&server, &mut client);
    let cv = r.client_id(v).unwrap();
    assert_eq!(client.property_by_name(cv, "Value"), Some(Value::Ref(None)));
    server.set_parent(target, Some(ws)).unwrap();
    r.sync(&server, &mut client);
    let ct = r.client_id(target).unwrap();
    assert_eq!(
        client.property_by_name(cv, "Value"),
        Some(Value::Ref(Some(ct.to_ref())))
    );
}
