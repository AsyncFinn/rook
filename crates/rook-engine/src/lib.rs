//! Assembles a DataModel: loads a place, creates the services scripts
//! expect, starts server Scripts, and runs the frame loop.
//!
//! Every DataModel has a physics world (`rook-physics`, kept in the
//! scripting world so script queries reach it). Servers simulate; clients
//! only query theirs. See `docs/design/architecture.md` §6 for the frame order.

mod bridges;
pub mod console;
pub mod data;
mod frame;
pub mod logging;
mod scripts;
pub mod session;
pub mod settings;

pub use bridges::input::{ClickEvent, ClickKind, InputEvent};

pub use bridges::render_changes::RenderChanges;
pub use bridges::sounds::{Emitter, Listener as SoundListener, PlayingSound};
pub use session::Session;

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use rook_dom::{Change, Dom, InstanceId};
use rook_lua::{LogEntry, Runtime};
use rook_scheduler::{Clock, Queue};
use rook_types::Value;

pub use rook_lua::MessageType;

use bridges::{input, sounds, unions};
use session::threads;

#[derive(Debug, thiserror::Error)]
pub enum EngineError {
    #[error(transparent)]
    Load(#[from] rook_formats::LoadError),
    #[error("Luau setup failed: {0}")]
    Lua(String),
}

/// Services every place gets, created up front so scripts find them in
/// `game` as they would on Roblox.
const DEFAULT_SERVICES: &[&str] = &[
    "Workspace",
    "Players",
    "Lighting",
    "ReplicatedFirst",
    "ReplicatedStorage",
    "ServerScriptService",
    "ServerStorage",
    "StarterGui",
    "StarterPack",
    "StarterPlayer",
    "Teams",
    "SoundService",
    "Chat",
    "RunService",
    "Debris",
];

/// Options for a DataModel host.
#[derive(Debug, Clone)]
pub struct Options {
    /// Frames per second of the fixed clock.
    pub hz: f64,
    /// Script timeout (Roblox Studio's default is 10 seconds).
    pub script_timeout: Duration,
    /// Luau heap limit in bytes.
    pub memory_limit: usize,
    /// Where content requests (`ContentProvider:PreloadAsync`) resolve.
    /// `None` uses `settings::content_config(false)`: the install,
    /// content folders and the cache, with asset delivery off unless
    /// `ROOK_ASSET_DELIVERY=on` or `settings.json` turns it on. Shared by
    /// a session's DataModels.
    pub content: Option<Arc<rook_content::Resolver>>,
    /// Finish loading sound files within the frame that asks for them, so
    /// simulated time can't run ahead of loading and runs repeat exactly
    /// (headless runs, conformance, tests). The player loads in the
    /// background instead, as Roblox does.
    pub wait_for_sounds: bool,
}

impl Default for Options {
    fn default() -> Self {
        Self {
            hz: 60.0,
            script_timeout: rook_lua::DEFAULT_SCRIPT_TIMEOUT,
            memory_limit: 1 << 30,
            content: None,
            wait_for_sounds: true,
        }
    }
}

/// Which side of a game a DataModel is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Server,
    Client,
}

/// A server DataModel (the usual case, and what headless runs use).
pub type Server = DataModel;

/// One DataModel with its scripts: a server or a client.
#[derive(Debug)]
pub struct DataModel {
    runtime: Runtime,
    role: Role,
    /// Trim the journal at the end of each frame. A session turns this off
    /// for the server so the replicator can read the frame's changes first.
    auto_trim: bool,
    run_service: InstanceId,
    /// Journal position up to which new scripts have been looked for.
    script_cursor: u64,
    /// Journal position up to which render-relevant changes have been checked.
    render_cursor: u64,
    /// What a renderer draws that changed since it last asked.
    render_changes: RenderChanges,
    /// Keyboard and mouse bookkeeping (clients).
    input: input::InputTracker,
    content: Content,
    sounds: sounds::Sounds,
    unions: unions::Unions,
}

/// Content requests from scripts, resolved off the engine thread.
#[derive(Debug, Default)]
struct Content {
    resolver: Option<Arc<rook_content::Resolver>>,
    /// Made on the first request, so places that never ask cost nothing.
    loader: Option<rook_content::Loader>,
    /// Loader tickets to the scripts' request ids.
    tickets: HashMap<rook_content::Ticket, u64>,
    /// Loader tickets to the request ids of assets scripts load
    /// (`require(assetId)`, `InsertService:LoadAsset`).
    assets: HashMap<rook_content::Ticket, u64>,
    wait_for_sounds: bool,
}

impl DataModel {
    /// An empty place.
    pub fn new(options: &Options) -> Result<Self, EngineError> {
        let mut dom = Dom::new();
        dom.journal_mut().set_enabled(false);
        ensure_terrain(&mut dom);
        Self::with_dom(dom, options)
    }

