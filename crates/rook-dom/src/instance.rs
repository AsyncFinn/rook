use std::sync::atomic::{AtomicU64, Ordering};

use rook_reflection::{ClassId, PropertyId};
use rook_types::Value;

use crate::InstanceId;

/// Attributes in insertion order, as `GetAttributes` reports them.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Attributes(Vec<(String, Value)>);

impl Attributes {
    pub fn get(&self, name: &str) -> Option<&Value> {
        self.0.iter().find(|(k, _)| k == name).map(|(_, v)| v)
    }

    /// Returns whether anything changed.
    pub fn insert(&mut self, name: String, value: Value) -> bool {
        match self.0.iter_mut().find(|(k, _)| *k == name) {
            Some((_, v)) if *v == value => false,
            Some((_, v)) => {
                *v = value;
                true
            }
            None => {
                self.0.push((name, value));
                true
            }
        }
    }

    /// Returns whether the attribute existed.
    pub fn remove(&mut self, name: &str) -> bool {
        let before = self.0.len();
        self.0.retain(|(k, _)| k != name);
        self.0.len() != before
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, &Value)> {
        self.0.iter().map(|(k, v)| (k.as_str(), v))
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// Hierarchy changes go through [`crate::Dom`].
#[derive(Debug, Clone)]
pub struct Instance {
    pub class: ClassId,
    /// Set for classes Rook's database doesn't know; `class` is then `Instance`.
    pub unknown_class: Option<Box<str>>,
    pub(crate) name: String,
    pub(crate) parent: Option<InstanceId>,
    pub(crate) children: Children,
    pub(crate) parent_locked: bool,
    pub(crate) destroyed: bool,
    /// Explicitly set properties. Missing ones read the class default.
    /// Written only through [`crate::Dom`], which indexes references.
    pub(crate) properties: Properties,
    /// Properties from files that the database doesn't describe, kept so
    /// saving doesn't lose them.
    pub unknown_properties: Vec<(String, Value)>,
    pub attributes: Attributes,
    /// Written only through [`crate::Dom`], which indexes them.
    pub(crate) tags: Vec<String>,
    /// Counts up with every instance made, in any DataModel, so later
    /// ones compare greater (ids are reused and say nothing of age).
    pub(crate) created: u64,
}

/// An instance's children in order. They change only through `push` and
/// `remove`, which count a new version, so a reader keeping a copy can
/// always tell whether it's current.
#[derive(Debug, Clone, Default)]
pub(crate) struct Children {
    list: Vec<InstanceId>,
    version: u64,
}

impl Children {
    pub(crate) fn as_slice(&self) -> &[InstanceId] {
        &self.list
    }

    pub(crate) fn version(&self) -> u64 {
        self.version
    }

    pub(crate) fn push(&mut self, id: InstanceId) {
        self.list.push(id);
        self.version += 1;
    }

    pub(crate) fn remove(&mut self, id: InstanceId) {
        if let Some(pos) = self.list.iter().position(|c| *c == id) {
            self.list.remove(pos);
            self.version += 1;
        }
    }
}

static CREATED: AtomicU64 = AtomicU64::new(0);

impl Instance {
    /// Its CollectionService tags, in the order they were added.
    pub fn tags(&self) -> &[String] {
        &self.tags
    }

    pub(crate) fn new(class: ClassId, name: String) -> Self {
        Self {
            class,
            unknown_class: None,
            name,
            parent: None,
            children: Children::default(),
            parent_locked: false,
            destroyed: false,
            properties: Properties::default(),
            unknown_properties: Vec::new(),
            attributes: Attributes::default(),
            tags: Vec::new(),
            created: CREATED.fetch_add(1, Ordering::Relaxed),
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn parent(&self) -> Option<InstanceId> {
        self.parent
    }

    pub fn children(&self) -> &[InstanceId] {
        &self.children.list
    }

    pub fn is_destroyed(&self) -> bool {
        self.destroyed
    }

    pub fn properties(&self) -> &Properties {
        &self.properties
    }
}

/// An instance's explicitly set properties. Most set values equal the
/// class default (files save every property), so those keep only their
/// id, as Unreal keeps an object's differences from its class default
/// object; reading one reads the default. Both lists are sorted by id.
#[derive(Debug, Clone, Default)]
pub struct Properties {
    values: Vec<(PropertyId, Value)>,
    defaulted: Vec<PropertyId>,
}

impl Properties {
    /// The value set, when it isn't the class default.
    pub fn get(&self, property: PropertyId) -> Option<&Value> {
        let i = self.find(property).ok()?;
        Some(&self.values[i].1)
    }

    /// Whether `property` is set, to any value.
    pub fn contains(&self, property: PropertyId) -> bool {
        self.find(property).is_ok() || self.defaulted.binary_search(&property).is_ok()
    }

    /// The values set that aren't the class default.
    pub fn iter(&self) -> impl Iterator<Item = (PropertyId, &Value)> {
        self.values.iter().map(|(p, v)| (*p, v))
    }

    /// The properties set to the class default.
    pub fn defaulted(&self) -> &[PropertyId] {
        &self.defaulted
    }

    /// Every property set, whatever its value.
    pub fn ids(&self) -> impl Iterator<Item = PropertyId> + '_ {
        self.values
            .iter()
            .map(|(p, _)| *p)
            .chain(self.defaulted.iter().copied())
    }

    pub(crate) fn values_mut(&mut self) -> impl Iterator<Item = &mut Value> {
        self.values.iter_mut().map(|(_, v)| v)
    }

    /// Set a value that isn't the class default; returns the one it replaces.
    pub(crate) fn insert(&mut self, property: PropertyId, value: Value) -> Option<Value> {
        self.forget_default(property);
        match self.find(property) {
            Ok(i) => Some(std::mem::replace(&mut self.values[i].1, value)),
            Err(i) => {
                self.values.insert(i, (property, value));
                None
            }
        }
    }

    /// Set the class default; returns the value it replaces.
    pub(crate) fn insert_default(&mut self, property: PropertyId) -> Option<Value> {
        if let Err(i) = self.defaulted.binary_search(&property) {
            self.defaulted.insert(i, property);
        }
        self.remove_value(property)
    }

    /// Unset `property`; returns its value if it wasn't the class default.
    pub(crate) fn remove(&mut self, property: PropertyId) -> Option<Value> {
        self.forget_default(property);
        self.remove_value(property)
    }

    /// Release the room left over from growing, once loading is done.
    pub(crate) fn shrink_to_fit(&mut self) {
        self.values.shrink_to_fit();
        self.defaulted.shrink_to_fit();
    }

    fn remove_value(&mut self, property: PropertyId) -> Option<Value> {
        let i = self.find(property).ok()?;
        Some(self.values.remove(i).1)
    }

    fn forget_default(&mut self, property: PropertyId) {
        if let Ok(i) = self.defaulted.binary_search(&property) {
            self.defaulted.remove(i);
        }
    }

    fn find(&self, property: PropertyId) -> Result<usize, usize> {
        self.values.binary_search_by_key(&property, |(p, _)| *p)
    }
}
