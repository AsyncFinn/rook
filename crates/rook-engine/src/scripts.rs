use rustc_hash::FxHashSet;

use rook_types::RunContext;

use super::*;

impl DataModel {
    /// Start every server Script that should run, in tree order.
    ///
    /// A script enabled by one that ran before it in this pass (a game's
    /// loader setting its tagged scripts' `Disabled` to false) starts
    /// after the pass, as one enabled at any other time does.
    pub fn start(&mut self) {
        let root = self.runtime.world().dom.root();
        self.new_scripts = NewScripts::starting_at(self.runtime.world().dom.journal().end());
        self.start_scripts_under(root);
        self.start_new_scripts();
        if self.role == Role::Server {
            self.install_legacy_chat();
        }
    }

    /// After the place's scripts started, which find no ChatServiceRunner
    /// yet (Observed in Studio 0.746). The next frame starts its copy.
    fn install_legacy_chat(&self) {
        let (start, left) = {
            let mut world = self.runtime.world_mut();
            let start = world.dom.journal().end();
            let patch = self.core_scripts.as_deref();
            (start, crate::chat::legacy::install(&mut world.dom, patch))
        };
        if let Some(source) = left
            && let Err(e) = self.runtime.run_source("ChatLocalization", &source, None)
        {
            tracing::warn!("installing the chat: {e}");
        }
        // Whatever waits on the runner's copy wakes here.
        self.runtime.dispatch_since(start);
    }

    fn start_scripts_under(&mut self, id: InstanceId) {
        let mut candidates = vec![id];
        candidates.extend(self.runtime.world().dom.descendants(id));
        for script in candidates {
            if self.runs_here(script) && self.runtime.run_script(script) {
                // What it changed is noted now, while whether each script
                // is running still says whether an enable restarts it.
                self.note_new_scripts();
            }
        }
    }

    /// Whether a script runs where it is, running already or not.
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
    fn runs_here(&self, id: InstanceId) -> bool {
        let world = self.runtime.world();
        let dom = &world.dom;
        let class = dom.class_name(id);
        if !matches!(class, "Script" | "LocalScript") {
            return false;
        }
        if dom.bool_property(id, "Disabled") == Some(true) {
            return false;
        }
        if self.role == Role::Client {
            return client_runs_here(dom, id, class);
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
            // A player's Backpack too: classic gear's server Scripts hear
            // their tool's first `Equipped` there (Inference, from gear
            // like the LinkedSword working on the first equip).
            Some(RunContext::Legacy) | None => {
                matches!(top_class, "Workspace" | "ServerScriptService")
                    || dom.ancestors(id).any(|a| {
                        dom.class_name(a) == "Backpack"
                            && dom.parent(a).is_some_and(|p| dom.is_a(p, "Player"))
                    })
            }
        }
    }

    /// Note the scripts the journal says may have become runnable (moved
    /// into a running location, or enabled) or been disabled, to start or
    /// stop at the next [`DataModel::start_new_scripts`]. Only reads, so
    /// the journal's readers can all catch up before it's trimmed.
    pub(crate) fn note_new_scripts(&mut self) {
        let world = self.runtime.world();
        let db = world.dom.database();
        // The name check first: most changes are other properties (physics
        // writes every moving part's CFrame each frame).
        let disabled = |id: InstanceId, property: rook_reflection::PropertyId| {
            db.property(property).name == "Disabled"
                && world.dom.property_id(id, "Disabled") == Some(property)
        };
        let scripts = &mut self.new_scripts;
        let changes = world.dom.journal().since(scripts.cursor);
        rook_types::count!(JournalReadScripts, changes.len());
        for change in changes {
            match change {
                Change::Reparented { subtree, .. } => scripts.candidates.extend(subtree),
                Change::Property { id, property } if disabled(*id, *property) => {
                    scripts.candidates.push(*id);
                    // Only a running script is restarted by the change;
                    // one that never ran just starts.
                    if self.runtime.is_started(*id) {
                        scripts.toggled.insert(*id);
                    }
                }
                _ => {}
            }
        }
        scripts.cursor = world.dom.journal().end();
    }

    /// Start scripts that became runnable since the last check and stop
    /// running ones that were disabled or moved, or had an ancestor moved,
    /// out of where scripts run (destroyed included). A script disabled and
    /// enabled again runs afresh, as on Roblox (Confirmed, Studio recording
    /// 2026-10-17), and so does one moved back (Confirmed, Studio 0.747
    /// 2026-11-11).
    pub(crate) fn start_new_scripts(&mut self) {
        profiling::scope!("start_new_scripts");
        self.stop_scripts_out_of_place();
        let candidates = std::mem::take(&mut self.new_scripts.candidates);
        let toggled = std::mem::take(&mut self.new_scripts.toggled);
        self.new_scripts.checked = 0;
        let mut seen = FxHashSet::default();
        for id in candidates {
            if !seen.insert(id) {
                continue;
            }
            if toggled.contains(&id) {
                self.runtime.stop_script(id);
            }
            if self.runs_here(id) {
                self.runtime.run_script(id);
            }
        }
    }

    /// Stop running scripts that left where scripts run since the last
    /// look. Roblox stops them as they leave, so this runs between the
    /// frame's phases too: a destroyed script's connections don't fire
    /// later in the frame (Confirmed, Studio 0.747 2026-11-11: a
    /// LocalScript's RenderStepped connection stopped with its ScreenGui's
    /// `Destroy`). Starting scripts waits for the frame's end.
    pub(crate) fn stop_scripts_out_of_place(&mut self) {
        self.note_new_scripts();
        let end = self.new_scripts.candidates.len();
        for i in std::mem::replace(&mut self.new_scripts.checked, end)..end {
            let id = self.new_scripts.candidates[i];
            if self.runtime.is_started(id) && !self.runs_here(id) {
                self.runtime.stop_script(id);
            }
        }
    }
}

/// What the journal said about scripts since they were last started.
#[derive(Debug, Default)]
pub(crate) struct NewScripts {
    /// Journal position up to which scripts have been looked for.
    cursor: u64,
    /// Scripts that may have become runnable, in the order they changed.
    candidates: Vec<InstanceId>,
    /// How many of `candidates` were checked for having left where
    /// scripts run.
    checked: usize,
    /// Scripts whose Disabled changed. The journal only records real
    /// changes, so a running one among them was disabled at some point.
    toggled: FxHashSet<InstanceId>,
}

impl NewScripts {
    /// Scripts are looked for from journal position `cursor` on.
    pub(crate) fn starting_at(cursor: u64) -> Self {
        Self {
            cursor,
            ..Self::default()
        }
    }
}

fn client_runs_here(dom: &Dom, id: InstanceId, class: &str) -> bool {
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