    /// An empty client DataModel, filled by replication.
    pub fn new_client(options: &Options) -> Result<Self, EngineError> {
        let mut dm = Self::with_dom(Dom::new(), options)?;
        dm.role = Role::Client;
        let mut w = dm.runtime.world_mut();
        w.is_client = true;
        // Only the server simulates for now: it owns every part (network
        // ownership comes with characters). The client's world answers its
        // LocalScripts' and the mouse's queries.
        w.physics = rook_physics::Physics::for_queries();
        drop(w);
        Ok(dm)
    }

    pub fn role(&self) -> Role {
        self.role
    }

    pub fn set_auto_trim(&mut self, on: bool) {
        self.auto_trim = on;
    }

    /// Treat everything in the journal so far as already handled (after
    /// building the DataModel by hand, as a session does for clients).
    pub fn reset_cursors(&mut self) {
        let end = self.runtime.world().dom.journal().end();
        self.script_cursor = end;
        self.render_cursor = end;
        self.render_changes = RenderChanges::everything();
        self.sounds.rescan(end);
        self.unions.rescan(end);
    }

    /// Drop journal entries every reader has seen. Physics catches up first
    /// so changes made after its step (by Heartbeat, or by remotes routed
    /// between frames) aren't lost. A world no query has built yet (a
    /// client's) reads the whole DOM when it is first built instead.
    pub fn trim_journal(&mut self) {
        let mut w = self.runtime.world_mut();
        let w = &mut *w;
        if w.physics.is_built() {
            w.physics.sync(&mut w.dom);
        }
        let end = w.dom.journal().end();
        w.dom.journal_mut().trim(end);
    }

    /// Load a place file.
    pub fn load_place(bytes: &[u8], options: &Options) -> Result<Self, EngineError> {
        let loaded = rook_formats::load(bytes)?;
        let mut dom = Dom::new();
        let root = dom.root();
        rook_formats::into_dom(&loaded.dom, &mut dom, root);
        let duplicate_ids = has_duplicate_unique_ids(&dom);
        // Part of loading the place, not changes for anyone to replay.
        let journaling = dom.journal().is_enabled();
        dom.journal_mut().set_enabled(false);
        rook_physics::adopt_saved_velocities(&mut dom);
        ensure_terrain(&mut dom);
        if rook_physics::joints::surfaces::joins_on_load(&dom) {
            let parts = rook_physics::workspace_parts(&dom);
            let _ = rook_physics::joints::surfaces::make_joints(&mut dom, &parts);
        }
        dom.journal_mut().set_enabled(journaling);
        let engine = Self::with_dom(dom, options)?;
        if duplicate_ids {
            // Studio's Output, word for word (observed in Studio 0.743 on a
            // place with two instances saved under one UniqueId).
            engine
                .runtime
                .world_mut()
                .push_log(MessageType::Error, "DM contains duplicate Unique ids".into());
        }
        Ok(engine)
    }

    fn with_dom(mut dom: Dom, options: &Options) -> Result<Self, EngineError> {
        threads::size_threads();
        dom.journal_mut().set_enabled(false);
        for service in DEFAULT_SERVICES {
            // A class missing from the database would be a Rook bug, not a
            // problem with the place; skip it rather than fail.
            let _ = dom.get_service(service);
        }
        ensure_camera(&mut dom);
        dom.journal_mut().set_enabled(true);
        let run_service = dom.get_service("RunService").expect("RunService class");
        // Workspace.SignalBehavior: Default currently means Immediate
        // (creator-docs SignalBehavior); new template places save Deferred.
        // AncestryDeferred is treated as Deferred (Approximation).
        let signal_behavior = match dom
            .find_service("Workspace")
            .and_then(|w| dom.property_by_name(w, "SignalBehavior"))
        {
            Some(Value::Enum(2 | 3)) => rook_lua::SignalBehavior::Deferred,
            _ => rook_lua::SignalBehavior::Immediate,
        };
        let loadstring_enabled = dom
            .find_service("ServerScriptService")
            .and_then(|s| dom.property_by_name(s, "LoadStringEnabled"))
            == Some(Value::Bool(true));

        let runtime = Runtime::new(
            dom,
            Clock::Fixed {
                step: 1.0 / options.hz,
            },
        )
        .map_err(|e| EngineError::Lua(e.to_string()))?;
        runtime.set_script_timeout(options.script_timeout);
        runtime.world_mut().signal_behavior = signal_behavior;
        let _ = runtime.lua().set_memory_limit(options.memory_limit);
        if !loadstring_enabled {
            // Confirmed: creator-docs ServerScriptService.LoadStringEnabled.
            let _ = runtime
                .lua()
                .globals()
                .raw_set("loadstring", mlua::Value::Nil);
        }
        let script_cursor = runtime.world().dom.journal().end();
        Ok(Self {
            runtime,
            role: Role::Server,
            auto_trim: true,
            run_service,
            script_cursor,
            render_cursor: script_cursor,
            render_changes: RenderChanges::everything(),
            input: Default::default(),
            content: Content {
                resolver: options.content.clone(),
                wait_for_sounds: options.wait_for_sounds,
                ..Content::default()
            },
            sounds: Default::default(),
            unions: Default::default(),
        })
    }

