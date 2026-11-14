use rook_reflection::PropertyId;
use rook_types::Value;

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
