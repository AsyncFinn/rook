//! Luau scripting for Rook.
//!
//! A [`Runtime`] is one DataModel's Luau VM plus its [`World`] (DOM,
//! signals, scheduler, output). Scripts see the Roblox API: Instance
//! userdata, datatypes, `game`, `wait`/`task`, signals and `require`.
//! See `docs/design/architecture.md` §7.

mod api;
pub mod audio;
mod ctx;
mod net;
mod threads;
mod types;
mod world;

use threads::watchdog;

pub use api::camera::{Lens, viewport_ray};
pub use ctx::{ScriptError, lua_error_message};
pub use net::{NetValue, RemoteKind, RemoteMessage};
pub use world::{
    AudioState, Handler, InputState, Job, Listener, LogEntry, MessageType, Scheduled,
    SignalBehavior, SoundControl, WaitKind, World,
};

use std::cell::{Cell, Ref, RefCell, RefMut};
use std::rc::Rc;
use std::time::Duration;

use mlua::{IntoLuaMulti, Lua, Value as LuaValue};
use rook_dom::{Dom, InstanceId};
use rook_scheduler::{Clock, Queue};
use rook_signal::SignalKey;

/// Create an `InputObject` describing one input (`Enum.KeyCode`,
/// `Enum.UserInputType` and `Enum.UserInputState` values; position and
/// delta in pixels). Used by the engine when firing input events and by
/// UserInputService queries.
pub fn input_object(
    dom: &mut Dom,
    key_code: u32,
    input_type: u32,
    state: u32,
    position: (f32, f32),
    delta: (f32, f32),
) -> Option<InstanceId> {
    use rook_types::{Value, Vector3};
    let id = dom.create_by_name("InputObject").ok()?;
    let class = dom.get(id)?.class;
    let db = dom.database();
    let set = |dom: &mut Dom, name: &str, value: Value| {
        if let Some(p) = db.find_property(class, name) {
            dom.load_property(id, p.id, value);
        }
    };
    set(dom, "KeyCode", Value::Enum(key_code));
    set(dom, "UserInputType", Value::Enum(input_type));
    set(dom, "UserInputState", Value::Enum(state));
    set(
        dom,
        "Position",
        Value::Vector3(Vector3::new(position.0, position.1, 0.0)),
    );
    set(
        dom,
        "Delta",
        Value::Vector3(Vector3::new(delta.0, delta.1, 0.0)),
    );
    Some(id)
}

/// Roblox Studio's default script timeout: a script that runs this long
/// without yielding is stopped with an error.
pub const DEFAULT_SCRIPT_TIMEOUT: Duration = Duration::from_secs(10);

/// One DataModel's scripting state.
#[derive(Debug)]
pub struct Runtime {
    lua: Lua,
    world: Rc<RefCell<World>>,
    /// How long code may run without yielding.
    timeout: Cell<Duration>,
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

        watchdog::install(&lua)?;

