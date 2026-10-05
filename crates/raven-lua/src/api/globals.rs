//! Roblox globals: output, legacy and `task` scheduling, `typeof`,
//! `Instance.new`, `game`/`workspace`.
//!
//! The list follows creator-docs "Roblox globals" and the `task` library
//! page. Legacy timing details (minimum wait, spawn delay) are Inference.

use mlua::{Function, Lua, MultiValue, Table, Thread, Value as LuaValue};
use raven_scheduler::{LEGACY_MIN_WAIT, Queue};

use super::instance::push_instance;
use crate::args::{Args, display_value, type_name};
use crate::ctx::{ApiResult, Outcome, ScriptError, api_fn, api_ret, with_world};
use crate::threads::resume;
use crate::world::{Job, MessageType, WaitKind};

fn log(lua: &Lua, kind: MessageType, args: MultiValue) {
    let text = args
        .iter()
        .map(|v| display_value(lua, v))
        .collect::<Vec<_>>()
        .join(" ");
    with_world(lua, |w| w.push_log(kind, text));
}

fn now(lua: &Lua) -> f64 {
    with_world(lua, |w| w.scheduler.now())
}

/// `wait(t)`: legacy scheduler, at least [`LEGACY_MIN_WAIT`].
fn wait(lua: &Lua, args: MultiValue) -> ApiResult<Outcome> {
    let a = Args::new(args, "wait");
    let t = a.number_or(0, LEGACY_MIN_WAIT)?.max(LEGACY_MIN_WAIT);
    let thread = lua.current_thread();
    with_world(lua, |w| {
        let since = w.scheduler.now();
        w.scheduler.schedule(
            Queue::Legacy,
            t,
            Job::Resume {
                thread,
                args: Vec::new(),
                wait: WaitKind::Legacy { since },
            },
        );
    });
    Ok(Outcome::Yield)
}

/// `delay(t, f)`: call `f(elapsed, time())` after `t` seconds.
fn delay(lua: &Lua, args: MultiValue) -> ApiResult<Outcome> {
    let a = Args::new(args, "delay");
    let t = a.number_or(0, 0.0)?.max(LEGACY_MIN_WAIT);
    let func = a.function(1)?;
    with_world(lua, |w| {
        let since = w.scheduler.now();
        w.scheduler.schedule(
            Queue::Legacy,
            t,
            Job::Call {
                func,
                args: Vec::new(),
                legacy_since: Some(since),
            },
        );
    });
    Outcome::ret(lua, ())
}

/// `spawn(f)`: call `f(elapsed, time())` on the next legacy step.
fn legacy_spawn(lua: &Lua, args: MultiValue) -> ApiResult<Outcome> {
    let a = Args::new(args, "spawn");
    let func = a.function(0)?;
    with_world(lua, |w| {
        let since = w.scheduler.now();
        w.scheduler.schedule(
            Queue::Legacy,
            LEGACY_MIN_WAIT,
            Job::Call {
                func,
                args: Vec::new(),
                legacy_since: Some(since),
            },
        );
    });
    Outcome::ret(lua, ())
}

/// `task.*` accepts a function or a thread.
fn thread_arg(lua: &Lua, a: &Args, i: usize) -> ApiResult<Thread> {
    match a.get(i) {
        LuaValue::Function(f) => Ok(lua.create_thread(f.clone())?),
        LuaValue::Thread(t) => Ok(t.clone()),
        _ => Err(a.wrong(i, "function or thread")),
    }
}

fn task_wait(lua: &Lua, args: MultiValue) -> ApiResult<Outcome> {
    let a = Args::new(args, "wait");
    // Without a duration, resume on the next Heartbeat (creator-docs task.wait).
    let t = a.number_or(0, 0.0)?.max(0.0);
    let thread = lua.current_thread();
    with_world(lua, |w| {
        let since = w.scheduler.now();
        w.scheduler.schedule(
            Queue::Task,
            t,
            Job::Resume {
                thread,
                args: Vec::new(),
                wait: WaitKind::Task { since },
            },
        );
    });
    Ok(Outcome::Yield)
}

