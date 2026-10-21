//! MaterialService: which MaterialVariant stands in for each material
//! (`docs/behavior/rendering/materials.md`).

use super::*;
use rook_types::Material;

/// A `Material` argument, by EnumItem, number or name.
fn material(lua: &Lua, a: &Args, i: usize) -> ApiResult<Material> {
    let id = database()
        .enum_by_name("Material")
        .expect("Material enum")
        .id;
    let value = crate::types::enums::coerce(lua, a.get(i), id).map_err(ScriptError)?;
    Ok(Material::of_part(Some(value)))
}

/// The override property of `m`, or Studio's error for a material without
/// one (observed in Studio 0.742).
fn override_property(method: &str, m: Material) -> ApiResult<String> {
    m.override_property().ok_or_else(|| {
        ScriptError(format!(
            "{method} failed: Material '{}' is not supported.",
            m.name()
        ))
    })
}

pub(super) fn get_base_material_override(
    lua: &Lua,
    this: InstanceId,
    a: &Args,
) -> ApiResult<Outcome> {
    let m = material(lua, a, 0)?;
    override_property("GetBaseMaterialOverride", m)?;
    let name = with_world(lua, |w| {
        rook_physics::contacts::materials::base_material_override(&w.dom, this, m)
    });
    Outcome::ret(lua, name)
}

/// Any name is accepted, even one no MaterialVariant has (observed).
pub(super) fn set_base_material_override(
    lua: &Lua,
    this: InstanceId,
    a: &Args,
) -> ApiResult<Outcome> {
    let m = material(lua, a, 0)?;
    let name = a.string(1)?;
    let property = override_property("SetBaseMaterialOverride", m)?;
    mutate(lua, |w| {
        w.dom
            .set_property_by_name(this, &property, Value::String(name))
    })?;
    Outcome::ret(lua, ())
}

/// The first MaterialVariant among the service's descendants with this
/// name and base material, or nil (observed: one inside a Folder is found;
/// one of another base material isn't).
pub(super) fn get_material_variant(lua: &Lua, this: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let m = material(lua, a, 0)?;
    let name = a.string(1)?;
    let found = with_world(lua, |w| {
        rook_physics::contacts::materials::find_variant(&w.dom, this, m, &name)
    });
    ret_instance(lua, found)
}
