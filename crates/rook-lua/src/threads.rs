//! Running Lua threads, reporting their errors, and dispatching signals.

pub(crate) mod watchdog;

use crate::signal::{FireArgs, Firing, SignalKey};
use mlua::prelude::LuaThreadStatus as ThreadStatus;
use mlua::{Function, IntoLuaMulti, Lua, MultiValue, Table, Thread, Value as LuaValue};
use rook_dom::{Change, InstanceId};

use crate::api::instance::push_instance;
use crate::ctx::{lua_error_message, with_world};
use crate::world::{Handler, Job, MessageType, SignalBehavior, WaitKind};

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

/// Print an error the way Roblox's output does:
///
/// ```text
/// Workspace.Script:3: attempt to index nil with 'X'
/// Stack Begin
/// Script 'Workspace.Script', Line 3
/// Stack End
/// ```
pub(crate) fn report_error(lua: &Lua, thread: &Thread, e: &mlua::Error) {
    let message = lua_error_message(e);
    let mut text = message;
    let stack = mlua_traceback(e)
        .map(|tb| parse_traceback(&tb))
        .filter(|l| !l.is_empty())
        .or_else(|| stack_lines(lua, thread));
    if let Some(stack) = stack {
        text.push_str("\nStack Begin");
        for line in stack {
            text.push('\n');
            text.push_str(&line);
        }
        text.push_str("\nStack End");
    }
    let source = error_source(&text);
    with_world(lua, |w| {
        w.errors += 1;
        w.push_log_from(MessageType::Error, text, source);
    });
}

/// `Workspace.Script:3: message` → `Script:3`; otherwise the first stack line.
fn error_source(text: &str) -> Option<String> {
    let first = text.lines().next()?;
    if let Some((loc, _)) = first.split_once(": ")
        && let Some((chunk, line)) = loc.rsplit_once(':')
        && let Ok(line) = line.parse()
    {
        return Some(crate::world::short_source(chunk, line));
    }
    let frame = text.lines().find(|l| l.starts_with("Script '"))?;
    let chunk = frame.strip_prefix("Script '")?.split('\'').next()?;
    let line = frame
        .rsplit_once("Line ")?
        .1
        .split(' ')
        .next()?
        .parse()
        .ok()?;
    Some(crate::world::short_source(chunk, line))
}

/// The traceback mlua attached to an error, if any.
fn mlua_traceback(e: &mlua::Error) -> Option<String> {
    match e {
        mlua::Error::RuntimeError(s) => s
            .split_once(
                "
stack traceback:",
            )
            .map(|(_, tb)| tb.to_owned()),
        mlua::Error::CallbackError { traceback, cause } => {
            mlua_traceback(cause).or_else(|| (!traceback.is_empty()).then(|| traceback.clone()))
        }
        mlua::Error::WithContext { cause, .. } => mlua_traceback(cause),
        _ => None,
    }
}

/// Parse Lua-style traceback lines (`chunk:3: in function 'f'`) into
/// Roblox's `Script 'chunk', Line 3 - function f`. Engine frames are dropped.
fn parse_traceback(tb: &str) -> Vec<String> {
    tb.lines()
        .filter_map(|line| {
            let line = line.trim();
            let (location, rest) = line.split_once(": in ")?;
            let (chunk, number) = location.rsplit_once(':')?;
            if chunk.starts_with('[') {
                return None;
            }
            let number: u32 = number.parse().ok()?;
            let suffix = rest
                .strip_prefix("function '")
                .and_then(|f| f.strip_suffix('\''))
                .map(|f| format!(" - function {f}"))
                .unwrap_or_default();
            Some(format!("Script '{chunk}', Line {number}{suffix}"))
        })
        .collect()
}

/// Turn Luau's `debug.traceback(thread)` into Roblox's stack lines.
fn stack_lines(lua: &Lua, thread: &Thread) -> Option<Vec<String>> {
    let traceback: Function = lua.named_registry_value("rook.traceback").ok()?;
    let tb: String = traceback.call(thread.clone()).ok()?;
    let lines: Vec<String> = tb
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            // "Workspace.Script:3 function foo" or "Workspace.Script:3"
            let (location, rest) = line.split_once(' ').unwrap_or((line, ""));
            let (chunk, number) = location.rsplit_once(':')?;
            if chunk.starts_with('[') {
                return None;
            }
            let number: u32 = number.parse().ok()?;
            let suffix = rest
                .strip_prefix("function ")
                .map(|f| format!(" - function {f}"))
                .unwrap_or_default();
            Some(format!("Script '{chunk}', Line {number}{suffix}"))
        })
        .collect();
    (!lines.is_empty()).then_some(lines)
}

// jobs

/// Where a job's code lives (`script: Workspace.Script:12`), naming its
/// profiler zone. Only evaluated in profiling builds.
#[allow(dead_code)]
pub(crate) fn job_label(lua: &Lua, job: &Job) -> String {
    let function_label = |f: &Function| {
        let info = f.info();
        format!(
            "script: {}:{}",
            info.short_src.unwrap_or_default(),
            info.line_defined.unwrap_or(0)
        )
    };
    match job {
        Job::Resume { thread, .. }
        | Job::Fire {
            handler: Handler::Resume(thread),
            ..
        } => thread_label(lua, thread),
        Job::Call { func, .. }
        | Job::Fire {
            handler: Handler::Call(func),
            ..
        } => function_label(func),
        Job::Destroy(_) => "Debris destroy".into(),
    }
}

