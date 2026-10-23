//! Property values and the reference index; defaults in [`defaults`]
//! (Studio's measured ones in [`studio_defaults`]),
//! storing in the declared type in [`coerce`], typed reads in [`typed`].

use super::*;

mod coerce;
mod defaults;
mod studio_defaults;
mod typed;

pub(crate) use coerce::coerce;
pub use defaults::zero_value;

impl Dom {
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

    /// The reference properties pointing at `id`, as (holder, property)
    /// pairs. Holders may be destroyed; ones removed from the arena aren't
    /// listed.
    pub fn referrers(&self, id: InstanceId) -> &[(InstanceId, PropertyId)] {
        self.referrers.get(&id).map_or(&[], Vec::as_slice)
    }

    /// `Players.LocalPlayer` (set on clients only).
    pub fn local_player(&self) -> Option<InstanceId> {
        self.ref_property(self.find_service("Players")?, "LocalPlayer")
    }

    /// `Lighting.ClockTime`: Lighting's clock in hours, from `TimeOfDay`.
    pub fn clock_time(&self, lighting: InstanceId) -> f64 {
        self.string_property(lighting, "TimeOfDay")
            .map_or(14.0, |s| rook_types::clock_hours(&s))
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
        let inst = self.inst(id)?;
        let unchanged = match inst.properties.get(&property) {
            Some(current) => *current == value,
            None => self.default_is(inst.class, property, &value),
        };
        if unchanged {
            return Ok(());
        }
        self.store(id, property, value)?;
        self.journal.push(Change::Property { id, property });
        Ok(())
    }

    /// Whether `property` was set on `id`, rather than reading its class
    /// default.
    pub fn is_set(&self, id: InstanceId, property: PropertyId) -> bool {
        self.instances
            .get(id)
            .is_some_and(|i| i.properties.contains_key(&property))
    }

    /// Return a property to its class default, recording a change if that
    /// changes its value.
    pub fn reset_property(&mut self, id: InstanceId, property: PropertyId) -> Result<(), DomError> {
        let inst = self.inst_mut(id)?;
        let Some(old) = inst.properties.remove(&property) else {
            return Ok(());
        };
        self.unlink(id, property, &old);
        let class = self.inst(id)?.class;
        if !self.default_is(class, property, &old) {
            self.journal.push(Change::Property { id, property });
        }
        Ok(())
    }

    /// Store a property from a file without recording a change.
    pub fn load_property(&mut self, id: InstanceId, property: PropertyId, value: Value) {
        let value = coerce(self.db.property(property).ty, value);
        // A load into a missing instance has nowhere to go.
        let _ = self.store(id, property, value);
    }

    /// Store a value, keeping the reference index up to date.
    fn store(
        &mut self,
        id: InstanceId,
        property: PropertyId,
        value: Value,
    ) -> Result<(), DomError> {
        let target = ref_target(&value);
        if let Some(old) = self.inst_mut(id)?.properties.insert(property, value) {
            self.unlink(id, property, &old);
        }
        if let Some(target) = target {
            self.referrers
                .entry(target)
                .or_default()
                .push((id, property));
        }
        Ok(())
    }

    pub(crate) fn unlink(&mut self, holder: InstanceId, property: PropertyId, old: &Value) {
        let Some(target) = ref_target(old) else {
            return;
        };
        if let Some(list) = self.referrers.get_mut(&target) {
            list.retain(|r| *r != (holder, property));
            if list.is_empty() {
                self.referrers.remove(&target);
            }
        }
    }

    pub fn name_property(&self) -> PropertyId {
        self.name_prop
    }

    pub fn parent_property(&self) -> PropertyId {
        self.parent_prop
    }
}

pub(crate) fn ref_target(value: &Value) -> Option<InstanceId> {
    match value {
        Value::Ref(Some(r)) => Some(InstanceId::from_ref(*r)),
        _ => None,
    }
}
