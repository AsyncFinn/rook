use super::*;

fn part(dom: &mut Dom, name: &str) -> InstanceId {
    let id = dom.create_by_name("Part").unwrap();
    dom.set_name(id, name.to_owned()).unwrap();
    id
}

#[test]
fn services_and_full_names() {
    let mut dom = Dom::new();
    let ws = dom.get_service("Workspace").unwrap();
    assert_eq!(dom.get_service("Workspace").unwrap(), ws);
    assert!(dom.get_service("Part").is_err());
    let p = part(&mut dom, "Brick");
    dom.set_parent(p, Some(ws)).unwrap();
    assert_eq!(dom.full_name(p), "Workspace.Brick");
    assert_eq!(dom.full_name(dom.root()), "Game");
    // Services can't be reparented.
    assert!(matches!(
        dom.set_parent(ws, None),
        Err(DomError::ParentLocked { .. })
    ));
}

#[test]
fn defaults_come_from_the_database() {
    let mut dom = Dom::new();
    let p = part(&mut dom, "P");
    let size = dom.property_by_name(p, "Size").unwrap();
    assert_eq!(
        size,
        Value::Vector3(rook_types::Vector3::new(4.0, 1.2, 2.0))
    );
    assert_eq!(
        dom.property_by_name(p, "Name"),
        Some(Value::String("P".into()))
    );
    // `size` is the legacy alias.
    assert_eq!(dom.property_by_name(p, "size"), Some(size));
}

#[test]
fn cycles_are_rejected() {
    let mut dom = Dom::new();
    let a = part(&mut dom, "A");
    let b = part(&mut dom, "B");
    dom.set_parent(b, Some(a)).unwrap();
    assert!(matches!(
        dom.set_parent(a, Some(a)),
        Err(DomError::OwnParent(_))
    ));
    let err = dom.set_parent(a, Some(b)).unwrap_err();
    assert_eq!(
        err.to_string(),
        "Attempt to set parent of A to A.B would result in circular reference"
    );
}

#[test]
fn destroy_locks_and_recurses() {
    let mut dom = Dom::new();
    let ws = dom.get_service("Workspace").unwrap();
    let a = part(&mut dom, "A");
    let b = part(&mut dom, "B");
    dom.set_parent(b, Some(a)).unwrap();
    dom.set_parent(a, Some(ws)).unwrap();
    dom.destroy(a).unwrap();
    assert_eq!(dom.parent(a), None);
    assert!(dom.is_destroyed(b));
    assert_eq!(dom.parent(b), None);
    let err = dom.set_parent(a, Some(ws)).unwrap_err();
    assert_eq!(
        err.to_string(),
        "The Parent property of A is locked, current parent: NULL, new parent Workspace"
    );
    // Destroyed instances still read.
    assert_eq!(dom.name(a), "A");
}

#[test]
fn clone_skips_non_archivable_and_remaps_refs() {
    let mut dom = Dom::new();
    let model = dom.create_by_name("Model").unwrap();
    let a = part(&mut dom, "A");
    let b = part(&mut dom, "B");
    dom.set_parent(a, Some(model)).unwrap();
    dom.set_parent(b, Some(model)).unwrap();
    let primary = dom.property_id(model, "PrimaryPart").unwrap();
    dom.set_property(model, primary, Value::Ref(Some(a.to_ref())))
        .unwrap();
    let archivable = dom.property_id(b, "Archivable").unwrap();
    dom.set_property(b, archivable, Value::Bool(false)).unwrap();

    let copy = dom.clone_instance(model).unwrap();
    assert_eq!(dom.parent(copy), None);
    let kids = dom.children(copy).to_vec();
    assert_eq!(kids.len(), 1);
    assert_eq!(
        dom.property(copy, primary),
        Some(Value::Ref(Some(kids[0].to_ref())))
    );
    assert_eq!(dom.clone_instance(b), None);
}

#[test]
fn journal_records_moves_with_context() {
    let mut dom = Dom::new();
    let ws = dom.get_service("Workspace").unwrap();
    let start = dom.journal().end();
    let a = part(&mut dom, "A");
    dom.set_parent(a, Some(ws)).unwrap();
    let moved = dom
        .journal()
        .since(start)
        .iter()
        .find_map(|c| match c {
            Change::Reparented {
                new_ancestors,
                subtree,
                ..
            } => Some((new_ancestors.clone(), subtree.clone())),
            _ => None,
        })
        .unwrap();
    assert_eq!(moved, (vec![ws, dom.root()], vec![a]));
    let end = dom.journal().end();
    dom.journal_mut().trim(end);
    assert!(dom.journal().since(end).is_empty());
}

#[test]
fn attributes_and_tags() {
    let mut dom = Dom::new();
    let a = part(&mut dom, "A");
    dom.set_attribute(a, "Speed", Some(Value::Float64(5.0)))
        .unwrap();
    assert_eq!(dom.attribute(a, "Speed"), Some(&Value::Float64(5.0)));
    dom.set_attribute(a, "Speed", None).unwrap();
    assert_eq!(dom.attribute(a, "Speed"), None);
    dom.add_tag(a, "Lava").unwrap();
    dom.add_tag(a, "Lava").unwrap();
    assert!(dom.has_tag(a, "Lava"));
    assert_eq!(dom.get(a).unwrap().tags.len(), 1);
}

#[test]
fn stale_ids_never_alias() {
    let mut dom = Dom::new();
    let a = part(&mut dom, "A");
    dom.remove(a);
    let b = part(&mut dom, "B");
    assert!(dom.get(a).is_none());
    assert_ne!(a, b);
    assert_eq!(InstanceId::from_ref(b.to_ref()), b);
}
