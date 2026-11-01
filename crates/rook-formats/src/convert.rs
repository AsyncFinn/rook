//! Conversion between `rbx_dom_weak` trees (files) and Rook's runtime [`Dom`].

use std::collections::{HashMap, HashSet};
use std::sync::LazyLock;

use rbx_dom_weak::{InstanceBuilder, UstrMap, WeakDom};
use rbx_types::{Ref, Variant};
use rook_dom::{Dom, InstanceId};
use rook_reflection::{ClassId, PropertyId};
use rook_types::Value;

/// What [`into_dom`] and [`copy_into_dom`] made.
#[derive(Debug)]
pub struct Converted {
    /// The new top-level instances.
    pub top: Vec<InstanceId>,
    /// Two instances were saved under one `UniqueId` (nil ones, which old
    /// files and new instances have, don't count).
    pub duplicate_unique_ids: bool,
}

/// Move the top-level instances of `weak` (its root's children) under
/// `parent` in `dom`, taking its property values rather than copying them:
/// for a file loaded once.
///
/// Loading isn't journaled. Top-level service instances placed under the
/// DataModel get their `Parent` locked, as Roblox's services do. If the
/// DataModel already has a service of that class, the loaded children are
/// merged into it.
pub fn into_dom(mut weak: WeakDom, dom: &mut Dom, parent: InstanceId) -> Converted {
    convert(&mut weak, dom, parent, |w, r| {
        std::mem::take(&mut w.get_by_ref_mut(r).expect("referent exists").properties)
    })
}

/// [`into_dom`], copying: for a tree put in many times (a rig, a patch).
pub fn copy_into_dom(weak: &WeakDom, dom: &mut Dom, parent: InstanceId) -> Converted {
    convert(weak, dom, parent, |w, r| {
        w.get_by_ref(r).expect("referent exists").properties.clone()
    })
}

