//! Conversion between `rbx_dom_weak` trees (files) and Rook's runtime [`Dom`].

use std::collections::HashMap;

use rbx_dom_weak::{InstanceBuilder, WeakDom};
use rbx_types::{Ref, Variant};
use rook_dom::{Dom, InstanceId};
use rook_types::Value;

/// Copy the top-level instances of `weak` (its root's children) under
/// `parent` in `dom`. Returns the new top-level instances.
///
/// Loading isn't journaled. Top-level service instances placed under the
/// DataModel get their `Parent` locked, as Roblox's services do. If the
/// DataModel already has a service of that class, the loaded children are
/// merged into it.
pub fn into_dom(weak: &WeakDom, dom: &mut Dom, parent: InstanceId) -> Vec<InstanceId> {
    let was_enabled = dom.journal().is_enabled();
    dom.journal_mut().set_enabled(false);

    let mut map: HashMap<Ref, InstanceId> = HashMap::new();
    let mut pending_refs: Vec<(InstanceId, Ref)> = Vec::new();
    let mut top = Vec::new();
    for &child in weak.root().children() {
        top.push(create_rec(
            weak,
            dom,
            child,
            parent,
            &mut map,
            &mut pending_refs,
        ));
    }

    // Second pass: properties, now that every referent has a Rook ID.
    for (id, source) in pending_refs {
        let inst = weak.get_by_ref(source).expect("referent exists");
        let class = dom.get(id).expect("just created").class;
        let unknown_class = dom.get(id).expect("just created").unknown_class.is_some();
        let db = dom.database();
        for (name, variant) in &inst.properties {
            let name = name.as_str();
            let map_ref = |r| map.get(&r).map(|id: &InstanceId| id.to_ref());
            match (name, variant) {
                ("Name", _) => {}
                ("Attributes", Variant::Attributes(attrs)) => {
                    for (key, v) in attrs.iter() {
                        if let Ok(value) = Value::from_variant(v.clone(), map_ref) {
                            let inst = dom.instance_mut(id).expect("exists");
                            inst.attributes.insert(key.clone(), value);
                        }
                    }
                }
                ("Tags", Variant::Tags(tags)) => {
                    dom.instance_mut(id).expect("exists").tags =
                        tags.iter().map(str::to_owned).collect();
                }
                _ => {
                    let Ok(value) = Value::from_variant(variant.clone(), map_ref) else {
                        continue;
                    };
                    let prop = (!unknown_class)
                        .then(|| db.find_canonical_property(class, name))
                        .flatten();
                    match prop {
                        Some(p) => dom.load_property(id, p.id, value),
                        None => dom
                            .instance_mut(id)
                            .expect("exists")
                            .unknown_properties
                            .push((name.to_owned(), value)),
                    }
                }
            }
        }
    }

    dom.journal_mut().set_enabled(was_enabled);
    top
}

fn create_rec(
    weak: &WeakDom,
    dom: &mut Dom,
    source: Ref,
    parent: InstanceId,
    map: &mut HashMap<Ref, InstanceId>,
    pending: &mut Vec<(InstanceId, Ref)>,
) -> InstanceId {
    let inst = weak.get_by_ref(source).expect("referent exists");
    let class = inst.class.as_str();
    let db = dom.database();
    let is_service = db.class_by_name(class).is_some_and(|c| c.flags.service);

    let id = if is_service && parent == dom.root() {
        match dom.find_service(class) {
            Some(existing) => existing,
            None => {
                let id = dom.get_service(class).expect("service class");
                dom.set_name(id, inst.name.clone()).expect("exists");
                id
            }
        }
    } else {
        let id = match db.class_by_name(class) {
            Some(c) => dom.create(c.id),
            None => dom.create_unknown(class),
        };
        dom.set_name(id, inst.name.clone()).expect("exists");
        dom.attach_unchecked(id, parent);
        id
    };
    map.insert(source, id);
    pending.push((id, source));
    for &child in inst.children() {
        create_rec(weak, dom, child, id, map, pending);
    }
    id
}

