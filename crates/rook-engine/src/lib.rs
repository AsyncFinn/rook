//! Assembles a DataModel: loads a place, creates the services scripts
//! expect, starts server Scripts, and runs the frame loop.
//!
//! Every DataModel has a physics world (`rook-physics`, kept in the
//! scripting world so script queries reach it). Servers simulate; clients
//! only query theirs. See `docs/architecture.md` §6 for the frame order.

pub mod console;
mod input;
#[cfg(feature = "repl")]
pub mod repl;
mod session;

pub use input::{ClickEvent, ClickKind, InputEvent};

pub use session::Session;

use std::time::Duration;

use rook_dom::{Change, Dom, InstanceId};
use rook_lua::{LogEntry, Runtime};
use rook_scheduler::{Clock, Queue};
use rook_types::Value;

pub use rook_lua::MessageType;

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
}

impl Default for Options {
    fn default() -> Self {
        Self {
            hz: 60.0,
            script_timeout: rook_lua::DEFAULT_SCRIPT_TIMEOUT,
            memory_limit: 1 << 30,
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
}

impl DataModel {
    /// An empty place.
    pub fn new(options: &Options) -> Result<Self, EngineError> {
        Self::with_dom(Dom::new(), options)
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
        // Part of loading the place, not changes for anyone to replay.
        let journaling = dom.journal().is_enabled();
        dom.journal_mut().set_enabled(false);
        rook_physics::adopt_saved_velocities(&mut dom);
        if rook_physics::surfaces::joins_on_load(&dom) {
            let parts = rook_physics::workspace_parts(&dom);
            let _ = rook_physics::surfaces::make_joints(&mut dom, &parts);
        }
        dom.journal_mut().set_enabled(journaling);
        Self::with_dom(dom, options)
    }

    fn with_dom(mut dom: Dom, options: &Options) -> Result<Self, EngineError> {
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
        })
    }

    pub fn runtime(&self) -> &Runtime {
        &self.runtime
    }

    /// Start every server Script that should run, in tree order.
    pub fn start(&mut self) {
        let root = self.runtime.world().dom.root();
        self.start_scripts_under(root);
        self.script_cursor = self.runtime.world().dom.journal().end();
    }

    fn start_scripts_under(&self, id: InstanceId) {
        let mut candidates = vec![id];
        candidates.extend(self.runtime.world().dom.descendants(id));
        for script in candidates {
            if self.should_run(script) {
                self.runtime.run_script(script);
            }
        }
    }

    /// Whether a script runs here.
    ///
    /// Server: legacy-context Scripts under Workspace or
    /// ServerScriptService (Confirmed: creator-docs Script); Server-context
    /// Scripts anywhere except ServerStorage and the Starter containers
    /// (Inference).
    ///
    /// Client: LocalScripts under the player's Backpack, character,
    /// PlayerGui or PlayerScripts, or under ReplicatedFirst (Confirmed:
    /// creator-docs LocalScript); Client-context Scripts anywhere the client
    /// sees except the Starter containers (Inference).
    fn should_run(&self, id: InstanceId) -> bool {
        let world = self.runtime.world();
        let dom = &world.dom;
        if self.runtime.is_started(id) {
            return false;
        }
        let class = dom.class_name(id);
        if !matches!(class, "Script" | "LocalScript") {
            return false;
        }
        if dom.property_by_name(id, "Disabled") == Some(Value::Bool(true)) {
            return false;
        }
        if self.role == Role::Client {
            return client_should_run(dom, id, class);
        }
        if class != "Script" {
            return false;
        }
        let Some(top) = top_level(dom, id) else {
            return false;
        };
        let top_class = dom.class_name(top);
        match dom.property_by_name(id, "RunContext") {
            Some(Value::Enum(1)) => !(is_starter(top_class) || top_class == "ServerStorage"),
            Some(Value::Enum(2 | 3)) => false,
            _ => matches!(top_class, "Workspace" | "ServerScriptService"),
        }
    }

    /// Start scripts that became runnable since the last check: moved into
    /// a running location or enabled.
    fn start_new_scripts(&mut self) {
        profiling::scope!("start_new_scripts");
        let end = self.runtime.world().dom.journal().end();
        let candidates: Vec<InstanceId> = {
            let world = self.runtime.world();
            let journal = world.dom.journal();
            let disabled = |id: InstanceId| world.dom.property_id(id, "Disabled");
            journal
                .since(self.script_cursor)
                .iter()
                .flat_map(|change| match change {
                    Change::Reparented { subtree, .. } => subtree.clone(),
                    Change::Property { id, property } if disabled(*id) == Some(*property) => {
                        vec![*id]
                    }
                    _ => Vec::new(),
                })
                .collect()
        };
        self.script_cursor = end;
        for id in candidates {
            if self.should_run(id) {
                self.runtime.run_script(id);
            }
        }
    }

