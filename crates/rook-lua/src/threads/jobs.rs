//! Running what the scheduler queued: resumes, calls and Debris'
//! destroys, and the deferred queue.

use super::*;

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
