//! Every Workspace part's `Size` box in a BVH, which the queries that
//! find parts by their box search (Region3, `GetPartBounds*`,
//! `MakeJoints`, FloorWire floors): the colliders' bounds miss a turned
//! cylinder's or wedge's box corners. How it's kept fresh and cheap, as
//! Rapier keeps its broad phase, is in `docs/design/architecture.md`
//! (Part boxes). A leaf may reach past its part's box (its skin), so
//! callers test the part's own box.

use rapier3d::parry::bounding_volume::{Aabb, BoundingVolume};
use rapier3d::parry::partitioning::{Bvh, BvhBuildStrategy};
use rustc_hash::{FxHashMap, FxHashSet};

use glam::Vec3;
use rook_dom::{Dom, InstanceId};

use crate::Physics;
use crate::shape;

#[derive(Default)]
pub(crate) struct PartBounds {
    bvh: Bvh,
    /// Each part's leaf, and each leaf's part.
    leaves: FxHashMap<InstanceId, u32>,
    parts: Vec<Option<InstanceId>>,
    free: Vec<u32>,
    /// Parts physics added that the tree doesn't hold yet.
    added: FxHashSet<InstanceId>,
    /// Parts whose box may have changed since the tree last read it.
    marked: FxHashSet<InstanceId>,
}

impl PartBounds {
    pub(crate) fn add(&mut self, part: InstanceId) {
        self.added.insert(part);
    }

    pub(crate) fn mark(&mut self, part: InstanceId) {
        self.marked.insert(part);
    }

    pub(crate) fn remove(&mut self, part: InstanceId) {
        self.added.remove(&part);
        self.marked.remove(&part);
        if let Some(leaf) = self.leaves.remove(&part) {
            self.bvh.remove(leaf);
            self.parts[leaf as usize] = None;
            self.free.push(leaf);
        }
    }

    /// Put the parts physics added in the tree, as it adds them (so the
    /// first query after a place loads doesn't pay for the whole place).
    /// Many at once (a sixteenth of the tree or more, Rapier's cut-off
    /// for bulk changes) build the whole tree again in bulk, which is far
    /// cheaper than inserting them one by one; fewer are inserted.
    pub(crate) fn place_added(&mut self, dom: &Dom) {
        if self.added.is_empty() {
            return;
        }
        profiling::scope!("PartBounds::place_added");
        let added = std::mem::take(&mut self.added);
        if added.len() * 16 < self.leaves.len() {
            for part in added {
                self.place(dom, part);
            }
            return;
        }
        // Every box is read again, so nothing is left marked.
        self.marked.clear();
        let mut all: Vec<InstanceId> = self.leaves.keys().copied().chain(added).collect();
        all.sort_unstable();
        all.dedup();
        let boxes: Vec<(InstanceId, Aabb)> = all
            .into_iter()
            .filter_map(|part| Some((part, size_box(dom, part)?)))
            .collect();
        self.leaves.clear();
        self.free.clear();
        self.parts = boxes.iter().map(|&(part, _)| Some(part)).collect();
        self.leaves = (boxes.iter().enumerate())
            .map(|(leaf, &(part, _))| (part, leaf as u32))
            .collect();
        self.bvh = Bvh::from_iter(
            BvhBuildStrategy::Binned,
            (boxes.iter().enumerate()).map(|(leaf, (_, aabb))| (leaf, aabb.loosened(skin(aabb)))),
        );
    }

    /// The parts whose box may meet `aabb`: every one that does, and
    /// perhaps some more. Parts added or moved since are read from `dom`
    /// first, so the tree is never read stale; only box queries pay for
    /// the parts that moved, and only when they look.
    pub(crate) fn near<'a>(
        &'a mut self,
        dom: &Dom,
        aabb: &'a Aabb,
    ) -> impl Iterator<Item = InstanceId> + use<'a> {
        self.place_added(dom);
        if !self.marked.is_empty() {
            profiling::scope!("PartBounds::refresh");
            for part in std::mem::take(&mut self.marked) {
                if self.leaves.contains_key(&part) {
                    self.place(dom, part);
                }
            }
        }
        let this = &*self;
        this.bvh
            .intersect_aabb(aabb)
            .filter_map(|leaf| this.parts.get(leaf as usize).copied().flatten())
    }

    /// Put `part`'s box in the tree as it is now, giving it a leaf if it
    /// has none.
    fn place(&mut self, dom: &Dom, part: InstanceId) {
        let Some(aabb) = size_box(dom, part) else {
            return;
        };
        let leaf = match self.leaves.get(&part) {
            Some(&leaf) => leaf,
            None => {
                let leaf = self.free.pop().unwrap_or_else(|| {
                    self.parts.push(None);
                    (self.parts.len() - 1) as u32
                });
                self.parts[leaf as usize] = Some(part);
                self.leaves.insert(part, leaf);
                leaf
            }
        };
        self.bvh
            .reinsert_or_update_with_change_detection(aabb, leaf, skin(&aabb));
    }
}

/// How far a leaf reaches past its part's box, so that a part moving
/// within it leaves the tree alone: Rapier's adaptive broad-phase skin
/// (an eighth of the box's smallest side, from 0.04 to 0.25 studs).
fn skin(aabb: &Aabb) -> f32 {
    (aabb.extents().min_element() * 0.125).clamp(0.04, 0.25)
}

/// The world-axis-aligned box around `part`'s `Size` box.
fn size_box(dom: &Dom, part: InstanceId) -> Option<Aabb> {
    let (min, max) = shape::part_box(dom, part)?;
    Some(Aabb::new(min, max))
}

impl Physics {
    /// Body `h` was made or moved: queries put its colliders in place,
    /// and its parts' boxes, before they look.
    pub(crate) fn body_moved(&mut self, h: rapier3d::prelude::RigidBodyHandle) {
        self.stale.insert(h);
        if let Some(body) = self.bodies.get(&h) {
            for m in &body.asm.members {
                self.bounds.mark(m.part);
            }
        }
    }

    /// The same for an anchored part on its own (a static).
    pub(crate) fn static_moved(&mut self, part: InstanceId) {
        self.stale_statics.insert(part);
        self.bounds.mark(part);
    }

    /// The Workspace parts whose `Size` box meets the box from `min` to
    /// `max` (touching counts), in id order. Unfiltered: every part,
    /// whatever its `CanQuery` or collision group. Physics must have taken
    /// the DOM's changes ([`Physics::sync`]): the parts are found where
    /// `dom` has them, never where they were.
    pub fn parts_by_box(&mut self, dom: &Dom, min: Vec3, max: Vec3) -> Vec<InstanceId> {
        profiling::scope!("Physics::parts_by_box");
        assert!(
            self.cursor == dom.journal().end() || self.props.is_none(),
            "physics hasn't taken the DOM's latest changes (Physics::sync)"
        );
        let query = Aabb::new(min, max);
        let mut found: Vec<InstanceId> = self
            .bounds
            .near(dom, &query)
            .filter(|&p| size_box(dom, p).is_some_and(|b| b.intersects(&query)))
            .collect();
        found.sort_unstable();
        found
    }
}
