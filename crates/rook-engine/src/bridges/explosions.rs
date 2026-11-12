//! Explosions going off (docs/behavior/effects/explosions.md): an Explosion that
//! enters Workspace goes off once, at the next physics step, and is
//! removed 2.75 seconds of its own time (`TimeScale`) later.
//!
//! Only servers set them off: a client draws every Explosion it sees,
//! but doesn't hit, break or push anything, nor remove them
//! (Approximation: Studio does both on the client for Explosions a
//! LocalScript makes).
//!
//! New Explosions are noticed at the end of a frame
//! ([`DataModel::check_explosions`], before the journal is trimmed) and go
//! off before the next one's step ([`DataModel::set_off_explosions`]).

use rustc_hash::FxHashSet;

use super::journal_watch::JournalWatch;
use crate::*;

/// How long an Explosion stays in Workspace after going off, in its own
/// seconds (measured on the server: 2.7 to 2.9 s, at 0.1 s resolution,
/// Visible or not).
const LIFETIME: f64 = 2.75;

#[derive(Debug, Default)]
pub(crate) struct Explosions {
    pub(crate) watch: JournalWatch,
    /// Every Explosion seen in Workspace, so each goes off once.
    seen: FxHashSet<InstanceId>,
    /// Waiting for the next step.
    pending: Vec<InstanceId>,
    /// Gone off, and their age in their own time.
    live: Vec<(InstanceId, f64)>,
}

/// Whether `id` is an Explosion in Workspace.
fn in_workspace(dom: &Dom, id: InstanceId) -> bool {
    dom.class_name(id) == "Explosion"
        && dom
            .find_service("Workspace")
            .is_some_and(|ws| dom.is_ancestor_of(ws, id))
}

impl DataModel {
    /// Note the Explosions that entered Workspace this frame (servers).
    pub(crate) fn check_explosions(&mut self) {
        if self.role == Role::Client {
            return;
        }
        let w = self.runtime.world();
        let dom = &w.dom;
        let x = &mut self.explosions;
        let mut found = Vec::new();
        let (first, changes) = x.watch.look(dom);
        rook_types::count!(JournalReadBridges, changes.len());
        if first && let Some(ws) = dom.find_service("Workspace") {
            found.extend(dom.descendants(ws));
        }
        for c in changes {
            match c {
                Change::Reparented { subtree, .. } => found.extend(subtree.iter().copied()),
                Change::Collected(id) => {
                    x.seen.remove(id);
                    x.pending.retain(|p| p != id);
                    x.live.retain(|(l, _)| l != id);
                }
                _ => {}
            }
        }
        for id in found {
            if in_workspace(dom, id) && x.seen.insert(id) {
                x.pending.push(id);
            }
        }
    }

    /// Set off the Explosions waiting to go off, before the step: fire
    /// `Hit` for every part each one hits, with the part's distance, then
    /// break their joints and push them (measured: `Hit`'s handlers still
    /// see the joints and no push).
    pub(crate) fn set_off_explosions(&mut self) {
        let pending = std::mem::take(&mut self.explosions.pending);
        for id in pending {
            let blast = {
                let mut w = self.runtime.world_mut();
                let w = &mut *w;
                if !in_workspace(&w.dom, id) {
                    continue;
                }
                w.physics.blast(&mut w.dom, id)
            };
            for &(part, distance) in &blast.hits {
                if let Ok(part) = self.runtime.push_instance(part) {
                    self.runtime
                        .fire_event(id, "Hit", (part, f64::from(distance)));
                }
            }
            for &joint in &blast.joints {
                self.runtime.destroy(joint);
            }
            {
                let mut w = self.runtime.world_mut();
                let w = &mut *w;
                w.physics.push(&mut w.dom, &blast);
            }
            self.explosions.live.push((id, 0.0));
        }
    }

    /// Age the Explosions that went off by `dt`, removing those whose
    /// time is up (and forgetting those already gone).
    pub(crate) fn age_explosions(&mut self, dt: f64) {
        if self.explosions.live.is_empty() {
            return;
        }
        let mut done = Vec::new();
        {
            let w = self.runtime.world();
            self.explosions.live.retain_mut(|(id, age)| {
                if w.dom.is_destroyed(*id) {
                    done.push(*id);
                    return false;
                }
                let scale = w
                    .dom
                    .number_property(*id, "TimeScale")
                    .unwrap_or(1.0)
                    .clamp(0.0, 1.0);
                *age += dt * scale;
                if *age >= LIFETIME {
                    done.push(*id);
                    return false;
                }
                true
            });
        }
        for id in done {
            self.explosions.seen.remove(&id);
            self.runtime.destroy(id);
        }
    }
}
