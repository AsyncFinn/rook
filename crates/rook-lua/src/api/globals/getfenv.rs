//! `getfenv` as a script sees it in Roblox, where each script's thread
//! has the script's environment as its globals: level 0 and any C
//! function's level give that environment. Rook's threads share the
//! globals, and its engine-side Luau (`[rook] ...` chunks) has
//! environments of its own, so Luau's `getfenv` would hand out tables a
//! script never sees in Roblox. Anti-tamper scripts walk the stack
//! comparing each level's environment with their own and hang the
//! client when one differs.
//!
//! Like Rook's other globals it raises plain-string errors through a Luau
//! shim, so levels are counted from past the shim.

use mlua::{Function, Lua, Table, Value as LuaValue};

use crate::ctx::args::type_name;

/// What chunk names of Rook's own Luau start with.
const ENGINE_CHUNK: &str = "[rook]";

/// `getfenv`: the shim, given the error function to raise with.
pub(super) fn getfenv(lua: &Lua, error: LuaValue) -> mlua::Result<Function> {
    let raw = lua.create_function(|lua, arg: LuaValue| {
        Ok(match environment(lua, arg) {
            Ok(env) => (Some(env), None),
            Err(why) => (
                None,
                Some(format!("invalid argument #1 to 'getfenv' ({why})")),
            ),
        })
    })?;
    lua.load(
        r#"
        local raw, error = ...
        return function(f)
            local env, err = raw(f)
            if err then
                error(err, 0)
            end
            return env
        end
        "#,
    )
    .set_name("=[rook] getfenv")
    .call((raw, error))
}

/// The environment `arg` (a function, or a level counted from the
/// shim's caller) names, or why it names none.
fn environment(lua: &Lua, arg: LuaValue) -> Result<Table, String> {
    let function = match arg {
        LuaValue::Function(f) => Some(f),
        LuaValue::Nil => function_at(lua, 1)?,
        LuaValue::Integer(_) | LuaValue::Number(_) => {
            let level = lua.coerce_integer(arg).ok().flatten().unwrap_or(0);
            if level < 0 {
                return Err("level must be non-negative".into());
            }
            function_at(lua, level as usize)?
        }
        other => return Err(format!("number expected, got {}", type_name(&other))),
    };
    let env = match function.filter(is_script_code) {
        Some(f) => f.environment(),
        None => None,
    };
    let env = match env {
        Some(env) => env,
        None => script_environment(lua),
    };
    // Luau's own rule: an environment a script reached through getfenv
    // may be changed, so builtins stop taking their fast paths in it.
    env.set_safeenv(false);
    Ok(env)
}

/// The function running `level` calls up from the shim's caller (0 is
/// `getfenv` itself, which has none of a script's).
fn function_at(lua: &Lua, level: usize) -> Result<Option<Function>, String> {
    if level == 0 {
        return Ok(None);
    }
    // Past this function and the shim.
    match running(lua, level + 1) {
        Some(f) => Ok(Some(f)),
        None => Err("invalid level".into()),
    }
}

/// The function running `level` calls up from this one.
#[expect(
    clippy::redundant_closure_for_method_calls,
    reason = "`Debug::function` can't be passed for every lifetime"
)]
fn running(lua: &Lua, level: usize) -> Option<Function> {
    lua.inspect_stack(level, |d| d.function())
}

/// A Luau function of a script's, not a C function or Rook's own Luau.
fn is_script_code(f: &Function) -> bool {
    let info = f.info();
    info.what != "C" && !info.short_src.is_some_and(|s| s.starts_with(ENGINE_CHUNK))
}

/// The environment of the script running this thread: that of the
/// outermost script function on the stack, the one the thread started in.
fn script_environment(lua: &Lua) -> Table {
    let mut found = None;
    let mut level = 1;
    while let Some(f) = running(lua, level) {
        if is_script_code(&f)
            && let Some(env) = f.environment()
        {
            found = Some(env);
        }
        level += 1;
    }
    found.unwrap_or_else(|| lua.globals())
}
