//! Which scripts run where, and starting them.

use std::collections::HashSet;

use rook_types::RunContext;

use super::*;

impl DataModel {
    /// Start every server Script that should run, in tree order.
    pub fn start(&mut self) {
        // So scripts' first queries see unions' shapes.
        self.update_unions();
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
        if dom.bool_property(id, "Disabled") == Some(true) {
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
        match run_context(dom, id) {
            Some(RunContext::Server) => !(is_starter(top_class) || top_class == "ServerStorage"),
            Some(RunContext::Client | RunContext::Plugin) => false,
            Some(RunContext::Legacy) | None => {
                matches!(top_class, "Workspace" | "ServerScriptService")
            }
        }
    }

    /// Start scripts that became runnable since the last check (moved into
    /// a running location or enabled) and stop running ones that were
    /// disabled. A script disabled and enabled again runs afresh, as on
    /// Roblox (Confirmed, Studio recording 2026-10-17).
    pub(crate) fn start_new_scripts(&mut self) {
        profiling::scope!("start_new_scripts");
        let end = self.runtime.world().dom.journal().end();
        let mut candidates: Vec<InstanceId> = Vec::new();
        // Scripts whose Disabled changed. The journal only records real
        // changes, so a running one among them was disabled at some point.
        let mut toggled: HashSet<InstanceId> = HashSet::new();
        {
            let world = self.runtime.world();
            let db = world.dom.database();
            // The name check first: most changes are other properties (physics
            // writes every moving part's CFrame each frame).
            let disabled = |id: InstanceId, property: rook_reflection::PropertyId| {
                db.property(property).name == "Disabled"
                    && world.dom.property_id(id, "Disabled") == Some(property)
            };
            for change in world.dom.journal().since(self.script_cursor) {
                match change {
                    Change::Reparented { subtree, .. } => candidates.extend(subtree),
                    Change::Property { id, property } if disabled(*id, *property) => {
                        candidates.push(*id);
                        toggled.insert(*id);
                    }
                    _ => {}
                }
            }
        }
        self.script_cursor = end;
        let mut seen = HashSet::new();
        for id in candidates {
            if !seen.insert(id) {
                continue;
            }
            if toggled.contains(&id) {
                self.runtime.stop_script(id);
            }
            if self.should_run(id) {
                self.runtime.run_script(id);
            }
        }
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
    run_context(dom, id) == Some(RunContext::Client)
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

fn run_context(dom: &Dom, id: InstanceId) -> Option<RunContext> {
    RunContext::from_u32(dom.enum_property(id, "RunContext")?)
}
