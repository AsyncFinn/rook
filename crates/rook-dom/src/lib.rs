//! The runtime instance tree.
//!
//! A [`Dom`] holds one DataModel: an arena of instances keyed by
//! generational [`InstanceId`]s, their hierarchy, property values,
//! attributes and tags, and a [`Journal`] of every change.
//!
//! The DOM itself never calls out while mutating. Signal dispatch and
//! engine systems read the journal afterwards, each with its own cursor.
//! See `docs/architecture.md` §5.
//!
//! Hierarchy rules (locked parents, cycles, `Destroy`, `Clone`) follow
//! Roblox. Error texts are Inference until the conformance suite pins them.

mod instance;
mod journal;

pub use instance::{Attributes, Instance};
pub use journal::{Change, Journal};

use std::cell::RefCell;
use std::collections::HashMap;

use rook_reflection::{ClassId, Database, PropertyId, PropertyType};
use rook_types::{InstanceRef, Value};
use slotmap::{Key, KeyData, SlotMap};

slotmap::new_key_type! {
    /// A handle to an instance. Handles of destroyed and removed instances
    /// never refer to a different instance later.
    pub struct InstanceId;
}

impl InstanceId {
    /// Encode as an [`InstanceRef`] for storage in a [`Value::Ref`].
    pub fn to_ref(self) -> InstanceRef {
        InstanceRef(self.data().as_ffi())
    }

