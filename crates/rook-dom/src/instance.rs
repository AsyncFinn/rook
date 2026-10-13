use rustc_hash::FxHashMap;

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

/// One instance's data. Hierarchy changes go through [`crate::Dom`].
#[derive(Debug, Clone)]
pub struct Instance {
    pub class: ClassId,
    /// Set for classes Rook's database doesn't know; `class` is then `Instance`.
    pub unknown_class: Option<Box<str>>,
    pub(crate) name: String,
    pub(crate) parent: Option<InstanceId>,
    pub(crate) children: Vec<InstanceId>,
    pub(crate) parent_locked: bool,
    pub(crate) destroyed: bool,
    /// Explicitly set properties. Missing ones read the class default.
    /// Written only through [`crate::Dom`], which indexes references.
    pub(crate) properties: FxHashMap<PropertyId, Value>,
    /// Properties from files that the database doesn't describe, kept so
    /// saving doesn't lose them.
    pub unknown_properties: Vec<(String, Value)>,
    pub attributes: Attributes,
    pub tags: Vec<String>,
}

impl Instance {
    pub(crate) fn new(class: ClassId, name: String) -> Self {
        Self {
            class,
            unknown_class: None,
            name,
            parent: None,
            children: Vec::new(),
            parent_locked: false,
            destroyed: false,
            properties: FxHashMap::default(),
            unknown_properties: Vec::new(),
            attributes: Attributes::default(),
            tags: Vec::new(),
        }
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn parent(&self) -> Option<InstanceId> {
        self.parent
    }

    pub fn children(&self) -> &[InstanceId] {
        &self.children
    }

    pub fn is_destroyed(&self) -> bool {
        self.destroyed
    }

    /// Explicitly set properties.
    pub fn properties(&self) -> &FxHashMap<PropertyId, Value> {
        &self.properties
    }
}