    pub fn runtime(&self) -> &Runtime {
        &self.runtime
    }

    /// What changed for a renderer since the last call, including changes
    /// made between frames (by the caller).
    pub fn take_render_changes(&mut self) -> RenderChanges {
        self.check_render_changes();
        std::mem::take(&mut self.render_changes)
    }

    /// `Workspace.CurrentCamera`.
    pub fn current_camera(&self) -> Option<InstanceId> {
        let world = self.runtime.world();
        world
            .dom
            .ref_property(world.dom.find_service("Workspace")?, "CurrentCamera")
    }

    /// Set a property from the host (an editor camera, input) and fire the
    /// signals scripts connected to it.
    pub fn set_property(&self, id: InstanceId, name: &str, value: Value) {
        let start = {
            let mut w = self.runtime.world_mut();
            let start = w.dom.journal().end();
            let _ = w.dom.set_property_by_name(id, name, value);
            start
        };
        self.runtime.dispatch_since(start);
    }

    fn fire(&self, instance: InstanceId, event: &'static str, args: impl mlua::IntoLuaMulti) {
        let _ = self.runtime.fire_event(instance, event, args);
        self.runtime.drain_deferred();
    }

    /// Run a Luau file given on the command line (`--script`) as if it were
    /// a Script in ServerScriptService. Its output names it `RookScript`.
    pub fn run_extra_script(&self, source: &str) -> Result<(), String> {
        self.runtime
            .run_source("ServerScriptService.RookScript", source, None)
            .map_err(|e| e.to_string())
    }

    /// Run frames until `seconds` of game time have passed.
    pub fn run_for(&mut self, seconds: f64) {
        let target = self.runtime.world().scheduler.now() + seconds;
        while self.runtime.world().scheduler.now() + 1e-9 < target {
            self.step();
        }
    }

    pub fn now(&self) -> f64 {
        self.runtime.world().scheduler.now()
    }

    pub fn take_log(&self) -> Vec<LogEntry> {
        self.runtime.take_log()
    }

    /// Scripts that have been started.
    pub fn started_scripts(&self) -> usize {
        self.runtime.world().started_count()
    }
}

/// Workspace always has a current camera: Roblox creates one if the place
/// was saved without (Inference: observed behaviour, not documented).
fn ensure_camera(dom: &mut Dom) {
    let Ok(ws) = dom.get_service("Workspace") else {
        return;
    };
    if dom
        .ref_property(ws, "CurrentCamera")
        .is_some_and(|c| dom.get(c).is_some())
    {
        return;
    }
    let Ok(camera) = dom.create_by_name("Camera") else {
        return;
    };
    let _ = dom.set_name(camera, "Camera".into());
    dom.attach_unchecked(camera, ws);
    if let Some(p) = dom.property_id(ws, "CurrentCamera") {
        dom.load_property(ws, p, Value::Ref(Some(camera.to_ref())));
    }
}

/// Whether two instances were saved with the same `UniqueId` (nil ones,
/// which old files and new instances have, don't count).
fn has_duplicate_unique_ids(dom: &Dom) -> bool {
    let mut seen = std::collections::HashSet::new();
    dom.descendants(dom.root()).into_iter().any(|id| {
        matches!(
            dom.property_by_name(id, "UniqueId"),
            Some(Value::UniqueId(u)) if !u.is_nil() && !seen.insert(u)
        )
    })
}

/// Workspace always has a Terrain, as in Roblox, where `workspace.Terrain`
/// is never nil (Inference: every place Studio saves has one, and a new
/// place starts with one). Clients get the server's by replication.
fn ensure_terrain(dom: &mut Dom) {
    let Ok(ws) = dom.get_service("Workspace") else {
        return;
    };
    if dom.find_first_child_of_class(ws, "Terrain").is_some() {
        return;
    }
    let Ok(terrain) = dom.create_by_name("Terrain") else {
        return;
    };
    let _ = dom.set_name(terrain, "Terrain".into());
    dom.attach_unchecked(terrain, ws);
}

#[cfg(test)]
mod tests;
