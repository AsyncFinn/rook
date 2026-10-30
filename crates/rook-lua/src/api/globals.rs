//! Roblox globals: output, legacy and `task` scheduling, `typeof`,
//! `getfenv`, `Instance.new`, `game`/`workspace`.
//!
//! The list follows creator-docs "Roblox globals" and the `task` library
//! page.

use crate::scheduler::{LEGACY_MIN_WAIT, Queue};
use mlua::{Function, Lua, MultiValue, Table, Thread, Value as LuaValue};

mod getfenv;
mod instance_new;
mod new_name;
mod output;
mod scheduling;
mod type_of;

use instance_new::instance_table;
use new_name::new_name;
use output::*;
use scheduling::*;
use type_of::typeof_;

use super::instance::push_instance;
use crate::ctx::args::{Args, display_value, type_name};
use crate::ctx::{ApiResult, Outcome, ScriptError, api_fn, api_ret, with_world};
use crate::threads::resume;
use crate::world::{Job, MessageType, WaitKind};

pub(crate) fn install(lua: &Lua) -> mlua::Result<()> {
    let g = lua.globals();

    // Originals of library functions engine-side Luau code relies on.
    let std = lua.create_table()?;
    for name in ["coroutine", "error", "type", "pcall", "select", "table"] {
        std.set(name, g.get::<LuaValue>(name)?)?;
    }

    g.set(
        "print",
        api_ret(lua, |lua, args| {
            log(lua, MessageType::Output, &args);
            Ok(())
        })?,
    )?;
    let warn = api_ret(lua, |lua, args| {
        log(lua, MessageType::Warning, &args);
        Ok(())
    })?;
    g.set("warn", warn.clone())?;
    std.set("warn", warn)?;

    let wait_fn = api_fn(lua, wait)?;
    g.set("wait", wait_fn.clone())?;
    g.set("Wait", wait_fn)?;
    let delay_fn = api_fn(lua, delay)?;
    g.set("delay", delay_fn.clone())?;
    g.set("Delay", delay_fn)?;
    let spawn_fn = api_fn(lua, legacy_spawn)?;
    g.set("spawn", spawn_fn.clone())?;
    g.set("Spawn", spawn_fn)?;

    let task = lua.create_table()?;
    task.set("wait", api_fn(lua, task_wait)?)?;
    task.set("spawn", api_fn(lua, task_spawn)?)?;
    task.set("defer", api_fn(lua, task_defer)?)?;
    task.set("delay", api_fn(lua, task_delay)?)?;
    task.set("cancel", api_fn(lua, task_cancel)?)?;
    // One engine thread: parallel phases are no-ops until Actors exist.
    let noop = lua.create_function(|_, ()| Ok(()))?;
    task.set("synchronize", noop.clone())?;
    task.set("desynchronize", noop)?;
    g.set("task", task.clone())?;
    std.set("task", task)?;
    std.set(
        "get_callback",
        lua.create_function(|lua, (inst, name): (mlua::AnyUserData, String)| {
            let id = inst.borrow::<super::instance::LuaInstance>()?.0;
            super::methods::get_callback(lua, id, &name)
        })?,
    )?;
    lua.set_named_registry_value("rook.std", std.clone())?;

    let time_fn = api_ret(lua, |lua, _| Ok(now(lua)))?;
    g.set("time", time_fn.clone())?;
    g.set("elapsedTime", time_fn.clone())?;
    g.set("ElapsedTime", time_fn)?;
    let start_epoch = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0.0, |d| d.as_secs_f64());
    // Fixed clocks keep tick() consistent with game time.
    g.set(
        "tick",
        api_ret(lua, move |lua, _| Ok(start_epoch + now(lua)))?,
    )?;
    g.set("typeof", api_ret(lua, typeof_)?)?;
    g.set("getfenv", getfenv::getfenv(lua, std.get("error")?)?)?;
    std.set(
        "datastore",
        super::methods::data_stores::datastore_helpers(lua)?,
    )?;
    g.set(
        "version",
        lua.create_function(|_, ()| Ok(rook_reflection::roblox_version()))?,
    )?;
    g.set(
        "Version",
        lua.create_function(|_, ()| Ok(rook_reflection::roblox_version()))?,
    )?;
    g.set(
        "printidentity",
        api_ret(lua, |lua, args| {
            let a = Args::new(args, "printidentity");
            let prefix = a
                .opt_string(0)?
                .unwrap_or_else(|| "Current identity is".into());
            with_world(lua, |w| {
                w.push_log(MessageType::Output, format!("{prefix} 2"))
            });
            Ok(())
        })?,
    )?;
    // `settings()` and `UserSettings()`: one of each per DataModel, outside
    // its tree.
    for (global, class) in [
        ("settings", "GlobalSettings"),
        ("UserSettings", "UserSettings"),
    ] {
        let key = format!("rook.{class}");
        g.set(
            global,
            lua.create_function(move |lua, ()| {
                if let Some(ud) = lua.named_registry_value::<Option<mlua::AnyUserData>>(&key)? {
                    return Ok(ud);
                }
                let id = with_world(lua, |w| -> Result<_, rook_dom::DomError> {
                    // Scripts read `UserSettings().GameSettings` without
                    // asking for the service first: the player's settings,
                    // which the engine fills from their saved file.
                    if class == "UserSettings" {
                        return Ok(crate::user_settings::ensure(w).0);
                    }
                    let id = w.dom.create_by_name(class)?;
                    w.dom.set_name(id, class.to_owned())?;
                    // And `settings().Rendering`.
                    for (child, name) in [
                        ("RenderSettings", "Rendering"),
                        ("NetworkSettings", "Network"),
                        ("PhysicsSettings", "Physics"),
                        ("DebugSettings", "Diagnostics"),
                    ] {
                        let c = w.dom.create_by_name(child)?;
                        w.dom.set_name(c, name.into())?;
                        w.dom.set_parent(c, Some(id))?;
                    }
                    Ok(id)
                })
                .map_err(|e| mlua::Error::RuntimeError(e.to_string()))?;
                let ud = push_instance(lua, id)?;
                lua.set_named_registry_value(&key, &ud)?;
                Ok(ud)
            })?,
        )?;
    }
    // Roblox's profiler and memory-category marks: no profiler to feed.
    let debug: Table = g.get("debug")?;
    let noop = lua.create_function(|_, _: MultiValue| Ok(()))?;
    for name in [
        "profilebegin",
        "profileend",
        "setmemorycategory",
        "resetmemorycategory",
    ] {
        debug.set(name, noop.clone())?;
    }
    g.set("ypcall", g.get::<Function>("pcall")?)?;
    g.set("shared", lua.create_table()?)?;
    g.set("Instance", instance_table(lua)?)?;

    let (root, workspace) = with_world(lua, |w| {
        let root = w.dom.root();
        (root, w.dom.get_service("Workspace").ok())
    });
    let game = push_instance(lua, root)?;
    g.set("game", game.clone())?;
    g.set("Game", game)?;
    if let Some(ws) = workspace {
        let ws = push_instance(lua, ws)?;
        g.set("workspace", ws.clone())?;
        g.set("Workspace", ws)?;
    }
    install_engine_helpers(lua)?;
    Ok(())
}