    pub fn from_ref(r: InstanceRef) -> Self {
        KeyData::from_ffi(r.0).into()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DomError {
    #[error("The Parent property of {name} is locked, current parent: {current}, new parent {new}")]
    ParentLocked {
        name: String,
        current: String,
        new: String,
    },
    #[error("Attempt to set {0} as its own parent")]
    OwnParent(String),
    #[error("Attempt to set parent of {child} to {parent} would result in circular reference")]
    Circular { child: String, parent: String },
    #[error("'{0}' is not a valid Service name")]
    NotAService(String),
    #[error("Unable to create an Instance of type \"{0}\"")]
    UnknownClass(String),
    #[error("instance no longer exists")]
    Stale,
}

/// One DataModel's instance tree.
#[derive(Debug)]
pub struct Dom {
    db: &'static Database,
    instances: SlotMap<InstanceId, Instance>,
    root: InstanceId,
    journal: Journal,
    /// Default values converted from the reflection database, cached per class.
    defaults: RefCell<HashMap<(ClassId, PropertyId), Value>>,
    name_prop: PropertyId,
    parent_prop: PropertyId,
}

impl Dom {
    /// An empty DataModel named `Game`.
    pub fn new() -> Self {
        let db = rook_reflection::database();
        let data_model = db.class_by_name("DataModel").expect("DataModel class").id;
        let instance = db.class_by_name("Instance").expect("Instance class").id;
        let prop = |name| db.find_property(instance, name).expect(name).id;
        let mut instances = SlotMap::with_key();
        let mut root_inst = Instance::new(data_model, "Game".to_owned());
        root_inst.parent_locked = true;
        let root = instances.insert(root_inst);
        Self {
            db,
            instances,
            root,
            journal: Journal::default(),
            defaults: RefCell::default(),
            name_prop: prop("Name"),
            parent_prop: prop("Parent"),
        }
    }

    pub fn database(&self) -> &'static Database {
        self.db
    }

    /// The DataModel (`game`).
    pub fn root(&self) -> InstanceId {
        self.root
    }

    pub fn journal(&self) -> &Journal {
        &self.journal
    }

    pub fn journal_mut(&mut self) -> &mut Journal {
        &mut self.journal
    }

    pub fn get(&self, id: InstanceId) -> Option<&Instance> {
        self.instances.get(id)
    }

    fn inst(&self, id: InstanceId) -> Result<&Instance, DomError> {
        self.instances.get(id).ok_or(DomError::Stale)
    }

    fn inst_mut(&mut self, id: InstanceId) -> Result<&mut Instance, DomError> {
        self.instances.get_mut(id).ok_or(DomError::Stale)
    }

    pub fn len(&self) -> usize {
        self.instances.len()
    }

    pub fn is_empty(&self) -> bool {
        self.instances.is_empty()
    }

    // ---- creation -------------------------------------------------------

    /// Create an unparented instance of `class`, named after the class.
    /// Whether scripts may create the class is the caller's decision.
    pub fn create(&mut self, class: ClassId) -> InstanceId {
        let name = self.db.class(class).name.to_owned();
        let id = self.instances.insert(Instance::new(class, name));
        self.journal.push(Change::Created(id));
        id
    }

    /// Create an instance of a class Rook's database doesn't know (kept
    /// from files so they round-trip). It behaves as a plain `Instance`.
    pub fn create_unknown(&mut self, class_name: &str) -> InstanceId {
        let base = self
            .db
            .class_by_name("Instance")
            .expect("Instance class")
            .id;
        let mut inst = Instance::new(base, class_name.to_owned());
        inst.unknown_class = Some(class_name.into());
        let id = self.instances.insert(inst);
        self.journal.push(Change::Created(id));
        id
    }

    pub fn create_by_name(&mut self, class: &str) -> Result<InstanceId, DomError> {
        let class = self
            .db
            .class_by_name(class)
            .ok_or_else(|| DomError::UnknownClass(class.to_owned()))?;
        Ok(self.create(class.id))
    }

    // ---- names and classes ---------------------------------------------

    pub fn class_name(&self, id: InstanceId) -> &str {
        match self.instances.get(id) {
            Some(i) => i
                .unknown_class
                .as_deref()
                .unwrap_or(self.db.class(i.class).name),
            None => "<stale>",
        }
    }

    pub fn is_a(&self, id: InstanceId, class: &str) -> bool {
        let (Some(inst), Some(target)) = (self.instances.get(id), self.db.class_by_name(class))
        else {
            return false;
        };
        self.db.is_a(inst.class, target.id)
    }

    pub fn name(&self, id: InstanceId) -> &str {
        self.instances.get(id).map_or("<stale>", |i| &i.name)
    }

    pub fn set_name(&mut self, id: InstanceId, name: String) -> Result<(), DomError> {
        let inst = self.inst_mut(id)?;
        if inst.name != name {
            inst.name = name;
            let property = self.name_prop;
            self.journal.push(Change::Property { id, property });
        }
        Ok(())
    }

    /// `Instance:GetFullName()`: dot-separated names up to, but not
    /// including, the DataModel.
    pub fn full_name(&self, id: InstanceId) -> String {
        let mut parts = Vec::new();
        let mut cur = Some(id);
        while let Some(c) = cur {
            if c == self.root && c != id {
                break;
            }
            parts.push(self.name(c));
            cur = self.parent(c);
        }
        parts.reverse();
        parts.join(".")
    }

    // ---- hierarchy -----------------------------------------------------

    pub fn parent(&self, id: InstanceId) -> Option<InstanceId> {
        self.instances.get(id).and_then(|i| i.parent)
    }

    pub fn children(&self, id: InstanceId) -> &[InstanceId] {
        self.instances.get(id).map_or(&[], |i| &i.children)
    }

    /// Descendants in pre-order (the order `GetDescendants` returns).
    pub fn descendants(&self, id: InstanceId) -> Vec<InstanceId> {
        let mut out = Vec::new();
        self.collect_descendants(id, &mut out);
        out
    }

    fn collect_descendants(&self, id: InstanceId, out: &mut Vec<InstanceId>) {
        for &child in self.children(id) {
            out.push(child);
            self.collect_descendants(child, out);
        }
    }

    fn collect_post_order(&self, id: InstanceId, out: &mut Vec<InstanceId>) {
        for &child in self.children(id) {
            self.collect_post_order(child, out);
        }
        out.push(id);
    }

    /// Parent first, up to the topmost ancestor.
    pub fn ancestors(&self, id: InstanceId) -> impl Iterator<Item = InstanceId> + '_ {
        let mut cur = self.parent(id);
        std::iter::from_fn(move || {
            let c = cur?;
            cur = self.parent(c);
            Some(c)
        })
    }

    pub fn is_ancestor_of(&self, ancestor: InstanceId, descendant: InstanceId) -> bool {
        self.ancestors(descendant).any(|a| a == ancestor)
    }

    pub fn find_first_child(
        &self,
        id: InstanceId,
        name: &str,
        recursive: bool,
    ) -> Option<InstanceId> {
        if recursive {
            self.descendants(id)
                .into_iter()
                .find(|c| self.name(*c) == name)
        } else {
            self.children(id)
                .iter()
                .copied()
                .find(|c| self.name(*c) == name)
        }
    }