    /// Run one frame.
    ///
    /// Order (Inference from the creator-docs task scheduler diagram):
    /// legacy resumption (`wait`, `spawn`, `delay`), Stepped/PreSimulation,
    /// [physics], PostSimulation, Heartbeat, `task` resumption.
    pub fn step(&mut self) {
        profiling::scope!("DataModel::step");
        let (now, dt) = {
            let mut w = self.runtime.world_mut();
            let dt = w.scheduler.begin_frame();
            (w.scheduler.now(), dt)
        };
        let rs = self.run_service;
        if self.role == Role::Client {
            // Before the frame is drawn (creator-docs RunService.PreRender;
            // RenderStepped is its older name).
            self.fire(rs, "RenderStepped", dt);
            self.fire(rs, "PreRender", dt);
        }
        self.runtime.resume_due(Queue::Legacy);
        self.fire(rs, "Stepped", (now, dt));
        self.fire(rs, "PreSimulation", dt);
        self.simulate(dt);
        self.fire(rs, "PostSimulation", dt);
        self.fire(rs, "Heartbeat", dt);
        self.runtime.resume_due(Queue::Task);
        self.start_new_scripts();
        self.check_render_changes();
        if self.auto_trim {
            self.trim_journal();
        }
    }

    /// Step physics and write the results into the DOM. Parts that fell
    /// below Workspace.FallenPartsDestroyHeight are destroyed.
    fn simulate(&mut self, dt: f64) {
        if self.role == Role::Client {
            return;
        }
        let (fallen, touches, reached) = {
            let mut w = self.runtime.world_mut();
            let w = &mut *w;
            w.physics.sync(&mut w.dom);
            w.physics.step(dt);
            let fallen = w.physics.write_back(&mut w.dom);
            (fallen, w.physics.take_touches(), w.physics.take_reached())
        };
        // Both parts fire, the lower id first (observed: the part made
        // first fired first in Studio; the real order is unknown).
        for t in touches {
            let event = if t.began { "Touched" } else { "TouchEnded" };
            for (part, other) in [(t.part, t.other), (t.other, t.part)] {
                if let Ok(other) = self.runtime.push_instance(other) {
                    let _ = self.runtime.fire_event(part, event, other);
                }
            }
        }
        for rocket in reached {
            let _ = self.runtime.fire_event(rocket, "ReachedTarget", ());
        }
        self.runtime.drain_deferred();
        for id in fallen {
            self.runtime.destroy(id);
        }
    }