/// Helpers for engine features written in Luau (tweens, UIPageLayout).
/// They bypass the rules game scripts follow, so they live only in the
/// registry.
fn install_engine_helpers(lua: &Lua) -> mlua::Result<()> {
    use mlua::AnyUserData;
    let std: Table = lua.named_registry_value("rook.std")?;
    let g = lua.globals();
    for name in [
        "typeof",
        "pairs",
        "ipairs",
        "setmetatable",
        "math",
        "string",
        "tostring",
    ] {
        std.set(name, g.get::<LuaValue>(name)?)?;
    }
    std.set(
        "set_internal",
        lua.create_function(
            |lua, (inst, name, value): (AnyUserData, String, LuaValue)| {
                let id = inst.borrow::<super::instance::LuaInstance>()?.0;
                let known = with_world(lua, |w| w.dom.property_id(id, &name).is_some());
                // Not a stored property, or a value Rook can't store (a
                // DateTime): keep it as a runtime value, which reads first.
                if !known
                    || !matches!(
                        super::instance::write_property(lua, id, &name, &value),
                        Ok(Ok(()))
                    )
                {
                    let all = super::instance::runtime_properties(lua)?;
                    crate::ctx::ctx(lua)
                        .runtime_prop_ids
                        .borrow_mut()
                        .insert(id);
                    let key = super::instance::key_of(id);
                    let t: Table = match all.raw_get::<Option<Table>>(key)? {
                        Some(t) => t,
                        None => {
                            let t = lua.create_table()?;
                            all.raw_set(key, t.clone())?;
                            t
                        }
                    };
                    return t.raw_set(name, value);
                }
                Ok(())
            },
        )?,
    )?;
    std.set(
        "fire",
        lua.create_function(
            |lua, (inst, name, args): (AnyUserData, String, mlua::Variadic<LuaValue>)| {
                let id = inst.borrow::<super::instance::LuaInstance>()?.0;
                // Only engine events raised from Luau are listed here.
                let event: &'static str = match name.as_str() {
                    "Completed" => "Completed",
                    "Stopped" => "Stopped",
                    "MessageReceived" => "MessageReceived",
                    "SendingMessage" => "SendingMessage",
                    "Chatted" => "Chatted",
                    _ => {
                        return Err(mlua::Error::RuntimeError(format!(
                            "unknown engine event {name}"
                        )));
                    }
                };
                crate::threads::fire(
                    lua,
                    &crate::signal::SignalKey::event(id, event),
                    &args.into_iter().collect::<Vec<_>>(),
                );
                Ok(())
            },
        )?,
    )?;
    std.set(
        "new_instance",
        lua.create_function(|lua, class: String| {
            let id = with_world(lua, |w| w.dom.create_by_name(&class))
                .map_err(|e| mlua::Error::RuntimeError(e.to_string()))?;
            push_instance(lua, id)
        })?,
    )?;
    std.set(
        "pages",
        lua.create_function(|lua, inst: AnyUserData| {
            let id = inst.borrow::<super::instance::LuaInstance>()?.0;
            let pages = with_world(lua, |w| w.gui.pages(&mut w.dom, id));
            (pages.into_iter())
                .map(|p| push_instance(lua, p))
                .collect::<mlua::Result<Vec<_>>>()
        })?,
    )?;
    std.set(
        "turn_pages",
        lua.create_function(|lua, (inst, at): (AnyUserData, Option<f32>)| {
            let id = inst.borrow::<super::instance::LuaInstance>()?.0;
            with_world(lua, |w| w.gui.turn_pages(id, at));
            Ok(())
        })?,
    )?;
    let run_service = with_world(lua, |w| w.dom.get_service("RunService").ok());
    if let Some(rs) = run_service {
        let heartbeat = super::instance::get_member(lua, rs, "Heartbeat")
            .map_err(|e| mlua::Error::RuntimeError(e.0))?
            .0;
        std.set("heartbeat", heartbeat)?;
        let tween: Table = lua
            .load(include_str!("luau/tween.luau"))
            .set_name("=[rook] Tween")
            .call(std.clone())?;
        std.set("tween", tween)?;
        let pages: Table = lua
            .load(include_str!("luau/page_layout.luau"))
            .set_name("=[rook] UIPageLayout")
            .call(std.clone())?;
        std.set("page_layout", pages)?;
    }
    let render: Table = lua
        .load(include_str!("luau/render_step.luau"))
        .set_name("=[rook] RenderStep")
        .call(std.clone())?;
    lua.set_named_registry_value("rook.render_step", render.get::<Function>("step")?)?;
    std.set("render_step", render)?;
    let actions: Table = lua
        .load(include_str!("luau/context_action.luau"))
        .set_name("=[rook] ContextActionService")
        .call(std.clone())?;
    lua.set_named_registry_value("rook.context_action", actions.get::<Function>("input")?)?;
    std.set("context_action", actions)?;
    Ok(())
}
