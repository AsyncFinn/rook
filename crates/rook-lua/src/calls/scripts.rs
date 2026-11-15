//! Running scripts and Luau source, firing events, resuming threads and
//! handing out script-side values.

use mlua::IntoLuaMulti;
use rook_dom::InstanceId;

use crate::Runtime;
use crate::runtime::ctx::ScriptError;
use crate::runtime::scheduler::Queue;
use crate::runtime::signal::SignalKey;

impl Runtime {
    /// Start a Script. Returns false if it was already started or didn't compile.
    pub fn run_script(&self, script: InstanceId) -> bool {
        self.guarded(|| {
            let started = crate::api::scripts::run_script(&self.lua, script);
            crate::runtime::threads::drain_deferred(&self.lua);
            started
        })
    }

    /// Stop a started Script so it can run again. Returns false if it wasn't
    /// running.
    pub fn stop_script(&self, script: InstanceId) -> bool {
        crate::api::scripts::stop_script(&self.lua, script)
    }

    pub fn is_started(&self, script: InstanceId) -> bool {
        self.world().started.contains_key(&script)
    }

    /// Whether a script runs where it is now, enabled. One that's
    /// disabled or leaves stops as the change is dispatched.
    pub fn runs_here(&self, script: InstanceId) -> bool {
        let w = self.world();
        crate::api::scripts::runs_here(&w.dom, script, w.is_client)
    }

    /// Run Luau source on a new thread, as a script at `script` if given.
    pub fn run_source(
        &self,
        chunk_name: &str,
        source: &str,
        script: Option<InstanceId>,
    ) -> Result<(), ScriptError> {
        self.guarded(|| {
            let r = crate::api::scripts::run_source(&self.lua, chunk_name, source, script);
            crate::runtime::threads::drain_deferred(&self.lua);
            r
        })
        .map_err(ScriptError::from)
    }

    /// Run Luau source on a new thread in an environment kept under `name`
    /// between calls (the command bar's: its globals persist).
    pub fn run_source_in(
        &self,
        name: &str,
        chunk_name: &str,
        source: &str,
    ) -> Result<(), ScriptError> {
        self.guarded(|| {
            let r = crate::api::scripts::run_source_in(&self.lua, name, chunk_name, source);
            crate::runtime::threads::drain_deferred(&self.lua);
            r
        })
        .map_err(ScriptError::from)
    }

    /// Fire an instance's event with arguments (for engine events such as
    /// `RunService.Heartbeat` or `Players.PlayerAdded`). Arguments Luau
    /// can't hold (out of memory) leave the event unfired, with a warning.
    pub fn fire_event(&self, instance: InstanceId, event: &'static str, args: impl IntoLuaMulti) {
        profiling::scope!("fire_event");
        let args = match args.into_lua_multi(&self.lua) {
            Ok(args) => args.into_vec(),
            Err(e) => {
                tracing::warn!("{event} not fired: {e}");
                return;
            }
        };
        self.guarded(|| {
            crate::runtime::threads::fire(&self.lua, &SignalKey::event(instance, event), &args)
        });
    }

    /// Fire signals for journal entries recorded since `start` by code
    /// outside Lua (engine systems).
    pub fn dispatch_since(&self, start: u64) {
        profiling::scope!("dispatch_since");
        self.guarded(|| crate::runtime::threads::dispatch(&self.lua, start));
    }

    /// `Instance:Destroy()` from the engine (fallen parts), firing the
    /// signals it causes.
    pub fn destroy(&self, id: InstanceId) {
        // A part a script destroyed first has nothing left to destroy.
        self.guarded(|| {
            let _ = crate::runtime::removal::destroy(&self.lua, id);
        });
    }

    /// Resume due jobs on one queue, then drain the deferred queue.
    pub fn resume_due(&self, queue: Queue) {
        profiling::scope!("resume_due");
        let jobs = self.world_mut().scheduler.take_due(queue);
        self.guarded(|| {
            // Deferred handlers run when the thread that caused them yields
            // or finishes, so drain after every resumed job.
            for job in jobs {
                {
                    profiling::scope!(&*crate::runtime::threads::job_label(&self.lua, &job));
                    crate::runtime::threads::run_job(&self.lua, job);
                }
                profiling::scope!("drain_deferred");
                crate::runtime::threads::drain_deferred(&self.lua);
            }
        });
    }

    /// Drain the deferred queue (a resumption point).
    pub fn drain_deferred(&self) {
        profiling::scope!("drain_deferred");
        self.guarded(|| crate::runtime::threads::drain_deferred(&self.lua));
    }

    /// The script-side handle for an instance.
    pub fn push_instance(&self, id: InstanceId) -> mlua::Result<mlua::AnyUserData> {
        crate::api::instance::push_instance(&self.lua, id)
    }

    /// The script-side `Enum.<enum_name>` item with this value.
    pub fn push_enum_item(&self, enum_name: &str, value: u32) -> mlua::Result<mlua::Value> {
        crate::types::enums::enum_item(&self.lua, enum_name, value)
    }
}
