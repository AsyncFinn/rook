//! Running Scripts and ModuleScripts: environments, chunk names, `require`.

use mlua::{Function, Lua, MultiValue, Table, Value as LuaValue};
use rook_dom::InstanceId;
use rook_types::Value;

use super::instance::{LuaInstance, push_instance};
use crate::args::Args;
use crate::ctx::{ApiResult, Outcome, ScriptError, api_fn, api_ret, with_world};
use crate::threads::spawn;
use crate::world::MessageType;

fn source_of(lua: &Lua, id: InstanceId) -> String {
    with_world(lua, |w| match w.dom.property_by_name(id, "Source") {
        Some(Value::String(s)) => s,
        _ => String::new(),
    })
}

/// A fresh environment whose globals fall back to the shared ones, with
/// `script` set. Global assignments stay in the script's own environment.
fn environment(lua: &Lua, script: InstanceId) -> mlua::Result<Table> {
    let env = lua.create_table()?;
    env.raw_set("script", push_instance(lua, script)?)?;
    let mt = lua.create_table()?;
    mt.raw_set("__index", lua.globals())?;
    env.set_metatable(Some(mt))?;
    Ok(env)
}

/// Compile a script's source. Chunk names are full names, so errors read
/// `Workspace.Script:3: ...` as on Roblox.
fn compile(lua: &Lua, id: InstanceId) -> mlua::Result<Function> {
    let name = with_world(lua, |w| w.dom.full_name(id));
    lua.load(source_of(lua, id))
        .set_name(format!("={name}"))
        .set_environment(environment(lua, id)?)
        .into_function()
}

/// Start a Script on a new thread. Compile errors go to the output.
/// Returns false if the script was already started or failed to compile.
pub fn run_script(lua: &Lua, id: InstanceId) -> bool {
    if with_world(lua, |w| w.started.contains_key(&id)) {
        return false;
    }
    let func = match compile(lua, id) {
        Ok(f) => f,
        Err(e) => {
            with_world(lua, |w| {
                w.errors += 1;
                w.push_log(MessageType::Error, crate::ctx::lua_error_message(&e));
            });
            return false;
        }
    };
    match lua.create_thread(func) {
        Ok(thread) => {
            with_world(lua, |w| w.started.insert(id, thread.clone()));
            crate::threads::resume(lua, &thread, ());
            true
        }
        Err(_) => false,
    }
}

/// Run a chunk of source as if it were a Script at `id` (used by tests and
/// the conformance runner).
pub fn run_source(
    lua: &Lua,
    chunk_name: &str,
    source: &str,
    script: Option<InstanceId>,
) -> mlua::Result<()> {
    let env = match script {
        Some(id) => environment(lua, id)?,
        None => fresh_environment(lua)?,
    };
    run_in(lua, chunk_name, source, env)
}

/// An environment that reads the shared globals and keeps its own writes.
fn fresh_environment(lua: &Lua) -> mlua::Result<Table> {
    let env = lua.create_table()?;
    let mt = lua.create_table()?;
    mt.raw_set("__index", lua.globals())?;
    env.set_metatable(Some(mt))?;
    Ok(env)
}

/// Run `source` in the environment kept under `name` across calls, so
/// globals one call sets the next one sees.
pub fn run_source_in(lua: &Lua, name: &str, chunk_name: &str, source: &str) -> mlua::Result<()> {
    let key = format!("rook.env.{name}");
    let env = match lua.named_registry_value::<Option<Table>>(&key)? {
        Some(env) => env,
        None => {
            let env = fresh_environment(lua)?;
            lua.set_named_registry_value(&key, &env)?;
            env
        }
    };
    run_in(lua, chunk_name, source, env)
}

fn run_in(lua: &Lua, chunk_name: &str, source: &str, env: Table) -> mlua::Result<()> {
    let f = lua
        .load(source)
        .set_name(format!("={chunk_name}"))
        .set_environment(env)
        .into_function()?;
    spawn(lua, f, ())?;
    Ok(())
}

// ---- require ------------------------------------------------------------------------

fn require(lua: &Lua, args: MultiValue) -> ApiResult<Outcome> {
    let a = Args::new(args, "require");
    let module = match a.get(0) {
        LuaValue::UserData(u) => u.borrow::<LuaInstance>().ok().map(|i| i.0),
        LuaValue::Integer(_) | LuaValue::Number(_) => {
            return Err(ScriptError(
                "require(assetId) needs the content system, which Rook doesn't have yet".into(),
            ));
        }
        _ => None,
    }
    .ok_or_else(|| ScriptError("Attempted to call require with invalid argument(s).".into()))?;
    if !with_world(lua, |w| w.dom.is_a(module, "ModuleScript")) {
        return Err(ScriptError(
            "Attempted to call require with invalid argument(s).".into(),
        ));
    }

    let cached = with_world(lua, |w| {
        w.modules
            .results
            .get(&module)
            .map(|k| lua.registry_value::<LuaValue>(k))
    });
    if let Some(v) = cached {
        return Outcome::ret(lua, v?);
    }
    if with_world(lua, |w| w.modules.loading.contains(&module)) {
        return Err(ScriptError(
            "Requested module was required recursively".into(),
        ));
    }
    let func = compile(lua, module).map_err(|e| {
        with_world(lua, |w| {
            w.errors += 1;
            w.push_log(MessageType::Error, crate::ctx::lua_error_message(&e));
        });
        ScriptError("Requested module experienced an error while loading".into())
    })?;
    with_world(lua, |w| w.modules.loading.push(module));
    let runner: Function = lua.named_registry_value("rook.module_runner")?;
    Ok(Outcome::Call(
        runner,
        MultiValue::from_vec(vec![
            LuaValue::UserData(push_instance(lua, module)?),
            LuaValue::Function(func),
        ]),
    ))
}

pub(crate) fn install(lua: &Lua) -> mlua::Result<()> {
    // Runs the module body from Lua, so modules may yield while loading.
    let finished = api_ret(lua, |lua, args| {
        let a = Args::new(args, "require");
        let module = a.instance(0)?;
        let ok = a.truthy(1);
        let value = a.get(2).clone();
        with_world(lua, |w| w.modules.loading.retain(|m| *m != module));
        if ok {
            let key = lua.create_registry_value(value)?;
            with_world(lua, |w| w.modules.results.insert(module, key));
        } else {
            let msg = crate::args::display_value(lua, &value);
            with_world(lua, |w| {
                w.errors += 1;
                w.push_log(MessageType::Error, msg);
            });
        }
        Ok(())
    })?;
    let runner: Function = lua
        .load(
            r##"
            local finished, pcall, select, error = ...
            local function check(module, ok, ...)
                if not ok then
                    finished(module, false, (...))
                    error("Requested module experienced an error while loading", 0)
                end
                if select("#", ...) ~= 1 then
                    finished(module, false, "Module code did not return exactly one value")
                    error("Module code did not return exactly one value", 0)
                end
                local value = ...
                finished(module, true, value)
                return value
            end
            return function(module, body)
                return check(module, pcall(body))
            end
            "##,
        )
        .set_name("=[rook] require")
        .call((
            finished,
            lua.globals().get::<Function>("pcall")?,
            lua.globals().get::<Function>("select")?,
            lua.globals().get::<Function>("error")?,
        ))?;
    lua.set_named_registry_value("rook.module_runner", runner)?;
    lua.globals().set("require", api_fn(lua, require)?)?;
    Ok(())
}
