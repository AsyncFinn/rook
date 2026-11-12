use rustc_hash::FxHashSet;

use super::*;

impl DataModel {
    /// Start every server Script that should run, in tree order.
    ///
    /// A script enabled by one that ran before it in this pass (a game's
    /// loader setting its tagged scripts' `Disabled` to false) starts
    /// after the pass, as one enabled at any other time does.
    ///
    /// The physics world is built first, as part of loading, as engines
    /// build a level's collision when it loads: otherwise the first query
    /// (a client's first mouse ray, a script's first raycast) builds it in
    /// a frame.
    pub fn start(&mut self) {
        {
            let mut w = self.runtime.world_mut();
            let w = &mut *w;
            w.physics.prepare(&mut w.dom);
        }
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

    fn start_scripts_under(&self, id: InstanceId) {
        let mut candidates = vec![id];
        candidates.extend(self.runtime.world().dom.descendants(id));
        for script in candidates {
            if self.should_run(script) {
                self.runtime.run_script(script);
            }
        }
    }

    /// Whether a script starts here: it isn't running, and runs where it
    /// is (`rook_lua`'s rule, which also stops scripts that leave).
    fn should_run(&self, id: InstanceId) -> bool {
        !self.runtime.is_started(id) && self.runtime.runs_here(id)
    }

    /// Note the scripts the journal says may have become runnable (moved
    /// into a running location, or enabled), to start at the next
    /// [`DataModel::start_new_scripts`]. Only reads, so the journal's
    /// readers can all catch up before it's trimmed.
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
                }
                _ => {}
            }
        }
        scripts.cursor = world.dom.journal().end();
    }

    /// Start scripts that became runnable since the last check. One that
    /// was stopped (disabled, or moved away) and is runnable again runs
    /// afresh, as on Roblox (Confirmed, Studio recording 2026-10-17).
    pub(crate) fn start_new_scripts(&mut self) {
        profiling::scope!("start_new_scripts");
        self.note_new_scripts();
        let candidates = std::mem::take(&mut self.new_scripts.candidates);
        let mut seen = FxHashSet::default();
        for id in candidates {
            if seen.insert(id) && self.should_run(id) {
                self.runtime.run_script(id);
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