        Ok(Self {
            lua,
            world,
            timeout: Cell::new(DEFAULT_SCRIPT_TIMEOUT),
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
        self.timeout.set(timeout);
    }

    fn guarded<R>(&self, f: impl FnOnce() -> R) -> R {
        watchdog::guarded(self.timeout.get(), f)
    }

    /// Start a Script. Returns false if it was already started or didn't compile.
    pub fn run_script(&self, script: InstanceId) -> bool {
        self.guarded(|| {
            let started = api::scripts::run_script(&self.lua, script);
            threads::drain_deferred(&self.lua);
            started
        })
    }

    /// Stop a started Script so it can run again. Returns false if it wasn't
    /// running.
    pub fn stop_script(&self, script: InstanceId) -> bool {
        api::scripts::stop_script(&self.lua, script)
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
        self.guarded(|| {
            let r = api::scripts::run_source(&self.lua, chunk_name, source, script);
            threads::drain_deferred(&self.lua);
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
            let r = api::scripts::run_source_in(&self.lua, name, chunk_name, source);
            threads::drain_deferred(&self.lua);
            r
        })
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
        profiling::scope!("fire_event");
        let args: Vec<LuaValue> = args.into_lua_multi(&self.lua)?.into_vec();
        self.guarded(|| threads::fire(&self.lua, &SignalKey::event(instance, event), args));
        Ok(())
    }

    /// Fire signals for journal entries recorded since `start` by code
    /// outside Lua (engine systems).
    pub fn dispatch_since(&self, start: u64) {
        profiling::scope!("dispatch_since");
        self.guarded(|| threads::dispatch(&self.lua, start));
    }

    /// `Instance:Destroy()` from the engine (fallen parts), firing the
    /// signals it causes.
    pub fn destroy(&self, id: InstanceId) {
        let start = self.world().dom.journal().end();
        let _ = self.world_mut().dom.destroy(id);
        self.dispatch_since(start);
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
                    profiling::scope!(&*threads::job_label(&self.lua, &job));
                    threads::run_job(&self.lua, job);
                }
                profiling::scope!("drain_deferred");
                threads::drain_deferred(&self.lua);
            }
        });
    }

    /// Spawn a player's character (`Player:LoadCharacter()`), firing the
    /// signals it causes.
    pub fn load_character(&self, player: InstanceId) -> Result<InstanceId, ScriptError> {
        self.guarded(|| {
            let r = api::methods::spawn_character(&self.lua, player);
            threads::drain_deferred(&self.lua);
            r
        })
    }

    /// Drain the deferred queue (a resumption point).
    pub fn drain_deferred(&self) {
        profiling::scope!("drain_deferred");
        self.guarded(|| threads::drain_deferred(&self.lua));
    }

    /// The script-side handle for an instance.
    pub fn push_instance(&self, id: InstanceId) -> mlua::Result<mlua::AnyUserData> {
        api::instance::push_instance(&self.lua, id)
    }

    /// Remote calls made since the last call, for the session to deliver.
    pub fn take_outbox(&self) -> Vec<RemoteMessage> {
        std::mem::take(&mut self.world_mut().outbox)
    }

    /// Deliver a RemoteEvent firing: `OnServerEvent(player, ...)` on a
    /// server, `OnClientEvent(...)` on a client.
    pub fn deliver_event(&self, remote: InstanceId, player: Option<InstanceId>, args: &[NetValue]) {
        let event = if self.world().is_client {
            "OnClientEvent"
        } else {
            "OnServerEvent"
        };
        let lua = &self.lua;
        let mut values = Vec::with_capacity(args.len() + 1);
        if let Some(p) = player {
            match api::instance::push_instance(lua, p) {
                Ok(ud) => values.push(mlua::Value::UserData(ud)),
                Err(_) => return,
            }
        }
        for a in args {
            values.push(net::from_net(lua, a).unwrap_or(mlua::Value::Nil));
        }
        self.guarded(|| {
            threads::fire(lua, &SignalKey::event(remote, event), values);
            threads::drain_deferred(lua);
        });
    }

    /// Run an incoming invoke against the remote's callback; the reply goes
    /// to this DataModel's outbox.
    pub fn deliver_invoke(
        &self,
        remote: InstanceId,
        player: Option<InstanceId>,
        call: u64,
        args: &[NetValue],
    ) {
        let lua = &self.lua;
        let callback = if self.world().is_client {
            "OnClientInvoke"
        } else {
            "OnServerInvoke"
        };
        let Ok(runner) = lua.named_registry_value::<mlua::Function>("rook.remote_runner") else {
            return;
        };
        let Ok(remote_ud) = api::instance::push_instance(lua, remote) else {
            return;
        };
        let mut values = vec![
            mlua::Value::UserData(remote_ud),
            mlua::Value::String(lua.create_string(callback).expect("short string")),
            mlua::Value::Number(call as f64),
        ];
        if let Some(p) = player
            && let Ok(ud) = api::instance::push_instance(lua, p)
        {
            values.push(mlua::Value::UserData(ud));
        }
        for a in args {
            values.push(net::from_net(lua, a).unwrap_or(mlua::Value::Nil));
        }
        self.guarded(|| {
            let _ = threads::spawn(lua, runner, mlua::MultiValue::from_vec(values));
            threads::drain_deferred(lua);
        });
    }

    /// Resume the thread waiting on `call` with `(ok, results)`.
    pub fn deliver_reply(&self, call: u64, ok: bool, args: &[NetValue]) {
        let lua = &self.lua;
        let Some(thread) = self.world_mut().pending_calls.remove(&call) else {
            return;
        };
        let packed = (|| -> mlua::Result<mlua::Value> {
            if !ok {
                return net::from_net(lua, args.first().unwrap_or(&NetValue::Nil));
            }
            let t = lua.create_table()?;
            for (i, a) in args.iter().enumerate() {
                t.raw_set(i + 1, net::from_net(lua, a)?)?;
            }
            t.raw_set("n", args.len())?;
            Ok(mlua::Value::Table(t))
        })()
        .unwrap_or(mlua::Value::Nil);
        self.guarded(|| {
            threads::resume(lua, &thread, (ok, packed));
            threads::drain_deferred(lua);
        });
    }

    /// ContentProvider references to resolve, by request id, asked for
    /// since the last call, with what each must load as.
    pub fn take_content_requests(
        &self,
    ) -> Vec<(u64, rook_content::ContentRef, rook_content::Want)> {
        std::mem::take(&mut self.world_mut().content.outbox)
    }

    /// ContentProvider requests answered without resolving anything
    /// (`(request, succeeded)`), to deliver with the next results.
    pub fn take_content_answers(&self) -> Vec<(u64, bool)> {
        std::mem::take(&mut self.world_mut().content.answered)
    }

    /// Deliver finished content requests (`(request, succeeded)`): their
    /// PreloadAsync callbacks run, and finished PreloadAsync calls return.
    pub fn deliver_content(&self, results: &[(u64, bool)]) {
        if results.is_empty() {
            return;
        }
        self.guarded(|| {
            api::content::deliver(&self.lua, results);
            threads::drain_deferred(&self.lua);
        });
    }

    /// Take everything logged so far.
    pub fn take_log(&self) -> Vec<LogEntry> {
        std::mem::take(&mut self.world_mut().log)
    }
}

#[cfg(test)]
mod tests;