    /// `Instance:FindFirstChildOfClass()`.
    pub fn find_first_child_of_class(&self, id: InstanceId, class: &str) -> Option<InstanceId> {
        self.children(id)
            .iter()
            .copied()
            .find(|c| self.class_name(*c) == class)
    }

    pub fn is_parent_locked(&self, id: InstanceId) -> bool {
        self.instances.get(id).is_some_and(|i| i.parent_locked)
    }

    pub fn is_destroyed(&self, id: InstanceId) -> bool {
        self.instances.get(id).is_none_or(|i| i.destroyed)
    }

    fn display_parent(&self, p: Option<InstanceId>) -> String {
        p.map_or_else(|| "NULL".to_owned(), |p| self.full_name(p))
    }

    /// Set `Parent`, with Roblox's checks.
    pub fn set_parent(
        &mut self,
        id: InstanceId,
        parent: Option<InstanceId>,
    ) -> Result<(), DomError> {
        let current = self.inst(id)?.parent;
        if let Some(p) = parent {
            self.inst(p)?;
        }
        if self.is_parent_locked(id) {
            return Err(DomError::ParentLocked {
                name: self.full_name(id),
                current: self.display_parent(current),
                new: self.display_parent(parent),
            });
        }
        if current == parent {
            return Ok(());
        }
        if let Some(p) = parent {
            if p == id {
                return Err(DomError::OwnParent(self.full_name(id)));
            }
            if self.is_ancestor_of(id, p) {
                return Err(DomError::Circular {
                    child: self.full_name(id),
                    parent: self.full_name(p),
                });
            }
        }
        self.move_unchecked(id, parent);
        Ok(())
    }

    fn move_unchecked(&mut self, id: InstanceId, parent: Option<InstanceId>) {
        let old_parent = self.instances[id].parent;
        let journal = self.journal.is_enabled();
        let old_ancestors: Vec<_> = if journal {
            old_parent
                .into_iter()
                .chain(old_parent.into_iter().flat_map(|p| self.ancestors(p)))
                .collect()
        } else {
            Vec::new()
        };

        if let Some(op) = old_parent {
            let siblings = &mut self.instances[op].children;
            if let Some(pos) = siblings.iter().position(|c| *c == id) {
                siblings.remove(pos);
            }
        }
        self.instances[id].parent = parent;
        if let Some(np) = parent {
            self.instances[np].children.push(id);
        }

        if journal {
            let new_ancestors: Vec<_> = parent
                .into_iter()
                .chain(parent.into_iter().flat_map(|p| self.ancestors(p)))
                .collect();
            let mut subtree = vec![id];
            self.collect_descendants(id, &mut subtree);
            let mut subtree_post_order = Vec::with_capacity(subtree.len());
            self.collect_post_order(id, &mut subtree_post_order);
            self.journal.push(Change::Reparented {
                id,
                old_parent,
                new_parent: parent,
                old_ancestors,
                new_ancestors,
                subtree,
                subtree_post_order,
            });
            let property = self.parent_prop;
            self.journal.push(Change::Property { id, property });
        }
    }

    /// `Instance:Destroy()`: fire `Destroying`, set Parent to nil, lock it,
    /// and destroy every descendant.
    pub fn destroy(&mut self, id: InstanceId) -> Result<(), DomError> {
        if self.inst(id)?.destroyed {
            return Ok(());
        }
        if id == self.root {
            return Err(DomError::ParentLocked {
                name: self.full_name(id),
                current: "NULL".to_owned(),
                new: "NULL".to_owned(),
            });
        }
        self.journal.push(Change::Destroying(id));
        let inst = &mut self.instances[id];
        inst.destroyed = true;
        // A locked instance (a service, or one already destroyed) still
        // gets destroyed; only the parent assignment is skipped.
        if inst.parent.is_some() {
            self.move_unchecked(id, None);
        }
        self.instances[id].parent_locked = true;
        for child in self.children(id).to_vec() {
            self.destroy(child)?;
        }
        Ok(())
    }

    /// `Instance:ClearAllChildren()`: destroy every child.
    pub fn clear_all_children(&mut self, id: InstanceId) -> Result<(), DomError> {
        for child in self.inst(id)?.children.clone() {
            // Locked children (services) are skipped, as in Roblox.
            if !self.is_parent_locked(child) {
                self.destroy(child)?;
            }
        }
        Ok(())
    }

