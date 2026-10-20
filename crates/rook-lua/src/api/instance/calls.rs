//! Method functions, and the fast forms of the lookups scripts call most.

use super::*;

/// Each `(owner, name)` method gets one function, created on first use,
/// so `a.Destroy == b.Destroy`.
pub(super) fn method_function(
    lua: &Lua,
    owner: &'static str,
    name: &'static str,
    preferred: Option<&'static str>,
) -> mlua::Result<Function> {
    let c = ctx(lua);
    let key = (owner, name);
    if let Some(f) = c.methods.borrow().get(&key) {
        return Ok(f.clone());
    }
    if let Some(source) = methods::luau_source(owner, name) {
        let std: mlua::Table = lua.named_registry_value("rook.std")?;
        let f: Function = lua
            .load(source)
            .set_name(format!("=[rook] {owner}.{name}"))
            .call(std)?;
        c.methods.borrow_mut().insert(key, f.clone());
        return Ok(f);
    }
    // A deprecated lowercase spelling (`findFirstChild`, `makeJoints`)
    // behaves as the capitalized member.
    let capitalized = preferred
        .filter(|p| p.eq_ignore_ascii_case(name))
        .map(str::to_owned)
        .or_else(|| {
            let mut chars = name.chars();
            let first = chars.next()?;
            first
                .is_ascii_lowercase()
                .then(|| first.to_ascii_uppercase().to_string() + chars.as_str())
        });
    let resolved = if methods::native(owner, name).is_some() {
        name.to_owned()
    } else {
        capitalized.unwrap_or_else(|| name.to_owned())
    };
    let native = methods::native(owner, &resolved);
    let inert = native.is_some() && methods::is_inert(owner, &resolved);
    let resolved: &'static str = Box::leak(resolved.into_boxed_str());
    let f = match native {
        Some(raw) => api_fn(lua, move |lua, args| {
            profiling::scope!("Instance method", &format!("{owner}:{resolved}"));
            let (this, args) = Args::method(args, name)?;
            if inert {
                with_world(lua, |w| w.count_inert(format!("{owner}.{resolved}")));
            }
            raw(lua, this, &args)
        })?,
        None => api_fn(lua, move |lua, args| {
            let (this, _) = Args::method(args, name)?;
            let class = with_world(lua, |w| {
                w.count_unimplemented(format!("{owner}.{name}"));
                w.dom.class_name(this).to_owned()
            });
            Err(ScriptError(format!(
                "{class}:{name} is not implemented in Rook yet"
            )))
        })?,
    };
    let f = match methods::fast(owner, resolved) {
        Some((core, takes_string)) => fast_method(lua, core, takes_string, f)?,
        None => f,
    };
    c.methods.borrow_mut().insert(key, f.clone());
    Ok(f)
}

/// A method with a fast form (see [`methods::fast`]): a Luau wrapper calls
/// `core` with the instance's key, and `general` for anything else.
pub(super) fn fast_method(
    lua: &Lua,
    core: methods::Core,
    takes_string: bool,
    general: Function,
) -> mlua::Result<Function> {
    let c = ctx(lua);
    let fast = lua.create_function(
        move |lua, (key, arg, flag): (LightUserData, Option<String>, LuaValue)| {
            let found = core(
                &c.world.borrow(),
                id_of(key),
                arg.as_deref().unwrap_or_default(),
                !matches!(flag, LuaValue::Nil | LuaValue::Boolean(false)),
            );
            Ok(match found {
                methods::Found::Bool(b) => (LuaValue::Boolean(b), None),
                methods::Found::Instance(Some(id)) => {
                    (LuaValue::LightUserData(key_of(id)), Some(1))
                }
                methods::Found::Instance(None) => (LuaValue::Nil, None),
                methods::Found::Instances(ids) => (
                    LuaValue::Table(lua.create_sequence_from(ids.into_iter().map(key_of))?),
                    Some(2),
                ),
            })
        },
    )?;
    let wrap: Function = lua.named_registry_value("rook.instance.fast_method")?;
    wrap.call((fast, general, takes_string))
}
