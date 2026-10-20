//! Waiting and threads: the legacy `wait`, `delay` and `spawn`, and the
//! `task` library. Legacy timing details (minimum wait, spawn delay) are
//! Inference.

use super::*;

pub(super) fn now(lua: &Lua) -> f64 {
    with_world(lua, |w| w.scheduler.now())
}

/// `wait(t)`: legacy scheduler, at least [`LEGACY_MIN_WAIT`].
pub(super) fn wait(lua: &Lua, args: MultiValue) -> ApiResult<Outcome> {
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
pub(super) fn delay(lua: &Lua, args: MultiValue) -> ApiResult<Outcome> {
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
pub(super) fn legacy_spawn(lua: &Lua, args: MultiValue) -> ApiResult<Outcome> {
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
pub(super) fn thread_arg(lua: &Lua, a: &Args, i: usize) -> ApiResult<Thread> {
    match a.get(i) {
        LuaValue::Function(f) => Ok(crate::threads::thread_for(lua, f.clone())?),
        LuaValue::Thread(t) => Ok(t.clone()),
        _ => Err(a.wrong(i, "function or thread")),
    }
}

pub(super) fn task_wait(lua: &Lua, args: MultiValue) -> ApiResult<Outcome> {
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

pub(super) fn task_spawn(lua: &Lua, args: MultiValue) -> ApiResult<Outcome> {
    let a = Args::new(args, "spawn");
    let thread = thread_arg(lua, &a, 0)?;
    resume(lua, &thread, MultiValue::from_vec(a.rest(1)));
    Outcome::ret(lua, thread)
}

pub(super) fn task_defer(lua: &Lua, args: MultiValue) -> ApiResult<Outcome> {
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

pub(super) fn task_delay(lua: &Lua, args: MultiValue) -> ApiResult<Outcome> {
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

pub(super) fn task_cancel(lua: &Lua, args: MultiValue) -> ApiResult<Outcome> {
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
        .named_registry_value::<Table>("rook.std")?
        .get("coroutine")?;
    let close: Function = co.get("close")?;
    // `coroutine.close` returns the thread's error, which the closing
    // script doesn't see.
    let _ = close.call::<MultiValue>(thread);
    Outcome::ret(lua, ())
}