/// The innermost script frame of a suspended thread.
fn thread_label(lua: &Lua, thread: &Thread) -> String {
    let find = luau_helper(
        lua,
        "thread_label",
        r#"
        local info = debug.info
        return function(co)
            for level = 0, 30 do
                local s, l = info(co, level, "sl")
                if s == nil then break end
                if s ~= "[C]" and string.sub(s, 1, 6) ~= "[rook]" then
                    return s .. ":" .. l
                end
            end
            return "?"
        end
        "#,
    )
    .and_then(|f| f.call::<String>(thread.clone()));
    format!("script: {}", find.unwrap_or_else(|_| "?".into()))
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

/// Run one scheduled job.
pub(crate) fn run_job(lua: &Lua, job: Job) {
    match job {
        Job::Resume { thread, args, wait } => {
            let now = with_world(lua, |w| w.scheduler.now());
            match wait {
                WaitKind::Legacy { since } => resume(lua, &thread, (now - since, now)),
                WaitKind::Task { since } => resume(lua, &thread, now - since),
                WaitKind::None => resume(lua, &thread, MultiValue::from_vec(args)),
            }
        }
        Job::Call {
            func,
            args,
            legacy_since,
        } => {
            let result = match legacy_since {
                Some(since) => {
                    let now = with_world(lua, |w| w.scheduler.now());
                    spawn(lua, func, (now - since, now))
                }
                None => spawn(lua, func, MultiValue::from_vec(args)),
            };
            if let Err(e) = result {
                with_world(lua, |w| {
                    w.push_log(MessageType::Error, lua_error_message(&e))
                });
            }
        }
        Job::Destroy(id) => {
            // Debris may outlive what it was given; destroyed is destroyed.
            let _ = crate::removal::destroy(lua, id);
        }
        Job::Fire { handler, args, .. } => run_handler(lua, handler, args),
    }
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

/// Drain the deferred queue, including jobs deferred while draining.
pub(crate) fn drain_deferred(lua: &Lua) {
    // Roblox stops after a bounded number of nested defers; Rook bounds the
    // drain so a self-deferring loop can't hang the frame.
    for _ in 0..100_000 {
        let Some(job) = with_world(lua, |w| w.scheduler.pop_deferred()) else {
            return;
        };
        run_job(lua, job);
    }
}

// signals

/// Fire a signal with already-converted arguments.
pub(crate) fn fire(lua: &Lua, key: &SignalKey, args: &[LuaValue]) {
    let (handlers, behavior) =
        with_world(lua, |w| (w.signals.take_handlers(key), w.signal_behavior));
    for (connection, handler) in handlers {
        // A stopped script's connections are disconnected.
        if let Handler::Call(f) = &handler
            && stopped_function(lua, f)
        {
            with_world(lua, |w| w.signals.disconnect(connection));
            continue;
        }
        match behavior {
            SignalBehavior::Deferred => with_world(lua, |w| {
                w.scheduler.defer(Job::Fire {
                    connection,
                    handler,
                    args: args.to_vec(),
                });
            }),
            SignalBehavior::Immediate => {
                let entered = with_world(lua, |w| w.signals.enter());
                if let Err(e) = entered {
                    with_world(lua, |w| w.push_log(MessageType::Error, e.to_string()));
                    continue;
                }
                run_handler(lua, handler, args.to_vec());
                with_world(lua, |w| w.signals.leave());
            }
        }
    }
}

/// Fire the signals for every journal entry from `start` to the journal's
/// end as of this call. Entries added by handlers are dispatched by the
/// mutation that added them, so Immediate handlers nest the way Roblox's do.
pub(crate) fn dispatch(lua: &Lua, start: u64) {
    let end = with_world(lua, |w| w.dom.journal().end());
    let mut destroyed: Vec<InstanceId> = Vec::new();
    for pos in start..end {
        let firings = with_world(lua, |w| {
            let change = w.dom.journal().get(pos)?;
            if let Change::Destroyed(id) = change {
                destroyed.push(*id);
            }
            if w.signals.is_empty() {
                return None;
            }
            Some(crate::signal::expand(change, &w.signals))
        });
        for firing in firings.into_iter().flatten() {
            if let Some(args) = firing_args(lua, &firing) {
                fire(lua, &firing.key, &args);
            }
        }
    }
    if !destroyed.is_empty() {
        with_world(lua, |w| {
            for id in destroyed {
                w.signals.disconnect_instance(id);
            }
        });
    }
}

/// Convert a firing's arguments. `None` means the firing doesn't apply
/// (ValueBase `Changed` only fires for `Value`).
fn firing_args(lua: &Lua, firing: &Firing) -> Option<Vec<LuaValue>> {
    let inst = |id| push_instance(lua, id).ok().map(LuaValue::UserData);
    Some(match &firing.args {
        FireArgs::None => Vec::new(),
        FireArgs::Instance(id) => vec![inst(*id)?],
        FireArgs::Ancestry { child, parent } => vec![
            inst(*child)?,
            parent.and_then(inst).unwrap_or(LuaValue::Nil),
        ],
        FireArgs::Attribute(name) => vec![LuaValue::String(lua.create_string(name).ok()?)],
        FireArgs::Property(pid) => {
            let id = firing.key.instance;
            let (is_value_base, name) = with_world(lua, |w| {
                let db = w.dom.database();
                (w.dom.is_a(id, "ValueBase"), db.property(*pid).name)
            });
            if is_value_base {
                // Confirmed: creator-docs ValueBase classes: Changed fires
                // with the new value, and only for Value.
                if name != "Value" {
                    return None;
                }
                vec![crate::api::instance::read_property(lua, id, name).ok()?]
            } else {
                vec![LuaValue::String(lua.create_string(name).ok()?)]
            }
        }
    })
}