fn task_spawn(lua: &Lua, args: MultiValue) -> ApiResult<Outcome> {
    let a = Args::new(args, "spawn");
    let thread = thread_arg(lua, &a, 0)?;
    resume(lua, &thread, MultiValue::from_vec(a.rest(1)));
    Outcome::ret(lua, thread)
}

fn task_defer(lua: &Lua, args: MultiValue) -> ApiResult<Outcome> {
    let a = Args::new(args, "defer");
    let thread = thread_arg(lua, &a, 0)?;
    let job = Job::Resume {
        thread: thread.clone(),
        args: a.rest(1),
        wait: WaitKind::None,
    };
    with_world(lua, |w| w.scheduler.defer(job));
    Outcome::ret(lua, thread)
}

fn task_delay(lua: &Lua, args: MultiValue) -> ApiResult<Outcome> {
    let a = Args::new(args, "delay");
    let t = a.number_or(0, 0.0)?.max(0.0);
    let thread = thread_arg(lua, &a, 1)?;
    let job = Job::Resume {
        thread: thread.clone(),
        args: a.rest(2),
        wait: WaitKind::None,
    };
    with_world(lua, |w| w.scheduler.schedule(Queue::Task, t, job));
    Outcome::ret(lua, thread)
}

fn task_cancel(lua: &Lua, args: MultiValue) -> ApiResult<Outcome> {
    let a = Args::new(args, "cancel");
    let LuaValue::Thread(thread) = a.get(0).clone() else {
        return Err(a.wrong(0, "thread"));
    };
    with_world(lua, |w| {
        w.scheduler.cancel(|j| match j {
            Job::Resume { thread: t, .. } => *t == thread,
            _ => false,
        });
    });
    // Closing a suspended coroutine stops it for good.
    let co: Table = lua
        .named_registry_value::<Table>("raven.std")?
        .get("coroutine")?;
    let close: Function = co.get("close")?;
    let _ = close.call::<MultiValue>(thread);
    Outcome::ret(lua, ())
}

/// Roblox's `typeof`: datatype names for userdata and `Vector3` for vectors.
fn typeof_(lua: &Lua, args: MultiValue) -> ApiResult<String> {
    let v = args.into_iter().next().unwrap_or(LuaValue::Nil);
    let _ = lua;
    Ok(type_name(&v))
}

/// `Instance.new(className, parent)`. Parent is set last, after the
/// instance exists (the parent argument is discouraged but supported).
fn instance_new(lua: &Lua, args: MultiValue) -> ApiResult<Outcome> {
    let a = Args::new(args, "new");
    let class_name = a.string(0)?;
    let parent = a.opt_instance(1)?;
    let db = raven_reflection::database();
    let class = db
        .class_by_name(&class_name)
        .filter(|c| !c.flags.not_creatable && !c.flags.service)
        .ok_or_else(|| {
            ScriptError(format!(
                "Unable to create an Instance of type \"{class_name}\""
            ))
        })?;
    let id = with_world(lua, |w| w.dom.create(class.id));
    if let Some(p) = parent {
        super::methods::mutate(lua, |w| w.dom.set_parent(id, Some(p)))?;
    }
    Outcome::ret(lua, push_instance(lua, id)?)
}

fn instance_table(lua: &Lua) -> mlua::Result<Table> {
    let t = lua.create_table()?;
    t.set("new", api_fn(lua, instance_new)?)?;
    Ok(t)
}

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
            log(lua, MessageType::Output, args);
            Ok(())
        })?,
    )?;
    let warn = api_ret(lua, |lua, args| {
        log(lua, MessageType::Warning, args);
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
    lua.set_named_registry_value("raven.std", std)?;

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
    g.set(
        "version",
        lua.create_function(|_, ()| Ok(raven_reflection::roblox_version()))?,
    )?;
    g.set(
        "Version",
        lua.create_function(|_, ()| Ok(raven_reflection::roblox_version()))?,
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
    Ok(())
}