    /// Lock or unlock `Parent` (used for services and engine-owned objects).
    pub fn set_parent_locked(&mut self, id: InstanceId, locked: bool) {
        if let Some(i) = self.instances.get_mut(id) {
            i.parent_locked = locked;
        }
    }

    /// Remove an instance and its descendants from the arena entirely.
    /// Called by the engine once nothing references a destroyed instance.
    pub fn remove(&mut self, id: InstanceId) {
        if id == self.root {
            return;
        }
        if self.parent(id).is_some() {
            self.move_unchecked(id, None);
        }
        let mut doomed = vec![id];
        self.collect_descendants(id, &mut doomed);
        for d in doomed {
            self.instances.remove(d);
        }
    }

    /// `Instance:Clone()`. Returns `None` when `id` isn't archivable;
    /// non-archivable descendants are skipped. References to instances
    /// inside the cloned tree point at the copies.
    pub fn clone_instance(&mut self, id: InstanceId) -> Option<InstanceId> {
        if !self.archivable(id) {
            return None;
        }
        let mut map = HashMap::new();
        let new = self.clone_rec(id, None, &mut map);
        for &copy in map.values() {
            for value in self.instances[copy].properties.values_mut() {
                if let Value::Ref(Some(r)) = value
                    && let Some(mapped) = map.get(&InstanceId::from_ref(*r))
                {
                    *r = mapped.to_ref();
                }
            }
        }
        Some(new)
    }

    fn clone_rec(
        &mut self,
        id: InstanceId,
        parent: Option<InstanceId>,
        map: &mut HashMap<InstanceId, InstanceId>,
    ) -> InstanceId {
        let src = &self.instances[id];
        let mut copy = Instance::new(src.class, src.name.clone());
        copy.unknown_class = src.unknown_class.clone();
        copy.properties = src.properties.clone();
        copy.unknown_properties = src.unknown_properties.clone();
        copy.attributes = src.attributes.clone();
        copy.tags = src.tags.clone();
        copy.parent = parent;
        let children = src.children.clone();
        let new = self.instances.insert(copy);
        self.journal.push(Change::Created(new));
        map.insert(id, new);
        for child in children {
            if self.archivable(child) {
                let c = self.clone_rec(child, Some(new), map);
                self.instances[new].children.push(c);
            }
        }
        new
    }

    fn archivable(&self, id: InstanceId) -> bool {
        !matches!(
            self.property_by_name(id, "Archivable"),
            Some(Value::Bool(false))
        )
    }

    // ---- services --------------------------------------------------------

    /// `DataModel:FindService()`.
    pub fn find_service(&self, class: &str) -> Option<InstanceId> {
        self.find_first_child_of_class(self.root, class)
    }

    /// `DataModel:GetService()`: find the service, creating it if needed.
    pub fn get_service(&mut self, class: &str) -> Result<InstanceId, DomError> {
        if let Some(s) = self.find_service(class) {
            return Ok(s);
        }
        let info = self
            .db
            .class_by_name(class)
            .filter(|c| c.flags.service)
            .ok_or_else(|| DomError::NotAService(class.to_owned()))?;
        let id = self.create(info.id);
        self.move_unchecked(id, Some(self.root));
        self.instances[id].parent_locked = true;
        Ok(id)
    }

    // ---- properties ------------------------------------------------------

    /// Read a property. Unset properties read the class default.
    /// `Name` and `Parent` are stored on the instance itself.
    pub fn property(&self, id: InstanceId, property: PropertyId) -> Option<Value> {
        let inst = self.instances.get(id)?;
        if property == self.name_prop {
            return Some(Value::String(inst.name.clone()));
        }
        if property == self.parent_prop {
            return Some(Value::Ref(inst.parent.map(InstanceId::to_ref)));
        }
        if let Some(v) = inst.properties.get(&property) {
            return Some(v.clone());
        }
        let class = inst.class;
        self.default_value(class, property)
    }

    /// Like [`Dom::property`], looked up by name (aliases resolved).
    pub fn property_by_name(&self, id: InstanceId, name: &str) -> Option<Value> {
        let inst = self.instances.get(id)?;
        let prop = self.db.find_canonical_property(inst.class, name)?;
        self.property(id, prop.id)
    }

