//! Running Luau threads: spawning and resuming them, skipping a stopped
//! script's, and the parts that build on that: reporting errors the way
//! Roblox's output does, running scheduled jobs, and firing signals.

mod errors;
mod firing;
mod jobs;
pub(crate) mod watchdog;

use crate::runtime::signal::{FireArgs, Firing, SignalKey};
use mlua::prelude::LuaThreadStatus as ThreadStatus;
use mlua::{Function, IntoLuaMulti, Lua, MultiValue, Table, Thread, Value as LuaValue};
use rook_dom::{Change, InstanceId};

use crate::api::instance::push_instance;
use crate::runtime::ctx::{lua_error_message, with_world};
use crate::runtime::world::{Handler, Job, MessageType, SignalBehavior, WaitKind};

pub(crate) use errors::report_error;
pub(crate) use firing::{dispatch, fire, fire_with};
#[allow(unused_imports, reason = "only profiling builds name their zones")]
pub(crate) use jobs::job_label;
pub(crate) use jobs::{drain_deferred, run_job};

/// Call `func` on a new thread right away. Errors go to the output.
/// Functions of a stopped script don't run.
pub(crate) fn spawn(lua: &Lua, func: Function, args: impl IntoLuaMulti) -> mlua::Result<Thread> {
    let stopped = stopped_function(lua, &func);
    let thread = lua.create_thread(func)?;
    if !stopped {
        resume(lua, &thread, args);
    }
    Ok(thread)
}

/// Resume a suspended thread. Threads that already finished or are
/// running are skipped, and so are a stopped script's.
pub(crate) fn resume(lua: &Lua, thread: &Thread, args: impl IntoLuaMulti) {
    if thread.status() != ThreadStatus::Resumable || stopped_thread(lua, thread) {
        return;
    }
    if let Err(e) = thread.resume::<MultiValue>(args) {
        report_error(lua, thread, &e);
    }
}

/// Whether `func` belongs to a stopped script.
fn stopped_function(lua: &Lua, func: &Function) -> bool {
    with_world(lua, |w| w.any_stopped)
        && func
            .environment()
            .is_some_and(|env| crate::api::scripts::is_stopped_env(lua, env))
}

/// A new thread that will call `func`, remembered so that it can be found
/// to belong to a stopped script before it has started (`task.delay`).
pub(crate) fn thread_for(lua: &Lua, func: Function) -> mlua::Result<Thread> {
    let thread = lua.create_thread(func.clone())?;
    lua.named_registry_value::<Table>("rook.thread_functions")?
        .raw_set(thread.clone(), func)?;
    Ok(thread)
}

/// Whether `thread` belongs to a stopped script: whether the function it
/// was made for does, or else the function at the bottom of its stack.
fn stopped_thread(lua: &Lua, thread: &Thread) -> bool {
    if !with_world(lua, |w| w.any_stopped) {
        return false;
    }
    let made_for = lua
        .named_registry_value::<Table>("rook.thread_functions")
        .and_then(|t| t.raw_get::<Option<Function>>(thread.clone()));
    if let Ok(Some(f)) = made_for {
        return stopped_function(lua, &f);
    }
    let base = luau_helper(
        lua,
        "thread_base",
        r#"
        local info = debug.info
        return function(co)
            local base
            for level = 0, 1000 do
                local f = info(co, level, "f")
                if f == nil then break end
                base = f
            end
            return base
        end
        "#,
    )
    .and_then(|f| f.call::<Option<Function>>(thread.clone()));
    matches!(base, Ok(Some(f)) if stopped_function(lua, &f))
}

/// The function a Luau chunk returns, loaded once and kept in the registry
/// under `rook.<name>`.
fn luau_helper(lua: &Lua, name: &str, source: &str) -> mlua::Result<Function> {
    let key = format!("rook.{name}");
    if let Some(f) = lua.named_registry_value::<Option<Function>>(&key)? {
        return Ok(f);
    }
    let f: Function = lua
        .load(source)
        .set_name(format!("=[rook] {name}"))
        .call(())?;
    lua.set_named_registry_value(&key, f.clone())?;
    Ok(f)
}

fn run_handler(lua: &Lua, handler: Handler, args: Vec<LuaValue>) {
    match handler {
        Handler::Call(f) => {
            if let Err(e) = spawn(lua, f, MultiValue::from_vec(args)) {
                with_world(lua, |w| {
                    w.push_log(MessageType::Error, lua_error_message(&e))
                });
            }
        }
        Handler::Resume(thread) => resume(lua, &thread, MultiValue::from_vec(args)),
    }
}
