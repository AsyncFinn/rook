//! Luau scripting for Rook.
//!
//! A [`Runtime`] is one DataModel's Luau VM plus its [`World`] (DOM,
//! signals, scheduler, output). Scripts see the Roblox API: Instance
//! userdata, datatypes, `game`, `wait`/`task`, signals and `require`.
//! See `docs/design/architecture.md#luau-bridge-rook-lua`.

mod api;
pub mod audio_api;
mod ctx;
mod net;
mod removal;
pub mod scheduler;
mod signal;
mod styles;
mod threads;
mod types;
pub mod user_settings;
mod world;

use threads::watchdog;

pub use api::methods::assets::AssetFill;
pub use api::methods::camera::{Lens, viewport_ray};
pub use api::methods::characters::movement::drives as humanoid_drives;
pub use api::methods::characters::ownership::authority;
pub use api::methods::characters::{
    Avatar, BuildRig, RigSource, RigType, Scales, character_root, humanoid_of_root, humanoid_root,
    root_part,
};
pub use api::methods::web::{WebAnswer, WebRequest};
pub use net::{NetValue, RemoteKind, RemoteMessage};
pub use world::{
    Compiled, EffectCall, Listener, LogEntry, MessageType, SignalBehavior, SoundControl,
};

use ctx::ScriptError;
use world::World;

use std::cell::{Cell, Ref, RefCell, RefMut};
use std::rc::Rc;
use std::time::Duration;

use crate::scheduler::{Clock, Queue};
use crate::signal::SignalKey;
use mlua::{IntoLuaMulti, Lua};
use rook_dom::{Dom, InstanceId};
use rook_types::{UserInputState, UserInputType};