    /// The instance a reference property (`PrimaryPart`, `CurrentCamera`,
    /// ...) points at, if it's set.
    pub fn ref_property(&self, id: InstanceId, name: &str) -> Option<InstanceId> {
        match self.property_by_name(id, name)? {
            Value::Ref(Some(r)) => Some(InstanceId::from_ref(r)),
            _ => None,
        }
    }

    /// `Players.LocalPlayer` (set on clients only).
    pub fn local_player(&self) -> Option<InstanceId> {
        self.ref_property(self.find_service("Players")?, "LocalPlayer")
    }

    /// `Lighting.ClockTime`: Lighting's clock in hours, from `TimeOfDay`.
    pub fn clock_time(&self, lighting: InstanceId) -> f64 {
        match self.property_by_name(lighting, "TimeOfDay") {
            Some(Value::String(s)) => rook_types::clock_hours(&s),
            _ => 14.0,
        }
    }

    /// The id of the property `name` (aliases resolved) on `id`'s class.
    pub fn property_id(&self, id: InstanceId, name: &str) -> Option<PropertyId> {
        let inst = self.instances.get(id)?;
        Some(self.db.find_canonical_property(inst.class, name)?.id)
    }

    /// Like [`Dom::set_property`], looked up by name (aliases resolved). A
    /// name the class doesn't have is ignored.
    pub fn set_property_by_name(
        &mut self,
        id: InstanceId,
        name: &str,
        value: Value,
    ) -> Result<(), DomError> {
        match self.property_id(id, name) {
            Some(p) => self.set_property(id, p, value),
            None => Ok(()),
        }
    }

    fn default_value(&self, class: ClassId, property: PropertyId) -> Option<Value> {
        if let Some(v) = self.defaults.borrow().get(&(class, property)) {
            return Some(v.clone());
        }
        let v = convert_default(self.db, class, property)?;
        self.defaults
            .borrow_mut()
            .insert((class, property), v.clone());
        Some(v)
    }

    /// Write a stored property. `Name` and `Parent` must go through
    /// [`Dom::set_name`] and [`Dom::set_parent`]. Records a change only if
    /// the value differs.
    pub fn set_property(
        &mut self,
        id: InstanceId,
        property: PropertyId,
        value: Value,
    ) -> Result<(), DomError> {
        debug_assert!(property != self.name_prop && property != self.parent_prop);
        let value = coerce(self.db.property(property).ty, value);
        let current = self.property(id, property);
        if current.as_ref() == Some(&value) {
            return Ok(());
        }
        self.inst_mut(id)?.properties.insert(property, value);
        self.journal.push(Change::Property { id, property });
        Ok(())
    }

    /// Store a property from a file without recording a change.
    pub fn load_property(&mut self, id: InstanceId, property: PropertyId, value: Value) {
        let value = coerce(self.db.property(property).ty, value);
        if let Some(inst) = self.instances.get_mut(id) {
            inst.properties.insert(property, value);
        }
    }

    pub fn name_property(&self) -> PropertyId {
        self.name_prop
    }

    pub fn parent_property(&self) -> PropertyId {
        self.parent_prop
    }

    // ---- attributes and tags ---------------------------------------------

    pub fn attribute(&self, id: InstanceId, name: &str) -> Option<&Value> {
        self.instances.get(id)?.attributes.get(name)
    }

    pub fn set_attribute(
        &mut self,
        id: InstanceId,
        name: &str,
        value: Option<Value>,
    ) -> Result<(), DomError> {
        let attrs = &mut self.inst_mut(id)?.attributes;
        let changed = match value {
            Some(v) => attrs.insert(name.to_owned(), v),
            None => attrs.remove(name),
        };
        if changed {
            self.journal.push(Change::Attribute {
                id,
                name: name.to_owned(),
            });
        }
        Ok(())
    }

    pub fn has_tag(&self, id: InstanceId, tag: &str) -> bool {
        self.instances
            .get(id)
            .is_some_and(|i| i.tags.iter().any(|t| t == tag))
    }

    pub fn add_tag(&mut self, id: InstanceId, tag: &str) -> Result<(), DomError> {
        let tags = &mut self.inst_mut(id)?.tags;
        if !tags.iter().any(|t| t == tag) {
            tags.push(tag.to_owned());
            self.journal.push(Change::TagAdded {
                id,
                tag: tag.to_owned(),
            });
        }
        Ok(())
    }

