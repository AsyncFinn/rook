//! Property values, defaults and the reference index.

use super::*;

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
        self.store(id, property, value)?;
        self.journal.push(Change::Property { id, property });
        Ok(())
    }

    /// Store a property from a file without recording a change.
    pub fn load_property(&mut self, id: InstanceId, property: PropertyId, value: Value) {
        let value = coerce(self.db.property(property).ty, value);
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

/// Defaults the database doesn't list (properties that don't serialize,
/// where the zero value is wrong) or lists wrongly. Keyed by the class that
/// declares the property.
const DEFAULTS: &[(&str, &str, Value)] = &[
    // Confirmed: creator-docs WeldConstraint.Enabled.
    ("WeldConstraint", "Enabled", Value::Bool(true)),
    // Confirmed: creator-docs, and Studio 0.742 (the database says false).
    ("SoundService", "RespectFilteringEnabled", Value::Bool(true)),
];

/// [`DEFAULTS`], then the class default from the database, then the zero
/// value of the property's type (Approximation: the database lists
/// defaults only for properties that serialize).
fn convert_default(db: &Database, class: ClassId, property: PropertyId) -> Option<Value> {
    let prop = db.property(property);
    let ty = prop.ty;
    let owner = db.class(prop.owner).name;
    if let Some((_, _, v)) = DEFAULTS
        .iter()
        .find(|(c, p, _)| *c == owner && *p == prop.name)
    {
        return Some(v.clone());
    }
    if let Some(variant) = db.default_value(class, property) {
        return Value::from_variant(variant.clone(), |_| None)
            .ok()
            .map(|v| coerce(ty, v));
    }
    zero_value(db, ty)
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
