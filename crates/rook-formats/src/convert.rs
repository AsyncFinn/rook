use rustc_hash::{FxHashMap, FxHashSet};
use std::sync::{Arc, LazyLock};

use crate::Loaded;

use rbx_dom_weak::{InstanceBuilder, UstrMap, WeakDom};
use rbx_types::{Ref, Variant};
use rook_dom::{Dom, InstanceId};
use rook_reflection::{ClassId, PropertyId};
use rook_types::Value;

/// What [`into_dom`] and [`copy_into_dom`] made.
#[derive(Debug)]
pub struct Converted {
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
pub fn into_dom(weak: WeakDom, dom: &mut Dom, parent: InstanceId) -> Converted {
    Placing::new(weak, parent).place_all(dom)
}

/// [`into_dom`], copying: for a tree put in many times (a rig, a patch).
pub fn copy_into_dom(weak: &WeakDom, dom: &mut Dom, parent: InstanceId) -> Converted {
    Placing::new(weak, parent).place_all(dom)
}

/// A tree [`Placing`] puts in: one loaded once, whose values it takes, or
/// one put in many times, whose values it copies.
pub trait Tree {
    fn weak(&self) -> &WeakDom;
    /// The properties of the instance `r`, taken or copied.
    fn properties(&mut self, r: Ref) -> UstrMap<Variant>;
}

impl Tree for WeakDom {
    fn weak(&self) -> &WeakDom {
        self
    }
    fn properties(&mut self, r: Ref) -> UstrMap<Variant> {
        std::mem::take(&mut self.get_by_ref_mut(r).expect("referent exists").properties)
    }
}

impl Tree for &WeakDom {
    fn weak(&self) -> &WeakDom {
        self
    }
    fn properties(&mut self, r: Ref) -> UstrMap<Variant> {
        self.get_by_ref(r)
            .expect("referent exists")
            .properties
            .clone()
    }
}

/// A file read on a loader's thread: its values are taken unless another
/// holder shares it.
impl Tree for Arc<Loaded> {
    fn weak(&self) -> &WeakDom {
        &self.dom
    }
    fn properties(&mut self, r: Ref) -> UstrMap<Variant> {
        match Arc::get_mut(self) {
            Some(loaded) => loaded.dom.properties(r),
            None => self
                .dom
                .get_by_ref(r)
                .expect("referent exists")
                .properties
                .clone(),
        }
    }
}

/// A file's top-level instances going under `parent` in a Dom a slice at a
/// time, as Unity's `InstantiateAsync` spreads a large instantiation over
/// frames: each [`Placing::place`] does a bounded amount of work, so a
/// large model a script loads doesn't stall one frame. Instances go under
/// `parent` as they're made, so until it's done `parent` must be out of
/// every script's reach (no parent, no handle). [`into_dom`] and
/// [`copy_into_dom`] place everything at once.
#[derive(Debug)]
pub struct Placing<T> {
    tree: T,
    /// Instances still to make, next last, each with where it goes.
    making: Vec<(Ref, InstanceId)>,
    /// Each instance made, by referent.
    map: FxHashMap<Ref, InstanceId>,
    /// The instances made, in order; those before `filled` have their
    /// properties (set once every instance is made, so references find
    /// theirs).
    made: Vec<(InstanceId, Ref)>,
    filled: usize,
    unique_ids: FxHashSet<rbx_types::UniqueId>,
    duplicate_unique_ids: bool,
}

impl<T: Tree> Placing<T> {
    pub fn new(tree: T, parent: InstanceId) -> Self {
        let making = (tree.weak().root().children().iter().rev())
            .map(|&child| (child, parent))
            .collect();
        Self {
            tree,
            making,
            map: FxHashMap::default(),
            made: Vec::new(),
            filled: 0,
            unique_ids: FxHashSet::default(),
            duplicate_unique_ids: false,
        }
    }

    /// Make instances, then set their properties, taking one from
    /// `budget` for each instance and property done, until it runs out;
    /// what was made, once everything is.
    pub fn place(&mut self, dom: &mut Dom, budget: &mut usize) -> Option<Converted> {
        let was_enabled = dom.journal().is_enabled();
        dom.journal_mut().set_enabled(false);
        while *budget > 0
            && let Some((source, parent)) = self.making.pop()
        {
            self.make(dom, source, parent);
            *budget -= 1;
        }
        let velocities = SavedVelocities::get();
        while *budget > 0 && self.making.is_empty() && self.filled < self.made.len() {
            let (id, source) = self.made[self.filled];
            self.filled += 1;
            let done = 1 + self.fill(dom, id, source);
            *budget = budget.saturating_sub(done);
            velocities.adopt(dom, id);
        }
        dom.journal_mut().set_enabled(was_enabled);
        if !self.making.is_empty() || self.filled < self.made.len() {
            return None;
        }
        let top = (self.tree.weak().root().children().iter())
            .map(|r| self.map[r])
            .collect();
        Some(Converted {
            top,
            duplicate_unique_ids: self.duplicate_unique_ids,
        })
    }

