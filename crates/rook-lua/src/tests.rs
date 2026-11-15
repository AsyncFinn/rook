use super::*;
use crate::runtime::scheduler::Queue;

/// Run `source` as a script, step `frames` 60 Hz frames, and return the
/// output lines (errors prefixed with `ERR `, warnings with `WARN `).
fn run(source: &str, frames: u32) -> Vec<String> {
    let rt = Runtime::new(Dom::new(), Clock::Fixed { step: 1.0 / 60.0 }).unwrap();
    rt.run_source("Workspace.Script", source, None).unwrap();
    finish(&rt, frames)
}

/// [`run`], as Roblox's own code: a script that came compiled.
fn run_core(source: &str, frames: u32) -> Vec<String> {
    let rt = Runtime::new(Dom::new(), Clock::Fixed { step: 1.0 / 60.0 }).unwrap();
    let script = rt.world_mut().dom.create_by_name("Script").unwrap();
    let code = mlua::chunk::Compiler::new().compile(source).unwrap();
    let core = Compiled {
        code: code.into(),
        core: true,
    };
    rt.world_mut().bytecode.insert(script, core);
    assert!(rt.run_script(script));
    finish(&rt, frames)
}

fn finish(rt: &Runtime, frames: u32) -> Vec<String> {
    for _ in 0..frames {
        step(rt);
    }
    rt.take_log()
        .into_iter()
        .map(|e| match e.kind {
            MessageType::Error => format!("ERR {}", e.message),
            MessageType::Warning => format!("WARN {}", e.message),
            _ => e.message,
        })
        .collect()
}

fn step(rt: &Runtime) {
    let dt = rt.world_mut().scheduler.begin_frame();
    rt.render_step(dt);
    rt.restyle();
    rt.resume_due(Queue::Legacy);
    let rs = rt.world().dom.find_service("RunService");
    if let Some(rs) = rs {
        rt.fire_event(rs, "Heartbeat", dt);
        rt.drain_deferred();
    }
    rt.resume_due(Queue::Task);
}

#[track_caller]
fn expect(source: &str, frames: u32, lines: &[&str]) {
    let out = run(source, frames);
    assert_eq!(out, lines, "\nsource:\n{source}");
}

mod core_gui;
mod datatypes;
mod descriptions;
mod errors;
mod gui;
mod instances;
mod members;
mod models;
mod object_arguments;
mod parts;
mod scheduling;
mod scripts;
mod tools;
mod tweens;
