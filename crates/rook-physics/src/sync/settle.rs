//! Joints moving their parts as scripts look at them.
//!
//! A joint takes effect lazily. Nothing moves when it's made or changed; a
//! read of where a part is brings that one part to where its assembly's root
//! holds it, and the next sync, or a spatial query, brings the rest. So what
//! a script changes before reading the part that moves still counts: a weld's
//! other side anchored straight after it decides which side that is (observed
//! in Studio 0.746: `docs/behavior/physics/joints.md#when-joints-take-effect`).
//!
//! [`Physics::settle_joints`] works out where the parts of assemblies changed
//! since the last sync belong, once per change to joints, and answers reads
//! from that: a script reading every part of a large assembly doesn't walk
//! it for each, and parts in no such assembly cost one lookup.

use super::*;
use crate::assembly::Clusters;
use crate::joints;

/// Where parts belong that joints haven't moved yet.
#[derive(Debug, Default)]
pub(crate) struct Unsettled {
    /// How far the journal has been looked at.
    cursor: u64,
    /// A part of each assembly changed since the last sync.
    seeds: BTreeSet<InstanceId>,
    /// Where the parts of those assemblies belong, if not where they are.
    targets: FxHashMap<InstanceId, CFrame>,
}

impl Unsettled {
    /// Everything is in place: sync has moved it all.
    pub(crate) fn clear(&mut self) {
        self.seeds.clear();
        self.targets.clear();
    }
}

/// Whether a joint moves its parts into place. Hinges leave them where they
/// are, and a WeldConstraint keeps where they were (observed).
fn places(j: &Joint) -> bool {
    j.kind.is_rigid() && j.kind != JointKind::WeldConstraint
}

impl Physics {
    /// Whether a read might have to wait for [`Physics::settle_joints`].
    pub fn joints_unsettled(&self, dom: &Dom) -> bool {
        !self.unsettled.targets.is_empty() || self.unsettled_from() < dom.journal().end()
    }

    fn unsettled_from(&self) -> u64 {
        self.unsettled.cursor.max(self.cursor)
    }

    /// Bring `part` to where its joints hold it. Scripts call this before
    /// reading where it is.
    pub fn settle_joints(&mut self, dom: &mut Dom, part: InstanceId) {
        self.look_at_changes(dom);
        self.bring(dom, part);
        self.unsettled.cursor = dom.journal().end();
    }

    /// [`Physics::settle_joints`] for every part in a model.
    pub fn settle_joints_within(&mut self, dom: &mut Dom, model: InstanceId) {
        self.look_at_changes(dom);
        if !self.unsettled.targets.is_empty() {
            for part in dom.descendants(model) {
                self.bring(dom, part);
            }
        }
        self.unsettled.cursor = dom.journal().end();
    }

    fn bring(&mut self, dom: &mut Dom, part: InstanceId) {
        if let Some(at) = self.unsettled.targets.remove(&part)
            && let Err(e) = dom.set_property_by_name(part, "CFrame", Value::CFrame(at))
        {
            tracing::warn!(target: "physics", "settling joints: {e}");
        }
    }

    fn look_at_changes(&mut self, dom: &Dom) {
        let from = self.unsettled_from();
        if from >= dom.journal().end() {
            return;
        }
        profiling::scope!("Physics::settle_joints");
        let joined = |part| joints::part_joints(dom, part).iter().any(places);
        let db = dom.database();
        let unsettled = &mut self.unsettled;
        let mut changed = false;
        for change in dom.journal().since(from) {
            match change {
                Change::Property { id, property } => match db.property(*property).name {
                    "Part0" | "Part1" | "Enabled" | "C0" | "C1" => {
                        changed = true;
                        if let Some(j) = joints::active_joint(dom, *id).filter(places) {
                            unsettled.seeds.insert(j.part0);
                        }
                    }
                    "Anchored" if is_part(dom, *id) && joined(*id) => {
                        changed = true;
                        unsettled.seeds.insert(*id);
                    }
                    "CFrame" if unsettled.targets.contains_key(id) => changed = true,
                    _ => {}
                },
                Change::Reparented { id, .. } => {
                    changed |= !unsettled.seeds.is_empty();
                    for i in std::iter::once(*id).chain(dom.descendants(*id)) {
                        if let Some(j) = joints::active_joint(dom, i).filter(places) {
                            changed = true;
                            unsettled.seeds.insert(j.part0);
                        } else if is_part(dom, i) && joined(i) {
                            changed = true;
                            unsettled.seeds.insert(i);
                        }
                    }
                }
                _ => {}
            }
        }
        if changed {
            self.place_seeds(dom);
        }
    }

    /// Work out where the parts of the changed assemblies belong.
    fn place_seeds(&mut self, dom: &Dom) {
        let unsettled = &mut self.unsettled;
        unsettled.targets.clear();
        let mut clusters = Clusters::default();
        let mut placed: FxHashSet<InstanceId> = FxHashSet::default();
        for &seed in &unsettled.seeds {
            if placed.contains(&seed) {
                continue;
            }
            let Some(asm) = Assembly::of_with(dom, seed, &mut clusters) else {
                continue;
            };
            let Some(root) = dom.cframe_property(asm.root(), "CFrame") else {
                continue;
            };
            for m in &asm.members {
                placed.insert(m.part);
                let at = (root * m.offset).orthonormalize();
                if dom.cframe_property(m.part, "CFrame") != Some(at) {
                    unsettled.targets.insert(m.part, at);
                }
            }
        }
    }
}