    /// Place everything now.
    fn place_all(mut self, dom: &mut Dom) -> Converted {
        let mut unlimited = usize::MAX;
        self.place(dom, &mut unlimited)
            .expect("placed without a limit")
    }

    fn make(&mut self, dom: &mut Dom, source: Ref, parent: InstanceId) {
        let inst = self
            .tree
            .weak()
            .get_by_ref(source)
            .expect("referent exists");
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
        self.map.insert(source, id);
        self.made.push((id, source));
        (self.making).extend(inst.children().iter().rev().map(|&child| (child, id)));
    }

    /// Set `id`'s properties from `source`; how many there were.
    fn fill(&mut self, dom: &mut Dom, id: InstanceId, source: Ref) -> usize {
        let props = self.tree.properties(source);
        let count = props.len();
        let class = dom.get(id).expect("just created").class;
        let unknown_class = dom.get(id).expect("just created").unknown_class.is_some();
        let db = dom.database();
        let map = &self.map;
        let map_ref = |r| map.get(&r).map(|id: &InstanceId| id.to_ref());
        text_size_from_font_size(dom, id, &props);
        for (name, variant) in props {
            let name = name.as_str();
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
                    let tags = tags.iter().map(str::to_owned).collect();
                    dom.set_tags(id, tags).expect("exists");
                }
                (_, variant) => {
                    if let Variant::UniqueId(u) = &variant {
                        self.duplicate_unique_ids |= !u.is_nil() && !self.unique_ids.insert(*u);
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
        count
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

pub fn from_dom(dom: &Dom, roots: &[InstanceId]) -> WeakDom {
    let mut weak = WeakDom::new(InstanceBuilder::new("DataModel"));
    let mut map: FxHashMap<InstanceId, Ref> = FxHashMap::default();
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
        let voxels = dom.voxel_property(id);
        let stored = inst.properties().iter().map(|(p, v)| (*p, v));
        for (pid, value) in stored.chain(voxels.as_ref().map(|(p, v)| (*p, v))) {
            let value = file_encoding(db, inst.class, pid, value);
            props.push((db.property(pid).name.to_owned(), value.to_variant(to_ref)));
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
        if !inst.tags().is_empty() {
            let mut tags = rbx_types::Tags::new();
            for t in inst.tags() {
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
    map: &mut FxHashMap<InstanceId, Ref>,
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

    /// Placed a slice at a time, a tree comes out as placed at once, its
    /// references found across slices, each slice doing no more than its
    /// budget.
    #[test]
    fn placing_in_slices_matches_placing_at_once() {
        let first = InstanceBuilder::new("Part").with_name("First");
        let first_ref = first.referent();
        let mut kit = InstanceBuilder::new("Model")
            .with_name("Kit")
            .with_child(first);
        for i in 0..30 {
            let part = InstanceBuilder::new("Part")
                .with_name(format!("Part{i}"))
                .with_property("Anchored", true);
            kit = kit.with_child(part);
        }
        let link = InstanceBuilder::new("ObjectValue")
            .with_name("Link")
            .with_property("Value", first_ref);
        let weak = WeakDom::new(InstanceBuilder::new("DataModel").with_child(kit.with_child(link)));

        let mut at_once = Dom::new();
        let whole = at_once.create_by_name("Model").unwrap();
        copy_into_dom(&weak, &mut at_once, whole);

        let mut sliced = Dom::new();
        let model = sliced.create_by_name("Model").unwrap();
        let mut placing = Placing::new(&weak, model);
        let mut slices = 1;
        let placed = loop {
            let mut budget = 10;
            if let Some(placed) = placing.place(&mut sliced, &mut budget) {
                break placed;
            }
            assert_eq!(budget, 0);
            slices += 1;
        };
        assert!(slices > 3, "{slices} slices");
        assert_eq!(placed.top.len(), 1);
        let kit = sliced.find_first_child(model, "Kit", false).unwrap();
        let link = sliced.find_first_child(kit, "Link", false).unwrap();
        let first = sliced.find_first_child(kit, "First", false).unwrap();
        assert_eq!(sliced.ref_property(link, "Value"), Some(first));
        let diffs = compare(&from_dom(&at_once, &[whole]), &from_dom(&sliced, &[model]));
        assert!(diffs.is_empty(), "{diffs:#?}");
    }

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
