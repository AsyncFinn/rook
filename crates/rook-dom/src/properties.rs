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
        let inst = self.slot(id)?;
        if property == self.name_prop {
            return Some(Value::String(inst.name.clone()));
        }
        if property == self.parent_prop {
            return Some(Value::Ref(inst.parent.map(InstanceId::to_ref)));
        }
        if let Some(v) = inst.properties.get(&property) {
            return Some(v.clone());
        }
        if Some(property) == self.smooth_grid
            && let Some(v) = self.encoded_voxels(id)
        {
            return Some(v);
        }
        let class = inst.class;
        self.default_value(class, property)
    }

    /// Read a property where it's kept, without copying it: `f` gets the
    /// value set, or the class default. The typed reads go through here.
    pub fn read_property<R>(
        &self,
        id: InstanceId,
        property: PropertyId,
        f: impl FnOnce(&Value) -> Option<R>,
    ) -> Option<R> {
        let inst = self.slot(id)?;
        if property == self.name_prop || property == self.parent_prop {
            return f(&self.property(id, property)?);
        }
        if let Some(v) = inst.properties.get(&property) {
            return f(v);
        }
        if Some(property) == self.smooth_grid {
            return f(&self.property(id, property)?);
        }
        self.with_default(inst.class, property, f)
    }

    /// [`Dom::read_property`], looked up by name (aliases resolved).
    pub fn read_by_name<R>(
        &self,
        id: InstanceId,
        name: &str,
        f: impl FnOnce(&Value) -> Option<R>,
    ) -> Option<R> {
        let inst = self.slot(id)?;
        let prop = self.db.find_canonical_property(inst.class, name)?;
        self.read_property(id, prop.id, f)
    }

    /// Like [`Dom::property`], looked up by name (aliases resolved).
    pub fn property_by_name(&self, id: InstanceId, name: &str) -> Option<Value> {
        let inst = self.slot(id)?;
        let prop = self.db.find_canonical_property(inst.class, name)?;
        self.property(id, prop.id)
    }

    /// The reference properties pointing at `id`, as (holder, property)
    /// pairs. Holders may be destroyed; freed ones aren't listed.
    pub fn referrers(&self, id: InstanceId) -> &[(InstanceId, PropertyId)] {
        self.check_not_freed(id);
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

    pub fn property_id(&self, id: InstanceId, name: &str) -> Option<PropertyId> {
        let inst = self.slot(id)?;
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
        // A whole `SmoothGrid` is taken without comparing it.
        let unchanged = Some(property) != self.smooth_grid
            && match inst.properties.get(&property) {
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
            || Some(property) == self.smooth_grid && self.voxels.contains_key(&id)
    }

    /// Return a property to its class default, recording a change if that
    /// changes its value.
    pub fn reset_property(&mut self, id: InstanceId, property: PropertyId) -> Result<(), DomError> {
        if Some(property) == self.smooth_grid && self.voxels.remove(&id).is_some() {
            self.journal.push(Change::Property { id, property });
        }
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

    /// Store a property without recording a change: a file's values, and
    /// values that change every frame with no journal reader to tell
    /// (time passing, physics' velocities).
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
        if Some(property) == self.smooth_grid && self.store_voxels(id, &value)? {
            return Ok(());
        }
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