    pub fn remove_tag(&mut self, id: InstanceId, tag: &str) -> Result<(), DomError> {
        let tags = &mut self.inst_mut(id)?.tags;
        if let Some(pos) = tags.iter().position(|t| t == tag) {
            tags.remove(pos);
            self.journal.push(Change::TagRemoved {
                id,
                tag: tag.to_owned(),
            });
        }
        Ok(())
    }

    /// Mutable access for loaders. Changes made through it are not journaled.
    pub fn instance_mut(&mut self, id: InstanceId) -> Option<&mut Instance> {
        self.instances.get_mut(id)
    }

    /// Attach `id` under `parent` without checks or journaling (loaders only).
    pub fn attach_unchecked(&mut self, id: InstanceId, parent: InstanceId) {
        let enabled = self.journal.is_enabled();
        self.journal.set_enabled(false);
        self.move_unchecked(id, Some(parent));
        self.journal.set_enabled(enabled);
    }
}

impl Default for Dom {
    fn default() -> Self {
        Self::new()
    }
}

/// The class default from the database, or the zero value of the
/// property's type when the database has none (Approximation: the
/// database lists defaults only for properties that serialize).
fn convert_default(db: &Database, class: ClassId, property: PropertyId) -> Option<Value> {
    let ty = db.property(property).ty;
    match db.default_value(class, property) {
        Some(variant) => Value::from_variant(variant.clone(), |_| None)
            .ok()
            .map(|v| coerce(ty, v)),
        None => zero_value(db, ty),
    }
}

/// Store values in the property's declared type. Files and the database
/// sometimes hold an equivalent encoding (`Color3uint8` for a `Color3`
/// property, `Int32` for an `Int64` one).
pub fn coerce(ty: PropertyType, value: Value) -> Value {
    use rbx_types::VariantType as T;
    let PropertyType::Value(target) = ty else {
        return value;
    };
    match (target, &value) {
        (T::Color3, Value::Color3uint8(c)) => Value::Color3((*c).into()),
        (T::Float32, v) if !matches!(v, Value::Float32(_)) => {
            v.as_number().map_or(value, |n| Value::Float32(n as f32))
        }
        (T::Float64, v) if !matches!(v, Value::Float64(_)) => {
            v.as_number().map_or(value, Value::Float64)
        }
        (T::Int32, v) if !matches!(v, Value::Int32(_)) => {
            v.as_number().map_or(value, |n| Value::Int32(n as i32))
        }
        (T::Int64, v) if !matches!(v, Value::Int64(_)) => {
            v.as_number().map_or(value, |n| Value::Int64(n as i64))
        }
        _ => value,
    }
}

fn zero_value(db: &Database, ty: PropertyType) -> Option<Value> {
    use rbx_types::VariantType as T;
    use rook_types::{CFrame, Color3, UDim, UDim2, Vector2, Vector3};
    Some(match ty {
        PropertyType::Enum(e) => Value::Enum(db.enum_info(e).items.first().map_or(0, |(_, v)| *v)),
        PropertyType::Value(t) => match t {
            T::Bool => Value::Bool(false),
            T::Float32 => Value::Float32(0.0),
            T::Float64 => Value::Float64(0.0),
            T::Int32 => Value::Int32(0),
            T::Int64 => Value::Int64(0),
            T::String => Value::String(String::new()),
            T::ContentId => Value::ContentId(String::new().into()),
            T::Vector3 => Value::Vector3(Vector3::ZERO),
            T::Vector2 => Value::Vector2(Vector2::ZERO),
            T::CFrame => Value::CFrame(CFrame::IDENTITY),
            T::Color3 => Value::Color3(Color3::new(0.0, 0.0, 0.0)),
            T::UDim => Value::UDim(UDim::default()),
            T::UDim2 => Value::UDim2(UDim2::default()),
            T::Ref => Value::Ref(None),
            T::BrickColor => Value::BrickColor(rbx_types::BrickColor::from_number(194)?),
            T::NumberRange => Value::NumberRange(rbx_types::NumberRange::new(0.0, 0.0)),
            _ => return None,
        },
    })
}

#[cfg(test)]
mod tests;