/// Create an `InputObject` describing one input (an `Enum.KeyCode` value;
/// position and delta in pixels). Used by the engine when firing input events and by
/// UserInputService queries.
pub fn input_object(
    dom: &mut Dom,
    key_code: u32,
    input_type: UserInputType,
    state: UserInputState,
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
    set(dom, "UserInputType", input_type.into());
    set(dom, "UserInputState", state.into());
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
    /// The heap size in bytes past which running code stops (0: none).
    heap_limit: Cell<usize>,
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
            heap_limit: Cell::new(0),
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

    /// Stop code that grows the Luau heap past `bytes` (0: no limit)
    /// with Luau's out-of-memory error.
    pub fn set_heap_limit(&self, bytes: usize) {
        self.heap_limit.set(bytes);
    }

    fn guarded<R>(&self, f: impl FnOnce() -> R) -> R {
        watchdog::guarded(self.timeout.get(), self.heap_limit.get(), f)
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
        self.guarded(|| threads::fire(&self.lua, &SignalKey::event(instance, event), &args));
    }

    /// Fire `UserInputService.PointerAction(wheel, pan, pinch,
    /// gameProcessedEvent)`: what the default camera zooms by.
    pub fn pointer_action(&self, wheel: f32, pan: (f32, f32), pinch: f32, processed: bool) {
        let Some(uis) = self.world().dom.find_service("UserInputService") else {
            return;
        };
        let pan = types::LVector2(rook_types::Vector2::new(pan.0, pan.1));
        self.fire_event(uis, "PointerAction", (wheel, pan, pinch, processed));
    }

    /// Move interpolating cameras and keep a Scriptable camera's Focus
    /// (clients, each frame after RenderStepped).
    pub fn step_cameras(&self) {
        self.guarded(|| {
            api::methods::camera_moves::step(&self.lua);
            threads::drain_deferred(&self.lua);
        });
    }

    /// Read the journal up to its end, before it is trimmed: names scripts
    /// read are dropped for the instances renamed in it.
    pub fn note_changes(&self) {
        if let Err(e) = api::instance::note_renames(&ctx::ctx(&self.lua)) {
            let message = format!("dropping renamed instances' names: {e}");
            self.world
                .borrow_mut()
                .push_log(MessageType::Error, message);
        }
    }

    /// Fire signals for journal entries recorded since `start` by code
    /// outside Lua (engine systems).
    pub fn dispatch_since(&self, start: u64) {
        profiling::scope!("dispatch_since");
        self.guarded(|| threads::dispatch(&self.lua, start));
    }

    /// Run what scripts bound with `RunService:BindToRenderStep`, by
    /// priority (clients, each frame before RenderStepped).
    pub fn render_step(&self, dt: f64) {
        let Ok(step) = self
            .lua
            .named_registry_value::<mlua::Function>("rook.render_step")
        else {
            return;
        };
        self.guarded(|| {
            // Each binding runs on its own thread and reports its own errors.
            let _ = step.call::<()>(dt);
            threads::drain_deferred(&self.lua);
        });
    }

    /// Offer an InputObject GUI didn't take to ContextActionService's
    /// bindings. Returns whether one took it.
    pub fn context_action(&self, object: InstanceId) -> bool {
        let Ok(offer) = self
            .lua
            .named_registry_value::<mlua::Function>("rook.context_action")
        else {
            return false;
        };
        self.guarded(|| {
            let taken = api::instance::push_instance(&self.lua, object)
                .and_then(|obj| offer.call::<bool>(obj))
                .unwrap_or_else(|e| {
                    tracing::warn!("ContextActionService: {e}");
                    false
                });
            threads::drain_deferred(&self.lua);
            taken
        })
    }

    /// Apply StyleSheets to what changed since the last call.
    pub fn restyle(&self) {
        let start = self.world().dom.journal().end();
        let styled = self.guarded(|| styles::restyle(&self.lua));
        if let Err(e) = styled {
            tracing::warn!("styling failed: {e}");
        }
        self.dispatch_since(start);
    }

    /// Lay GUI out again if it changed, as Roblox does each frame after
    /// RenderStepped, firing Changed for what moved.
    pub fn sync_gui(&self) {
        let start = {
            let mut w = self.world_mut();
            let w = &mut *w;
            let start = w.dom.journal().end();
            w.gui.sync(&mut w.dom);
            let physics = &mut w.physics;
            w.gui.sync_overheads(&mut w.dom, |dom, from, to, ignore| {
                let filter = rook_physics::Filter::legacy_ray(ignore.to_vec(), false);
                physics.raycast(dom, from, to - from, &filter).is_some()
            });
            start
        };
        self.dispatch_since(start);
    }

    /// Move players whose team left Teams or changed color, firing what
    /// that fires (once a frame, before scripts resume).
    pub fn keep_teams(&self) {
        let start = {
            let mut w = self.world_mut();
            let start = w.dom.journal().end();
            if let Err(e) = api::methods::characters::spawning::keep_teams(&mut w.dom) {
                tracing::warn!("keeping teams failed: {e}");
            }
            start
        };
        self.dispatch_since(start);
    }

    /// Give TextBox `id` focus as clicking it does, its caret before
    /// character `caret` (`None`: at the end), firing what that fires.
    pub fn focus_text_box(&self, id: InstanceId, caret: Option<usize>) {
        let focused = self.guarded(|| api::methods::textbox::focus(&self.lua, id, caret));
        if let Err(e) = focused {
            tracing::warn!("focusing a TextBox failed: {e}");
        }
    }

    /// Take focus from the focused TextBox, as a key or a click elsewhere
    /// does: `FocusLost(enter_pressed, cause)`, then
    /// `TextBoxFocusReleased`.
    pub fn release_text_box(&self, enter_pressed: bool, cause: Option<InstanceId>) {
        let released = self.guarded(|| {
            let cause = match cause {
                Some(id) => mlua::Value::UserData(api::instance::push_instance(&self.lua, id)?),
                None => mlua::Value::Nil,
            };
            api::methods::textbox::release(&self.lua, enter_pressed, cause, false)
        });
        if let Err(e) = released {
            tracing::warn!("releasing a TextBox failed: {e}");
        }
    }

    /// The TextBox with focus, if it's still there.
    pub fn focused_text_box(&self) -> Option<InstanceId> {
        api::methods::textbox::focused(&self.lua)
    }

    /// `Instance:Destroy()` from the engine (fallen parts), firing the
    /// signals it causes.
    pub fn destroy(&self, id: InstanceId) {
        // A part a script destroyed first has nothing left to destroy.
        self.guarded(|| {
            let _ = removal::destroy(&self.lua, id);
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

    /// A player joined: it goes on an auto-assignable team, and its look
    /// is asked of the avatar API. Returns whether the look is on its way
    /// (its first spawn waits for it: `World::avatars_pending`).
    pub fn welcome_player(&self, player: InstanceId) -> bool {
        let start = self.world().dom.journal().end();
        {
            let mut w = self.world_mut();
            if let Err(e) = api::methods::characters::spawning::assign_team(&mut w.dom, player) {
                w.push_log(MessageType::Error, format!("assigning a team: {e}"));
            }
        }
        self.dispatch_since(start);
        api::methods::characters::request_avatar(&self.lua, player)
    }

    /// A part touched a SpawnLocation (team changes on touch).
    pub fn spawn_touched(&self, spawn: InstanceId, part: InstanceId) {
        let start = self.world().dom.journal().end();
        {
            let mut w = self.world_mut();
            let result = api::methods::characters::spawning::spawn_touched(&mut w.dom, spawn, part);
            if let Err(e) = result {
                w.push_log(MessageType::Error, format!("changing team on touch: {e}"));
            }
        }
        self.dispatch_since(start);
    }

    /// The Humanoid controller stepped: its reports' property changes and
    /// state events.
    pub fn humanoids_stepped(&self, reports: Vec<rook_physics::humanoid::Report>) {
        if reports.is_empty() {
            return;
        }
        let result =
            self.guarded(|| api::methods::characters::movement::stepped(&self.lua, reports));
        if let Err(e) = result {
            tracing::warn!("Humanoid events: {e}");
        }
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

    /// Tools the client's scripts moved between its player's Backpack and
    /// character since the last call, with where each went.
    pub fn take_tool_moves(&self) -> Vec<(InstanceId, InstanceId)> {
        std::mem::take(&mut self.world_mut().tool_moves)
    }

    /// On the server, repeat a Tool move `player`'s client made, if the
    /// Tool is that player's.
    pub fn repeat_tool_move(&self, player: InstanceId, tool: InstanceId, parent: InstanceId) {
        self.guarded(|| api::methods::tools::repeat_move(&self.lua, player, tool, parent));
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
        // Chat messages travel as TextChatService's own firings.
        let chat = self.world().dom.class_name(remote) == "TextChatService";
        self.guarded(|| {
            if chat {
                let handler = if self.world().is_client {
                    "onClient"
                } else {
                    "onServer"
                };
                let api = lua
                    .named_registry_value::<mlua::Table>("rook.std")
                    .and_then(|std| std.get::<mlua::Table>("text_chat"))
                    .and_then(|api| api.get::<mlua::Function>(handler));
                if let Ok(f) = api {
                    // Errors are the thread's to report.
                    let _ = threads::spawn(lua, f, mlua::MultiValue::from_vec(values));
                }
            } else {
                threads::fire(lua, &SignalKey::event(remote, event), &values);
            }
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
            // The thread reports its own errors; starting it fails only
            // out of memory, which Luau has reported already.
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
            api::methods::preload::deliver(&self.lua, results);
            threads::drain_deferred(&self.lua);
        });
    }

    /// Assets `require(assetId)` and `InsertService:LoadAsset` wait for
    /// (`(request, asset id)`), asked for since the last call.
    pub fn take_asset_requests(&self) -> Vec<(u64, i64)> {
        std::mem::take(&mut self.world_mut().assets.outbox)
    }

    /// Deliver a fetched asset: `fill` puts its instances in the Model the
    /// waiting call gets, and the threads waiting for it resume.
    pub fn deliver_asset(&self, request: u64, fill: AssetFill) {
        self.guarded(|| {
            api::methods::assets::deliver(&self.lua, request, fill);
            threads::drain_deferred(&self.lua);
        });
    }

    /// Where scripts' web requests go (`rook_platform::endpoints`); set
    /// before scripts run.
    pub fn set_endpoints(&self, endpoints: std::sync::Arc<rook_platform::endpoints::Endpoints>) {
        self.world_mut().web.endpoints = endpoints;
    }

    /// Web requests members wait on, by request id, asked for
    /// since the last call.
    pub fn take_web_requests(&self) -> Vec<(u64, WebRequest)> {
        std::mem::take(&mut self.world_mut().web.outbox)
    }

    /// Deliver a web request's answer: the member waiting on it returns.
    pub fn deliver_web(&self, request: u64, answer: WebAnswer) {
        self.guarded(|| {
            api::methods::web::deliver(&self.lua, request, answer);
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