    /// Note whether the journal since the last check touched anything drawn:
    /// parts, their ancestry, or Lighting. Camera moves don't count; the
    /// renderer reads the camera every frame anyway.
    fn check_render_changes(&mut self) {
        profiling::scope!("check_render_changes");
        let world = self.runtime.world();
        let dom = &world.dom;
        let lighting = dom.find_service("Lighting");
        let changes = &mut self.render_changes;
        for c in dom.journal().since(self.render_cursor) {
            match c {
                Change::Property { id, .. } => {
                    if Some(*id) == lighting {
                        changes.lighting = true;
                    } else if !changes.full && dom.is_a(*id, "BasePart") {
                        changes.parts.insert(*id);
                    }
                }
                Change::Reparented { subtree, .. } => {
                    if subtree.iter().any(|i| dom.is_a(*i, "BasePart")) {
                        changes.full = true;
                    }
                }
                Change::Destroying(id) | Change::Created(id) if dom.is_a(*id, "BasePart") => {
                    changes.full = true;
                }
                _ => {}
            }
        }
        if changes.full {
            changes.parts.clear();
        }
        self.render_cursor = dom.journal().end();
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

/// Changes a renderer needs to apply. When `full` is set (parts were
/// added, removed or moved between parents) the whole scene should be
/// rebuilt; otherwise only `parts` changed, plus Lighting if `lighting`.
#[derive(Debug, Clone, Default)]
pub struct RenderChanges {
    pub full: bool,
    pub lighting: bool,
    pub parts: std::collections::HashSet<InstanceId>,
}

impl RenderChanges {
    fn everything() -> Self {
        Self {
            full: true,
            lighting: true,
            parts: Default::default(),
        }
    }

    pub fn is_empty(&self) -> bool {
        !self.full && !self.lighting && self.parts.is_empty()
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

fn client_should_run(dom: &Dom, id: InstanceId, class: &str) -> bool {
    let local_player = dom.local_player();
    let character = local_player.and_then(|p| dom.ref_property(p, "Character"));
    let Some(top) = top_level(dom, id) else {
        return false;
    };
    let top_class = dom.class_name(top);
    if class == "LocalScript" {
        if top_class == "ReplicatedFirst" {
            return true;
        }
        return dom.ancestors(id).any(|a| {
            Some(a) == character
                || (matches!(
                    dom.class_name(a),
                    "Backpack" | "PlayerGui" | "PlayerScripts"
                ) && dom.parent(a) == local_player)
        });
    }
    matches!(dom.property_by_name(id, "RunContext"), Some(Value::Enum(2)))
        && !(is_starter(top_class) || matches!(top_class, "ServerStorage" | "ServerScriptService"))
}

/// Containers whose contents are templates copied to players, so scripts
/// in them never run in place.
fn is_starter(class: &str) -> bool {
    matches!(class, "StarterGui" | "StarterPack" | "StarterPlayer")
}

/// The child of the DataModel that contains `id`.
fn top_level(dom: &Dom, id: InstanceId) -> Option<InstanceId> {
    let root = dom.root();
    let mut cur = id;
    loop {
        let parent = dom.parent(cur)?;
        if parent == root {
            return Some(cur);
        }
        cur = parent;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn add_script(server: &Server, location: &str, name: &str, source: &str) {
        let parent = service(server, location);
        add_under(server, parent, "Script", name, source);
    }

    fn lines(server: &Server) -> Vec<String> {
        server.take_log().into_iter().map(|e| e.message).collect()
    }

    #[test]
    fn scripts_run_in_server_locations_only() {
        let mut s = Server::new(&Options::default()).unwrap();
        add_script(
            &s,
            "Workspace",
            "Script",
            "print('ws', script:GetFullName())",
        );
        add_script(&s, "ServerStorage", "Stored", "print('should not run')");
        s.start();
        assert_eq!(lines(&s), ["ws Workspace.Script"]);
    }

    #[test]
    fn heartbeat_fires_every_frame() {
        let mut s = Server::new(&Options::default()).unwrap();
        add_script(
            &s,
            "ServerScriptService",
            "Script",
            r#"
            local n = 0
            game:GetService("RunService").Heartbeat:Connect(function() n += 1 end)
            task.delay(0.5, function() print(n) end)
            "#,
        );
        s.start();
        s.run_for(1.0);
        assert_eq!(lines(&s), ["30"]);
    }

    fn add_under(
        server: &Server,
        parent: InstanceId,
        class: &str,
        name: &str,
        source: &str,
    ) -> InstanceId {
        let mut w = server.runtime.world_mut();
        let id = w.dom.create_by_name(class).unwrap();
        w.dom.set_name(id, name.into()).unwrap();
        if !source.is_empty() {
            w.dom
                .set_property_by_name(id, "Source", Value::String(source.into()))
                .unwrap();
        }
        w.dom.set_parent(id, Some(parent)).unwrap();
        id
    }

    fn service(server: &Server, name: &str) -> InstanceId {
        server.runtime.world_mut().dom.get_service(name).unwrap()
    }

    #[test]
    fn session_join_runs_local_scripts_where_roblox_does() {
        let mut server = Server::new(&Options::default()).unwrap();
        add_script(
            &server,
            "ServerScriptService",
            "Main",
            r#"
            game.Players.PlayerAdded:Connect(function(p)
                print("server: joined", p.Name, p.UserId, #p.PlayerGui:GetChildren())
            end)
            task.wait(0.2)
            local part = Instance.new("Part")
            part.Name = "Late"
            part.Parent = workspace
            "#,
        );
        let ss = service(&server, "ServerStorage");
        add_under(&server, ss, "Folder", "Secret", "");
        let gui = service(&server, "StarterGui");
        let screen = add_under(&server, gui, "ScreenGui", "Hud", "");
        add_under(
            &server,
            screen,
            "LocalScript",
            "HudScript",
            r#"
            local rs = game:GetService("RunService")
            print("gui:", game.Players.LocalPlayer.Name, rs:IsClient(), rs:IsServer(), script:GetFullName())
            print("gui: secret visible?", game:GetService("ServerStorage"):FindFirstChild("Secret") ~= nil)
            print("gui: late part", workspace:WaitForChild("Late").ClassName)
            "#,
        );
        let sp = service(&server, "StarterPlayer");
        let sps = server
            .runtime
            .world()
            .dom
            .find_first_child(sp, "StarterPlayerScripts", false);
        let sps = sps.unwrap_or_else(|| {
            add_under(
                &server,
                sp,
                "StarterPlayerScripts",
                "StarterPlayerScripts",
                "",
            )
        });
        add_under(
            &server,
            sps,
            "LocalScript",
            "Control",
            "print('player scripts:', script.Parent.ClassName)",
        );
        let rf = service(&server, "ReplicatedFirst");
        add_under(
            &server,
            rf,
            "LocalScript",
            "Loading",
            "print('replicated first')",
        );
        let ws = service(&server, "Workspace");
        add_under(
            &server,
            ws,
            "LocalScript",
            "Nope",
            "print('workspace local script must not run')",
        );
        server.start();

        let mut session = Session::new(server, Options::default());
        let index = session.join("Builderman", 156).unwrap();
        session.run_for(1.0);

        let server_log: Vec<String> = session
            .server()
            .take_log()
            .into_iter()
            .map(|e| e.message)
            .collect();
        assert_eq!(server_log, ["server: joined Builderman 156 1"]);
        let mut client_log: Vec<String> = session
            .client(index)
            .unwrap()
            .take_log()
            .into_iter()
            .map(|e| e.message)
            .collect();
        client_log.sort();
        assert_eq!(
            client_log,
            [
                "gui: Builderman true false Players.Builderman.PlayerGui.Hud.HudScript",
                "gui: late part Part",
                "gui: secret visible? false",
                "player scripts: PlayerScripts",
                "replicated first",
            ]
        );
    }

    #[test]
    fn remote_events_and_functions_cross_the_session() {
        let mut server = Server::new(&Options::default()).unwrap();
        let rs = service(&server, "ReplicatedStorage");
        add_under(&server, rs, "RemoteEvent", "Ping", "");
        add_under(&server, rs, "RemoteFunction", "Ask", "");
        let ws = service(&server, "Workspace");
        add_under(&server, ws, "Part", "Target", "");
        add_script(
            &server,
            "ServerScriptService",
            "Main",
            r#"
            local rs = game:GetService("ReplicatedStorage")
            rs.Ping.OnServerEvent:Connect(function(player, data, part, fn)
                print("server got", player.Name, data.a, data.list[2], part == workspace.Target, fn)
                rs.Ping:FireClient(player, "pong", workspace.Target)
            end)
            rs.Ask.OnServerInvoke = function(player, n)
                if n < 0 then error("negative!") end
                task.wait(0.1)
                return n * 2, player.Name
            end
            print(pcall(function() rs.Ping:FireServer() end))
            "#,
        );
        let sp = service(&server, "StarterPlayer");
        let sps = add_under(
            &server,
            sp,
            "StarterPlayerScripts",
            "StarterPlayerScripts",
            "",
        );
        add_under(
            &server,
            sps,
            "LocalScript",
            "Client",
            r#"
            local rs = game:GetService("ReplicatedStorage")
            rs.Ping.OnClientEvent:Connect(function(word, part)
                print("client got", word, part == workspace.Target)
            end)
            rs.Ping:FireServer({ a = 1, list = { 10, 20 } }, workspace.Target, function() end)
            print("invoke", rs.Ask:InvokeServer(21))
            print("invoke error", pcall(function() return rs.Ask:InvokeServer(-1) end))
            "#,
        );
        server.start();
        let mut session = Session::new(server, Options::default());
        let index = session.join("Player1", 1).unwrap();
        session.run_for(1.0);
        let server_log: Vec<String> = session
            .server()
            .take_log()
            .into_iter()
            .map(|e| e.message)
            .collect();
        let client_log: Vec<String> = session
            .client(index)
            .unwrap()
            .take_log()
            .into_iter()
            .map(|e| e.message)
            .collect();
        assert_eq!(
            server_log,
            [
                "false FireServer can only be called from the client",
                "server got Player1 1 20 true nil",
            ]
        );
        assert_eq!(
            client_log,
            [
                "client got pong true",
                "invoke 42 Player1",
                "invoke error false ServerScriptService.Main:8: negative!",
            ]
        );
    }

    #[test]
    fn input_mouse_and_click_detectors() {
        let mut server = Server::new(&Options::default()).unwrap();
        let ws = service(&server, "Workspace");
        let button = add_under(&server, ws, "Part", "Button", "");
        {
            let mut w = server.runtime.world_mut();
            let cf = rook_types::CFrame::from_position(glam::Vec3::new(0.0, 0.0, -10.0));
            w.dom
                .set_property_by_name(button, "CFrame", Value::CFrame(cf))
                .unwrap();
            w.dom
                .set_property_by_name(
                    button,
                    "Size",
                    Value::Vector3(rook_types::Vector3::new(4.0, 4.0, 1.0)),
                )
                .unwrap();
        }
        add_under(&server, button, "ClickDetector", "ClickDetector", "");
        add_script(
            &server,
            "ServerScriptService",
            "Main",
            "workspace.Button.ClickDetector.MouseClick:Connect(function(p) print('server click', p.Name) end)",
        );
        let sp = service(&server, "StarterPlayer");
        let sps = add_under(
            &server,
            sp,
            "StarterPlayerScripts",
            "StarterPlayerScripts",
            "",
        );
        add_under(
            &server,
            sps,
            "LocalScript",
            "Input",
            r#"
            local uis = game:GetService("UserInputService")
            local mouse = game.Players.LocalPlayer:GetMouse()
            uis.InputBegan:Connect(function(input, processed)
                print("began", input.UserInputType.Name, input.KeyCode.Name, processed)
            end)
            uis.InputEnded:Connect(function(input)
                print("ended", input.UserInputType.Name, input.KeyCode.Name, input.UserInputState.Name)
            end)
            mouse.KeyDown:Connect(function(key) print("keydown", key, uis:IsKeyDown(Enum.KeyCode.W)) end)
            mouse.Button1Down:Connect(function() print("button1", mouse.Target and mouse.Target.Name, math.round(mouse.Hit.Position.Z)) end)
            local cd = workspace:WaitForChild("Button"):WaitForChild("ClickDetector")
            cd.MouseHoverEnter:Connect(function(p) print("hover", p.Name) end)
            cd.MouseClick:Connect(function(p) print("client click", p.Name) end)
            "#,
        );
        server.start();
        let mut session = Session::new(server, Options::default());
        let index = session.join("Clicker", 7).unwrap();
        {
            let client = session.client_mut(index).unwrap();
            client.set_viewport(800, 600);
            let camera = client.current_camera().unwrap();
            client.set_property(
                camera,
                "CFrame",
                Value::CFrame(rook_types::CFrame::IDENTITY),
            );
        }
        session.run_for(0.1);
        session.input(
            index,
            InputEvent::Key {
                code: "W".into(),
                down: true,
            },
        );
        session.input(
            index,
            InputEvent::Key {
                code: "W".into(),
                down: false,
            },
        );
        session.input(index, InputEvent::MouseMove { x: 400.0, y: 300.0 });
        session.input(
            index,
            InputEvent::MouseButton {
                button: 1,
                down: true,
            },
        );
        session.input(
            index,
            InputEvent::MouseButton {
                button: 1,
                down: false,
            },
        );
        session.run_for(0.1);
        let client: Vec<String> = session
            .client(index)
            .unwrap()
            .take_log()
            .into_iter()
            .map(|e| e.message)
            .collect();
        let server_log: Vec<String> = session
            .server()
            .take_log()
            .into_iter()
            .map(|e| e.message)
            .collect();
        assert_eq!(
            client,
            [
                "began Keyboard W false",
                "keydown w true",
                "ended Keyboard W End",
                "hover Clicker",
                "began MouseButton1 None false",
                "button1 Button -10",
                "ended MouseButton1 None End",
                "client click Clicker",
            ]
        );
        assert_eq!(server_log, ["server click Clicker"]);
    }

    #[test]
    fn scripts_moved_into_workspace_start() {
        let mut s = Server::new(&Options::default()).unwrap();
        add_script(&s, "ServerStorage", "Inner", "print('inner started')");
        add_script(
            &s,
            "ServerScriptService",
            "Outer",
            r#"
            local inner = game:GetService("ServerStorage").Inner
            task.wait(0.1)
            print("moving")
            inner.Parent = workspace
            "#,
        );
        s.start();
        s.run_for(0.5);
        assert_eq!(lines(&s), ["moving", "inner started"]);
        assert_eq!(s.started_scripts(), 2);
    }
}
