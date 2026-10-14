//! The runtime instance tree.
//!
//! A [`Dom`] holds one DataModel: an arena of instances keyed by
//! generational [`InstanceId`]s, their hierarchy, property values,
//! attributes and tags, and a [`Journal`] of every change.
//!
//! The DOM itself never calls out while mutating. Signal dispatch and
//! engine systems read the journal afterwards, each with its own cursor.
//! See `docs/design/architecture.md` §5.
//!
//! Reference properties are indexed both ways: [`Dom::referrers`] lists
//! what points at an instance (the joints holding a part, for example).
//!
//! Hierarchy rules (locked parents, cycles, `Destroy`, `Clone`) follow
//! Roblox. Error texts are Inference until the conformance suite pins them.

mod attributes;
mod instance;
mod journal;
mod properties;
mod tree;

pub use instance::{Attributes, Instance};
pub use journal::{Change, Journal};
pub use properties::coerce;

use std::cell::RefCell;
use std::collections::HashMap;

use rustc_hash::FxHashMap;

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
    defaults: RefCell<FxHashMap<(ClassId, PropertyId), Value>>,
    /// For each instance, the reference properties pointing at it.
    referrers: FxHashMap<InstanceId, Vec<(InstanceId, PropertyId)>>,
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
            referrers: FxHashMap::default(),
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

#[cfg(test)]
mod tests;