fn convert<W: std::ops::Deref<Target = WeakDom>>(
    mut weak: W,
    dom: &mut Dom,
    parent: InstanceId,
    mut properties: impl FnMut(&mut W, Ref) -> UstrMap<Variant>,
) -> Converted {
    let was_enabled = dom.journal().is_enabled();
    dom.journal_mut().set_enabled(false);

    let mut map: HashMap<Ref, InstanceId> = HashMap::new();
    let mut pending_refs: Vec<(InstanceId, Ref)> = Vec::new();
    let mut top = Vec::new();
    for &child in weak.root().children() {
        top.push(create_rec(
            &weak,
            dom,
            child,
            parent,
            &mut map,
            &mut pending_refs,
        ));
    }

    // Second pass: properties, now that every referent has a Rook ID.
    let velocities = SavedVelocities::get();
    let mut unique_ids = HashSet::new();
    let mut duplicate_unique_ids = false;
    for (id, source) in pending_refs {
        let props = properties(&mut weak, source);
        let class = dom.get(id).expect("just created").class;
        let unknown_class = dom.get(id).expect("just created").unknown_class.is_some();
        let db = dom.database();
        text_size_from_font_size(dom, id, &props);
        for (name, variant) in props {
            let name = name.as_str();
            let map_ref = |r| map.get(&r).map(|id: &InstanceId| id.to_ref());
            match (name, variant) {
                ("Name", _) => {}
                ("Attributes", Variant::Attributes(attrs)) => {
                    for (key, v) in attrs {
                        if let Ok(value) = Value::from_variant(v, map_ref) {
                            let inst = dom.instance_mut(id).expect("exists");
                            inst.attributes.insert(key, value);
                        }
                    }
                }
                ("Tags", Variant::Tags(tags)) => {
                    dom.instance_mut(id).expect("exists").tags =
                        tags.iter().map(str::to_owned).collect();
                }
                (_, variant) => {
                    if let Variant::UniqueId(u) = &variant {
                        duplicate_unique_ids |= !u.is_nil() && !unique_ids.insert(*u);
                    }
                    let Ok(value) = Value::from_variant(variant, map_ref) else {
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
        velocities.adopt(dom, id);
    }

    dom.journal_mut().set_enabled(was_enabled);
    Converted {
        top,
        duplicate_unique_ids,
    }
}

/// Files save a part's velocities in the deprecated `Velocity` and
/// `RotVelocity`; physics and scripts use `AssemblyLinearVelocity` and
/// `AssemblyAngularVelocity`, which aren't saved. Scripts see the old two
/// as views of them (observed: setting either sets the assembly's velocity
/// at once).
struct SavedVelocities {
    base_part: ClassId,
    /// (saved, used) pairs.
    pairs: [(PropertyId, PropertyId); 2],
}

impl SavedVelocities {
    fn get() -> &'static Self {
        static IDS: LazyLock<SavedVelocities> = LazyLock::new(|| {
            let db = rook_reflection::database();
            let base_part = db.class_by_name("BasePart").expect("BasePart").id;
            let p = |name| db.find_canonical_property(base_part, name).expect(name).id;
            SavedVelocities {
                base_part,
                pairs: [
                    (p("Velocity"), p("AssemblyLinearVelocity")),
                    (p("RotVelocity"), p("AssemblyAngularVelocity")),
                ],
            }
        });
        &IDS
    }

    fn adopt(&self, dom: &mut Dom, id: InstanceId) {
        if !dom.is_a_class(id, self.base_part) {
            return;
        }
        for (saved, used) in self.pairs {
            if let Some(Value::Vector3(v)) = dom.property(id, saved)
                && v != rook_types::Vector3::default()
            {
                dom.load_property(id, used, Value::Vector3(v));
            }
        }
    }
}

/// Files from before `TextSize` (about 2016) save only the `FontSize`
/// enum, which Studio shows as that `TextSize` (Inference: the enum names
/// its size, and files since save both, agreeing).
fn text_size_from_font_size(dom: &mut Dom, id: InstanceId, properties: &UstrMap<Variant>) {
    let Some(Variant::Enum(e)) = properties.get(&"FontSize".into()) else {
        return;
    };
    if properties.contains_key(&"TextSize".into()) {
        return;
    }
    let db = dom.database();
    let size = db
        .enum_by_name("FontSize")
        .and_then(|info| info.item_name(e.to_u32()))
        .and_then(rook_types::text_size_of);
    let class = dom.get(id).expect("exists").class;
    if let (Some(size), Some(p)) = (size, db.find_canonical_property(class, "TextSize")) {
        dom.load_property(id, p.id, Value::Float32(size));
    }
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
        for (pid, value) in inst.properties() {
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
        let top = copy_into_dom(&loaded.dom, &mut dom, root).top;
        assert_eq!(top.len(), 1);
        let ws = dom.find_service("Workspace").unwrap();
        assert!(dom.is_parent_locked(ws));
        let brick = dom.find_first_child(ws, "Brick", false).unwrap();
        let link = dom.find_first_child(ws, "Link", false).unwrap();
        assert_eq!(dom.bool_property(brick, "Anchored"), Some(true));
        assert_eq!(dom.ref_property(link, "Value"), Some(brick));
        // Loading isn't replayed as changes.
        assert_eq!(dom.journal().end(), 0);

        let back = from_dom(&dom, &[ws]);
        let diffs = compare(&loaded.dom, &back);
        assert!(diffs.is_empty(), "{diffs:#?}");
    }

    #[test]
    fn saved_velocities_are_adopted() {
        let part = InstanceBuilder::new("Part")
            .with_property("Velocity", rbx_types::Vector3::new(50.0, 0.0, 0.0))
            .with_property("RotVelocity", rbx_types::Vector3::new(0.0, 3.0, 0.0));
        let weak = WeakDom::new(InstanceBuilder::new("DataModel").with_child(part));
        let mut dom = Dom::new();
        let root = dom.root();
        let part = into_dom(weak, &mut dom, root).top[0];
        let v = |name| dom.vector3_property(part, name).unwrap().to_array();
        assert_eq!(v("AssemblyLinearVelocity"), [50.0, 0.0, 0.0]);
        assert_eq!(v("AssemblyAngularVelocity"), [0.0, 3.0, 0.0]);
    }

    #[test]
    fn duplicate_unique_ids_are_noticed() {
        let id = rbx_types::UniqueId::new(1, 2, 3);
        // Set after inserting, as decoders do: `WeakDom` replaces a
        // duplicate given at insert.
        let place = |second| {
            let mut weak = WeakDom::new(
                InstanceBuilder::new("DataModel")
                    .with_child(InstanceBuilder::new("Folder"))
                    .with_child(InstanceBuilder::new("Folder")),
            );
            let folders = weak.root().children().to_vec();
            for (folder, unique_id) in folders.into_iter().zip([id, second]) {
                let inst = weak.get_by_ref_mut(folder).unwrap();
                inst.properties.insert("UniqueId".into(), unique_id.into());
            }
            weak
        };
        let duplicates = |weak| {
            let mut dom = Dom::new();
            let root = dom.root();
            into_dom(weak, &mut dom, root).duplicate_unique_ids
        };
        assert!(duplicates(place(id)));
        assert!(!duplicates(place(rbx_types::UniqueId::new(1, 2, 4))));
        assert!(!duplicates(place(rbx_types::UniqueId::nil())));
    }

    #[test]
    fn font_size_becomes_text_size() {
        let size24 = rbx_types::Enum::from_u32(7);
        let old = InstanceBuilder::new("TextLabel")
            .with_name("Old")
            .with_property("FontSize", size24);
        let both = InstanceBuilder::new("TextLabel")
            .with_name("Both")
            .with_property("FontSize", size24)
            .with_property("TextSize", 14.0f32);
        let weak = WeakDom::new(
            InstanceBuilder::new("DataModel")
                .with_child(old)
                .with_child(both),
        );
        let mut dom = Dom::new();
        let root = dom.root();
        into_dom(weak, &mut dom, root);
        let size = |name| {
            let id = dom.find_first_child(root, name, false).unwrap();
            dom.float_property(id, "TextSize")
        };
        assert_eq!(size("Old"), Some(24.0));
        assert_eq!(size("Both"), Some(14.0));
    }
}
