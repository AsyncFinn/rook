//! Material colors: `GetMaterialColor` and `SetMaterialColor`.

use super::*;

/// The `MaterialColors` key of a material argument, with Studio's errors.
fn color_key(lua: &Lua, a: &Args) -> ApiResult<(TerrainMaterial, TerrainMaterials)> {
    let m = material_arg(lua, a, 0)?;
    let t = TerrainMaterial::from_material(m)
        .ok_or_else(|| ScriptError("Invalid terrain material".into()))?;
    let key = t
        .name()
        .parse()
        .map_err(|_| ScriptError("Unsupported terrain material".into()))?;
    Ok((t, key))
}

fn material_colors(dom: &rook_dom::Dom, id: InstanceId) -> Option<rook_types::MaterialColors> {
    dom.material_colors_property(id, "MaterialColors")
}

pub(in crate::api::methods) fn get_material_color(
    lua: &Lua,
    this: InstanceId,
    a: &Args,
) -> ApiResult<Outcome> {
    let (t, _) = color_key(lua, a)?;
    let colors = with_world(lua, |w| material_colors(&w.dom, this));
    let c = t
        .color(colors.as_ref(), MaterialSet::Materials2022)
        .unwrap_or(rook_types::Color3uint8::new(0, 0, 0));
    Outcome::ret(lua, LColor3(rook_types::Color3::from(c)))
}

pub(in crate::api::methods) fn set_material_color(
    lua: &Lua,
    this: InstanceId,
    a: &Args,
) -> ApiResult<Outcome> {
    let (_, key) = color_key(lua, a)?;
    let color = a.userdata::<LColor3>(1, "Color3")?.0;
    mutate(lua, |w| {
        let mut colors = material_colors(&w.dom, this).unwrap_or_default();
        colors.set_color(key, color.into());
        w.dom
            .set_property_by_name(this, "MaterialColors", Value::MaterialColors(colors))
    })?;
    Outcome::ret(lua, ())
}
