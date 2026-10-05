//! Assembles a DataModel: loads a place, creates the services scripts
//! expect, starts server Scripts, and runs the frame loop.
//!
//! M1 runs headless server Scripts only: no physics, rendering, players or
//! LocalScripts yet. See `docs/architecture.md` §6 for the frame order.

use std::time::Duration;

use raven_dom::{Change, Dom, InstanceId};
use raven_lua::{LogEntry, Runtime};
use raven_scheduler::{Clock, Queue};
use raven_types::Value;

pub use raven_lua::MessageType;

#[derive(Debug, thiserror::Error)]
pub enum EngineError {
    #[error(transparent)]
    Load(#[from] raven_formats::LoadError),
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
            script_timeout: raven_lua::DEFAULT_SCRIPT_TIMEOUT,
            memory_limit: 1 << 30,
        }
    }
}

/// A server DataModel running headless.
#[derive(Debug)]
pub struct Server {
    runtime: Runtime,
    run_service: InstanceId,
    /// Journal position up to which new scripts have been looked for.
    script_cursor: u64,
}

impl Server {
    /// An empty place.
    pub fn new(options: &Options) -> Result<Self, EngineError> {
        Self::with_dom(Dom::new(), options)
    }

    /// Load a place file.
    pub fn load_place(bytes: &[u8], options: &Options) -> Result<Self, EngineError> {
        let loaded = raven_formats::load(bytes)?;
        let mut dom = Dom::new();
        let root = dom.root();
        raven_formats::into_dom(&loaded.dom, &mut dom, root);
        Self::with_dom(dom, options)
    }

    fn with_dom(mut dom: Dom, options: &Options) -> Result<Self, EngineError> {
        dom.journal_mut().set_enabled(false);
        for service in DEFAULT_SERVICES {
            // A class missing from the database would be a Raven bug, not a
            // problem with the place; skip it rather than fail.
            let _ = dom.get_service(service);
        }
        dom.journal_mut().set_enabled(true);
        let run_service = dom.get_service("RunService").expect("RunService class");
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
            run_service,
            script_cursor,
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

    /// Whether a Script runs on the server where it is.
    ///
    /// Legacy run context: under Workspace or ServerScriptService
    /// (Confirmed: creator-docs Script). Server run context: anywhere
    /// except ServerStorage and the Starter containers (Inference).
    fn should_run(&self, id: InstanceId) -> bool {
        let world = self.runtime.world();
        let dom = &world.dom;
        if dom.class_name(id) != "Script" || self.runtime.is_started(id) {
            return false;
        }
        if dom.property_by_name(id, "Disabled") == Some(Value::Bool(true)) {
            return false;
        }
        let Some(top) = top_level(dom, id) else {
            return false;
        };
        let top_class = dom.class_name(top);
        match dom.property_by_name(id, "RunContext") {
            Some(Value::Enum(1)) => !matches!(
                top_class,
                "ServerStorage" | "StarterGui" | "StarterPack" | "StarterPlayer"
            ),
            Some(Value::Enum(2 | 3)) => false,
            _ => matches!(top_class, "Workspace" | "ServerScriptService"),
        }
    }

    /// Start scripts that became runnable since the last check: moved into
    /// a running location or enabled.
    fn start_new_scripts(&mut self) {
        let end = self.runtime.world().dom.journal().end();
        let candidates: Vec<InstanceId> = {
            let world = self.runtime.world();
            let journal = world.dom.journal();
            let disabled = |id: InstanceId| {
                world
                    .dom
                    .get(id)
                    .map(|i| i.class)
                    .and_then(|c| world.dom.database().find_property(c, "Disabled"))
                    .map(|p| p.id)
            };
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
        let (now, dt) = {
            let mut w = self.runtime.world_mut();
            let dt = w.scheduler.begin_frame();
            (w.scheduler.now(), dt)
        };
        let rs = self.run_service;
        self.runtime.resume_due(Queue::Legacy);
        self.fire(rs, "Stepped", (now, dt));
        self.fire(rs, "PreSimulation", dt);
        self.fire(rs, "PostSimulation", dt);
        self.fire(rs, "Heartbeat", dt);
        self.runtime.resume_due(Queue::Task);
        self.start_new_scripts();
        let mut w = self.runtime.world_mut();
        let end = w.dom.journal().end();
        w.dom.journal_mut().trim(end);
    }

    fn fire(&self, instance: InstanceId, event: &'static str, args: impl mlua::IntoLuaMulti) {
        let _ = self.runtime.fire_event(instance, event, args);
        self.runtime.drain_deferred();
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
        let mut w = server.runtime.world_mut();
        let parent = w.dom.get_service(location).unwrap();
        let script = w.dom.create_by_name("Script").unwrap();
        w.dom.set_name(script, name.into()).unwrap();
        let class = w.dom.get(script).unwrap().class;
        let pid = w.dom.database().find_property(class, "Source").unwrap().id;
        w.dom
            .set_property(script, pid, Value::String(source.into()))
            .unwrap();
        w.dom.set_parent(script, Some(parent)).unwrap();
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
