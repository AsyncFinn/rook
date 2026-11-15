//! Who scripts made simulate an assembly (`BasePart:SetNetworkOwner`),
//! kept by physics as the owner of assemblies
//! (docs/behavior/physics/ownership.md). A setting holds the assembly as it
//! was when it was made: a joint made or broken on it, or anchoring one of
//! its parts, sends it back to automatic, as Roblox does (measured). Leaving
//! Workspace and coming back keeps it (measured).

use rook_dom::{Change, Dom, InstanceId};
use rustc_hash::FxHashMap;

use crate::Physics;
use crate::assembly::Assembly;
use crate::joints::{endpoints, is_joint};

/// Who simulates an assembly a script set: a player's client, or the
/// server (`None`).
pub type Owner = Option<InstanceId>;

#[derive(Debug, Default)]
pub(crate) struct Ownership {
    /// The setting each set part's assembly holds, by part.
    by_part: FxHashMap<InstanceId, u32>,
    sets: FxHashMap<u32, Set>,
    next: u32,
    /// How far the journal has been read for changes that reset settings.
    cursor: u64,
}

#[derive(Debug)]
struct Set {
    owner: Owner,
    parts: Vec<InstanceId>,
}

impl Ownership {
    fn clear(&mut self, part: InstanceId) {
        let Some(set) = self
            .by_part
            .remove(&part)
            .and_then(|id| self.sets.remove(&id))
        else {
            return;
        };
        for p in set.parts {
            self.by_part.remove(&p);
        }
    }

    /// Reset the settings of assemblies the journal shows joined, split
    /// or anchored since the last call.
    fn refresh(&mut self, dom: &Dom) {
        let end = dom.journal().end();
        if self.by_part.is_empty() {
            self.cursor = end;
            return;
        }
        let db = dom.database();
        let mut reset = Vec::new();
        for change in dom.journal().since(self.cursor) {
            match change {
                Change::Property { id, property } => match db.property(*property).name {
                    "Anchored" => reset.push(*id),
                    "Part0" | "Part1" | "Enabled" if is_joint(dom, *id) => {
                        reset.extend(endpoints(dom, *id).into_iter().flatten());
                    }
                    _ => {}
                },
                Change::Reparented { subtree, .. } => {
                    for &i in subtree {
                        if is_joint(dom, i) {
                            reset.extend(endpoints(dom, i).into_iter().flatten());
                        }
                    }
                }
                _ => {}
            }
        }
        for part in reset {
            self.clear(part);
        }
        self.cursor = end;
    }
}

impl Physics {
    /// Read the journal for changes that send set assemblies back to
    /// automatic ownership: one of the journal's readers
    /// (`rook_engine::DataModel::read_journal`), and run before every
    /// question about ownership so scripts see a reset at once.
    pub fn note_ownership(&mut self, dom: &Dom) {
        self.ownership.refresh(dom);
    }

    /// What a script set `part`'s assembly to (`Some(owner)`), or `None`
    /// while ownership is automatic.
    pub fn network_setting(&mut self, dom: &Dom, part: InstanceId) -> Option<Owner> {
        self.ownership.refresh(dom);
        let id = self.ownership.by_part.get(&part)?;
        Some(self.ownership.sets[id].owner)
    }

    /// Set who simulates `part`'s assembly (`Some`), or make it automatic
    /// again (`None`): every part of the assembly as it is now.
    pub fn set_network_owner(&mut self, dom: &Dom, part: InstanceId, setting: Option<Owner>) {
        self.ownership.refresh(dom);
        let parts: Vec<InstanceId> = match Assembly::of(dom, part) {
            Some(asm) => asm.members.iter().map(|m| m.part).collect(),
            None => vec![part],
        };
        for &p in &parts {
            self.ownership.clear(p);
        }
        let Some(owner) = setting else {
            return;
        };
        let o = &mut self.ownership;
        let id = o.next;
        o.next = o.next.wrapping_add(1);
        for &p in &parts {
            o.by_part.insert(p, id);
        }
        o.sets.insert(id, Set { owner, parts });
    }

    /// Every part a script set the owner of, with its owner, in no order.
    pub fn network_settings(&mut self, dom: &Dom) -> Vec<(InstanceId, Owner)> {
        self.ownership.refresh(dom);
        let o = &self.ownership;
        o.by_part
            .iter()
            .map(|(&part, id)| (part, o.sets[id].owner))
            .collect()
    }

    /// `player` left: what it was set to simulate goes back to automatic
    /// (Inference: there is no client left to simulate it).
    pub fn forget_network_owner(&mut self, player: InstanceId) {
        let o = &mut self.ownership;
        let gone: Vec<u32> = (o.sets.iter())
            .filter(|(_, s)| s.owner == Some(player))
            .map(|(&id, _)| id)
            .collect();
        for id in gone {
            if let Some(set) = o.sets.remove(&id) {
                for p in set.parts {
                    o.by_part.remove(&p);
                }
            }
        }
    }
}
