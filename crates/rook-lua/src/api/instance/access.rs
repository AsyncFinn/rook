//! Reading and writing properties.

use super::*;

/// Read a property by canonical name, including derived ones.
pub(crate) fn read_property(lua: &Lua, id: InstanceId, name: &str) -> ApiResult<LuaValue> {
    let prop = with_world(lua, |w| {
        let class = w.dom.get(id)?.class;
        let p = w.dom.database().find_canonical_property(class, name)?;
        let enum_id = match p.ty {
            PropertyType::Enum(e) => Some(e),
            PropertyType::Value(_) => None,
        };
        Some((p.id, enum_id))
    });
    Ok(read(lua, &ctx(lua), id, name, prop)?.0)
}

/// Read a property given its canonical name and, when the database has
/// it, its ID and enum. Also says whether this was a plain read of the
/// stored value (not a derived property or a view of another).
pub(super) fn read(
    lua: &Lua,
    c: &Ctx,
    id: InstanceId,
    name: &str,
    prop: Option<(PropertyId, Option<EnumId>)>,
) -> ApiResult<(LuaValue, bool)> {
    let (value, plain) = {
        let w = c.world.borrow();
        match prop.and_then(|(pid, _)| properties::content_view(pid)) {
            Some(target) => (
                w.dom.property(id, target).map(|v| {
                    Value::ContentId(v.content_url().unwrap_or_default().to_owned().into())
                }),
                false,
            ),
            None => match properties::get(&w, id, name) {
                Some(v) => (Some(v), false),
                None => (
                    prop.and_then(|(pid, _)| w.dom.property(id, pid)),
                    prop.is_some(),
                ),
            },
        }
    };
    let value = match value {
        Some(v) => to_lua(lua, &v, prop.and_then(|(_, e)| e))?,
        None => LuaValue::Nil,
    };
    Ok((value, plain))
}

/// `inst[key] = value` for scripts.
pub(crate) fn set_member(lua: &Lua, id: InstanceId, key: &str, value: LuaValue) -> ApiResult<()> {
    let p = match find_member(lua, id, key) {
        Some(Member::Property(p)) => p,
        Some(Member::Callback(name)) => {
            let f = match value {
                LuaValue::Function(f) => Some(f),
                LuaValue::Nil => None,
                _ => {
                    return Err(ScriptError(format!(
                        "Unable to assign property {key}. Function expected, got {}",
                        crate::args::type_name(&value)
                    )));
                }
            };
            methods::set_callback(lua, id, name, f)?;
            return Ok(());
        }
        _ => return Err(not_a_member(lua, id, key)),
    };
    if !allowed(p.security[1]) {
        return Err(ScriptError(format!(
            "The current thread cannot write '{key}' (lacking capability {})",
            p.security[1].capability()
        )));
    }
    if p.flags.read_only || p.scriptability == Scriptability::Read {
        return Err(ScriptError(format!(
            "Unable to assign property {key}. Property is read only"
        )));
    }
    write_property(lua, id, p.name, &value)
        .map_err(|e| ScriptError(format!("Unable to assign property {key}. {e}")))
        .and_then(|r| r)
}

/// Convert and store a property, then fire its signals. The outer error is
/// a conversion failure (gets the "Unable to assign" prefix); the inner one
/// comes from the DOM (locked parent, cycles) and is reported as is.
pub(crate) fn write_property(
    lua: &Lua,
    id: InstanceId,
    name: &str,
    value: &LuaValue,
) -> Result<ApiResult<()>, String> {
    let db = database();
    let (class, start) = with_world(lua, |w| {
        (w.dom.get(id).map(|i| i.class), w.dom.journal().end())
    });
    let Some(class) = class else {
        return Ok(Err(ScriptError("instance no longer exists".into())));
    };
    let prop = db
        .find_canonical_property(class, name)
        .ok_or_else(|| format!("{name} is not a property"))?;
    let converted = from_lua(lua, value, prop.ty)?;
    if let Some(e) = with_world(lua, |w| {
        properties::wrong_ref_class(w, id, name, &converted)
    }) {
        return Ok(Err(ScriptError(e)));
    }
    let result = with_world(lua, |w| {
        if let (Some(target), Some(url)) =
            (properties::content_view(prop.id), converted.content_url())
        {
            // A Decal's or Texture's image set to a web URL is stored as its
            // asset id (`rook_content::image_url_rewrite`).
            let rewritten = (matches!(name, "Texture" | "ColorMap") && w.dom.is_a(id, "Decal"))
                .then(|| rook_content::image_url_rewrite(url))
                .flatten();
            let url = rewritten.as_deref().unwrap_or(url);
            // A SpecialMesh given a mesh or a texture becomes a FileMesh
            // (observed in Studio 0.742; conformance case `content/decal`).
            if matches!(name, "MeshId" | "TextureId")
                && !url.is_empty()
                && w.dom.class_name(id) == "SpecialMesh"
            {
                w.dom
                    .set_property_by_name(id, "MeshType", rook_types::Value::Enum(5))?;
            }
            return w
                .dom
                .set_property(id, target, properties::content_value(url));
        }
        if let Some(r) = properties::set(w, id, name, &converted) {
            return r;
        }
        match name {
            "Name" => {
                let rook_types::Value::String(s) = converted else {
                    return Ok(());
                };
                w.dom.set_name(id, s)
            }
            "Parent" => {
                let rook_types::Value::Ref(r) = converted else {
                    return Ok(());
                };
                w.dom.set_parent(id, r.map(InstanceId::from_ref))
            }
            _ => w.dom.set_property(id, prop.id, converted),
        }
    });
    dispatch(lua, start);
    Ok(result.map_err(ScriptError::from))
}