/// Build a `WeakDom` whose root's children are copies of `roots`.
pub fn from_dom(dom: &Dom, roots: &[InstanceId]) -> WeakDom {
    let mut weak = WeakDom::new(InstanceBuilder::new("DataModel"));
    let mut map: HashMap<InstanceId, Ref> = HashMap::new();
    let mut order = Vec::new();
    let weak_root = weak.root_ref();
    for &root in roots {
        build_rec(dom, &mut weak, root, weak_root, &mut map, &mut order);
    }
    for (id, target) in order {
        let inst = dom.get(id).expect("exists");
        let db = dom.database();
        let to_ref = |r| {
            map.get(&InstanceId::from_ref(r))
                .copied()
                .unwrap_or_else(Ref::none)
        };
        let mut props: Vec<(String, Variant)> = Vec::new();
        for (pid, value) in &inst.properties {
            let value = file_encoding(db, inst.class, *pid, value);
            props.push((db.property(*pid).name.to_owned(), value.to_variant(to_ref)));
        }
        for (name, value) in &inst.unknown_properties {
            props.push((name.clone(), value.to_variant(to_ref)));
        }
        if !inst.attributes.is_empty() {
            let mut attrs = rbx_types::Attributes::new();
            for (k, v) in inst.attributes.iter() {
                attrs.insert(k.to_owned(), v.to_variant(to_ref));
            }
            props.push(("Attributes".to_owned(), Variant::Attributes(attrs)));
        }
        if !inst.tags.is_empty() {
            let mut tags = rbx_types::Tags::new();
            for t in &inst.tags {
                tags.push(t);
            }
            props.push(("Tags".to_owned(), Variant::Tags(tags)));
        }
        let w = weak.get_by_ref_mut(target).expect("just built");
        for (k, v) in props {
            w.properties.insert(k.as_str().into(), v);
        }
    }
    weak
}

/// The DOM stores values in the property's scripting type; files may use
/// another encoding (`Color` is saved as `Color3uint8`).
fn file_encoding(
    db: &rook_reflection::Database,
    class: rook_reflection::ClassId,
    pid: rook_reflection::PropertyId,
    value: &Value,
) -> Value {
    use rook_reflection::{PropertyKind, PropertyType, Serialization};
    let PropertyKind::Canonical(Serialization::SerializesAs(name)) = &db.property(pid).kind else {
        return value.clone();
    };
    let target = db.find_property(class, name).map(|p| p.ty);
    match (target, value) {
        (Some(PropertyType::Value(rbx_types::VariantType::Color3uint8)), Value::Color3(c)) => {
            Value::Color3uint8((*c).into())
        }
        _ => value.clone(),
    }
}

fn build_rec(
    dom: &Dom,
    weak: &mut WeakDom,
    id: InstanceId,
    parent: Ref,
    map: &mut HashMap<InstanceId, Ref>,
    order: &mut Vec<(InstanceId, Ref)>,
) {
    let builder = InstanceBuilder::new(dom.class_name(id)).with_name(dom.name(id));
    let r = weak.insert(parent, builder);
    map.insert(id, r);
    order.push((id, r));
    for &child in dom.children(id) {
        build_rec(dom, weak, child, r, map, order);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Format, compare, load, save};

    #[test]
    fn file_to_dom_and_back() {
        let part = InstanceBuilder::new("Part")
            .with_name("Brick")
            .with_property("Anchored", true)
            .with_property("Size", rbx_types::Vector3::new(1.0, 2.0, 3.0));
        let part_ref = part.referent();
        let value = InstanceBuilder::new("ObjectValue")
            .with_name("Link")
            .with_property("Value", part_ref);
        let ws = InstanceBuilder::new("Workspace")
            .with_child(part)
            .with_child(value);
        let weak = WeakDom::new(InstanceBuilder::new("DataModel").with_child(ws));
        let bytes = save(&weak, Format::Binary).unwrap();
        let loaded = load(&bytes).unwrap();

        let mut dom = Dom::new();
        let root = dom.root();
        let top = into_dom(&loaded.dom, &mut dom, root);
        assert_eq!(top.len(), 1);
        let ws = dom.find_service("Workspace").unwrap();
        assert!(dom.is_parent_locked(ws));
        let brick = dom.find_first_child(ws, "Brick", false).unwrap();
        let link = dom.find_first_child(ws, "Link", false).unwrap();
        assert_eq!(
            dom.property_by_name(brick, "Anchored"),
            Some(Value::Bool(true))
        );
        assert_eq!(
            dom.property_by_name(link, "Value"),
            Some(Value::Ref(Some(brick.to_ref())))
        );
        // Loading isn't replayed as changes.
        assert_eq!(dom.journal().end(), 0);

        let back = from_dom(&dom, &[ws]);
        let diffs = compare(&loaded.dom, &back);
        assert!(diffs.is_empty(), "{diffs:#?}");
    }
}
