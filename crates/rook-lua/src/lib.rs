//! Luau scripting for Rook.
//!
//! A [`Runtime`] is one DataModel's Luau VM plus its [`World`] (DOM,
//! signals, scheduler, output). Scripts see the Roblox API: Instance
//! userdata, datatypes, `game`, `wait`/`task`, signals and `require`.
//! See `docs/architecture.md` §7.

mod api;
mod args;
mod convert;
mod ctx;
mod threads;
mod types;
mod world;

pub use ctx::{ScriptError, lua_error_message};
pub use world::{Handler, Job, LogEntry, MessageType, SignalBehavior, WaitKind, World};

use std::cell::{Cell, Ref, RefCell, RefMut};
use std::rc::Rc;
use std::time::{Duration, Instant};

use mlua::{IntoLuaMulti, Lua, Value as LuaValue, VmState};
use rook_dom::{Dom, InstanceId};
use rook_scheduler::{Clock, Queue};
use rook_signal::SignalKey;

/// Roblox Studio's default script timeout: a script that runs this long
/// without yielding is stopped with an error.
pub const DEFAULT_SCRIPT_TIMEOUT: Duration = Duration::from_secs(10);

/// Deadline for the code currently running, checked by the VM interrupt.
#[derive(Debug, Default)]
struct Watchdog {
    deadline: Cell<Option<Instant>>,
    timeout: Cell<Duration>,
}

/// One DataModel's scripting state.
#[derive(Debug)]
pub struct Runtime {
    lua: Lua,
    world: Rc<RefCell<World>>,
    watchdog: Rc<Watchdog>,
}

impl Runtime {
    pub fn new(dom: Dom, clock: Clock) -> mlua::Result<Self> {
        let lua = Lua::new();
        let world = Rc::new(RefCell::new(World::new(dom, clock)));
        ctx::Ctx::install(&lua, world.clone())?;

        let debug: mlua::Table = lua.globals().get("debug")?;
        lua.set_named_registry_value("rook.traceback", debug.get::<mlua::Function>("traceback")?)?;

        types::install(&lua)?;
        api::install(&lua)?;

        let watchdog = Rc::new(Watchdog {
            deadline: Cell::new(None),
            timeout: Cell::new(DEFAULT_SCRIPT_TIMEOUT),
        });
        let wd = watchdog.clone();
        lua.set_interrupt(move |_| {
            if let Some(deadline) = wd.deadline.get()
                && Instant::now() > deadline
            {
                // Reset so the error itself can unwind.
                wd.deadline.set(None);
                return Err(mlua::Error::RuntimeError(
                    "Script timeout: exhausted allowed execution time".into(),
                ));
            }
            Ok(VmState::Continue)
        });

        Ok(Self {
            lua,
            world,
            watchdog,
        })
    }

    pub fn lua(&self) -> &Lua {
        &self.lua
    }

    pub fn world(&self) -> Ref<'_, World> {
        self.world.borrow()
    }

    pub fn world_mut(&self) -> RefMut<'_, World> {
        self.world.borrow_mut()
    }

    pub fn set_script_timeout(&self, timeout: Duration) {
        self.watchdog.timeout.set(timeout);
    }

    fn guarded<R>(&self, f: impl FnOnce() -> R) -> R {
        self.watchdog
            .deadline
            .set(Some(Instant::now() + self.watchdog.timeout.get()));
        let r = f();
        self.watchdog.deadline.set(None);
        r
    }

    /// Start a Script. Returns false if it was already started or didn't compile.
    pub fn run_script(&self, script: InstanceId) -> bool {
        self.guarded(|| api::scripts::run_script(&self.lua, script))
    }

    /// Whether a script has been started.
    pub fn is_started(&self, script: InstanceId) -> bool {
        self.world().started.contains_key(&script)
    }

    /// Run Luau source on a new thread, as a script at `script` if given.
    pub fn run_source(
        &self,
        chunk_name: &str,
        source: &str,
        script: Option<InstanceId>,
    ) -> Result<(), ScriptError> {
        self.guarded(|| api::scripts::run_source(&self.lua, chunk_name, source, script))
            .map_err(ScriptError::from)
    }

    /// Fire an instance's event with arguments (for engine events such as
    /// `RunService.Heartbeat` or `Players.PlayerAdded`).
    pub fn fire_event(
        &self,
        instance: InstanceId,
        event: &'static str,
        args: impl IntoLuaMulti,
    ) -> mlua::Result<()> {
        let args: Vec<LuaValue> = args.into_lua_multi(&self.lua)?.into_vec();
        self.guarded(|| threads::fire(&self.lua, &SignalKey::event(instance, event), args));
        Ok(())
    }

    /// Fire signals for journal entries recorded since `start` by code
    /// outside Lua (engine systems).
    pub fn dispatch_since(&self, start: u64) {
        self.guarded(|| threads::dispatch(&self.lua, start));
    }

    /// Resume due jobs on one queue, then drain the deferred queue.
    pub fn resume_due(&self, queue: Queue) {
        let jobs = self.world_mut().scheduler.take_due(queue);
        self.guarded(|| {
            for job in jobs {
                threads::run_job(&self.lua, job);
            }
            threads::drain_deferred(&self.lua);
        });
    }

    /// Drain the deferred queue (a resumption point).
    pub fn drain_deferred(&self) {
        self.guarded(|| threads::drain_deferred(&self.lua));
    }

    /// Take everything logged so far.
    pub fn take_log(&self) -> Vec<LogEntry> {
        std::mem::take(&mut self.world_mut().log)
    }
}

#[cfg(test)]
mod tests;
