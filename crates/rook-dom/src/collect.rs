//! Freeing destroyed instances once nothing holds them, as Roblox frees a
//! destroyed instance once nothing, Lua included, references it.
//!
//! An instance holds its parent, its children and what its reference
//! properties point at (each can be read from it). A destroyed instance is
//! held when one that isn't destroyed holds it, when the caller says
//! something outside the tree does (a script's handle), or through other
//! destroyed instances held so. Live instances are never collected:
//! everything outside the DataModel that the engine keeps is live.
//!
//! The owner of the journal collects at each trim, in two steps a trim
//! apart, so every reader drops what it keeps of an instance before its
//! slot goes: [`Dom::announce_collectable`] records [`Change::Collected`]
//! for the destroyed instances nothing holds, and the next trim's
//! [`Dom::free_collected`] frees them, unless something took hold of one
//! meanwhile. Each check costs what is destroyed and not yet freed, not
//! the size of the place.

use super::*;
use crate::properties::ref_target;
use rustc_hash::FxHashSet;

impl Dom {
    /// Free the instances announced at the last trim that nothing holds
    /// yet, returning them. Those held again stay destroyed, checked again
    /// at the next trim.
    pub fn free_collected(&mut self, held: impl Fn(InstanceId) -> bool) -> Vec<InstanceId> {
        let collecting = std::mem::take(&mut self.collecting);
        let (free, kept) = self.split_unheld(collecting, &held);
        self.destroyed.extend(kept);
        for &id in &free {
            self.free(id);
        }
        free
    }

    /// Record [`Change::Collected`] for each destroyed instance nothing
    /// holds, to be freed at the next trim. Recorded after that trim's
    /// cut, so every reader sees it before the slot goes.
    pub fn announce_collectable(&mut self, held: impl Fn(InstanceId) -> bool) {
        if !self.journal.is_enabled() {
            // Readers couldn't hear of it; the next trim checks again.
            return;
        }
        let destroyed = std::mem::take(&mut self.destroyed);
        let (unheld, kept) = self.split_unheld(destroyed, &held);
        self.destroyed = kept;
        for &id in &unheld {
            self.journal.push(Change::Collected(id));
        }
        self.collecting = unheld;
    }

    /// Split `candidates` (destroyed instances) into those nothing holds
    /// and the rest.
    fn split_unheld(
        &self,
        candidates: Vec<InstanceId>,
        held: &impl Fn(InstanceId) -> bool,
    ) -> (Vec<InstanceId>, Vec<InstanceId>) {
        let candidate: FxHashSet<InstanceId> = candidates.iter().copied().collect();
        let mut kept = FxHashSet::default();
        let mut pending: Vec<InstanceId> = candidates
            .iter()
            .copied()
            .filter(|&id| held(id) || self.holders(id).any(|h| !candidate.contains(&h)))
            .collect();
        while let Some(id) = pending.pop() {
            if kept.insert(id) {
                pending.extend(self.holds(id).filter(|h| candidate.contains(h)));
            }
        }
        candidates.into_iter().partition(|id| !kept.contains(id))
    }

    /// What `id` holds: its parent, children and reference targets.
    fn holds(&self, id: InstanceId) -> impl Iterator<Item = InstanceId> + '_ {
        let inst = &self.instances[id];
        inst.parent
            .into_iter()
            .chain(inst.children.as_slice().iter().copied())
            .chain(inst.properties.values().filter_map(ref_target))
    }

    /// What holds `id`: its parent and children, and the instances whose
    /// reference properties point at it.
    fn holders(&self, id: InstanceId) -> impl Iterator<Item = InstanceId> + '_ {
        let inst = &self.instances[id];
        inst.parent
            .into_iter()
            .chain(inst.children.as_slice().iter().copied())
            .chain(self.referrers(id).iter().map(|r| r.0))
    }

    /// Take `id` out of the arena. Nothing holds it, so it has no parent or
    /// children (a destroyed instance leaves its parent, and a child would
    /// hold it).
    fn free(&mut self, id: InstanceId) {
        let inst = self.instances.remove(id).expect("collected instance");
        debug_assert!(inst.parent.is_none() && inst.children.as_slice().is_empty());
        for (property, value) in &inst.properties {
            self.unlink(id, *property, value);
        }
        self.referrers.remove(&id);
        self.drawn.remove(&id);
        #[cfg(debug_assertions)]
        self.freed.insert(id);
    }

    /// Fail in debug builds when `id` was freed: whoever read it kept it
    /// past its [`Change::Collected`].
    #[cfg_attr(not(debug_assertions), expect(clippy::unused_self))]
    pub(crate) fn check_not_freed(&self, id: InstanceId) {
        #[cfg(debug_assertions)]
        assert!(
            self.freed.is_empty() || !self.freed.contains(&id),
            "{id:?} was read after it was collected"
        );
        #[cfg(not(debug_assertions))]
        let _ = id;
    }
}
