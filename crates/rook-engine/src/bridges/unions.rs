//! Unions' collision geometry: read from the file and handed to physics,
//! which collides and queries unions as it (`rook_physics::unions`).
//!
//! Every DataModel does this, server and client alike: the client's
//! world answers the mouse's queries. A union in Workspace is the hulls
//! saved in its `PhysicalConfigData` (or, in older files, `PhysicsData`);
//! with none it is its box, as in Studio (`docs/behavior/collision.md`).
//!
//! A place's unions get theirs before its scripts start; after that,
//! changes are noticed at the end of a frame ([`DataModel::check_unions`],
//! before the journal is trimmed) and acted on at the start of the next.

use std::collections::HashSet;

use rook_content::decode::mesh::union::physics;
use rook_types::UnionCollision;

use crate::*;

#[derive(Debug, Default)]
pub(crate) struct Unions {
    /// Journal position up to which changes have been looked at.
    cursor: u64,
    /// Whether every union has been looked at once.
    scanned: bool,
    /// Unions to look at again.
    dirty: HashSet<InstanceId>,
    /// Geometry decoded so far, by its bytes: copies share it.
    saved: HashMap<Vec<u8>, Option<Arc<UnionCollision>>>,
}

/// The properties whose change gives a union new geometry.
fn is_geometry_property(name: &str) -> bool {
    matches!(name, "PhysicalConfigData" | "PhysicsData")
}

impl Unions {
    /// Look at every union again (a client DataModel filled by hand).
    pub(crate) fn rescan(&mut self, cursor: u64) {
        self.cursor = cursor;
        self.scanned = false;
    }
}

/// The physics blob saved in a union, if one holds geometry or a block.
/// `PhysicalConfigData` comes first: older files saved `PhysicsData`
/// instead, and a file without `PhysicalConfigData` reads as its default,
/// a block (Studio does the same: such a union is a box).
fn saved_blob(dom: &Dom, id: InstanceId) -> Option<Vec<u8>> {
    let blob = |name| {
        let bytes = match dom.property_by_name(id, name)? {
            Value::BinaryString(b) => b.into_vec(),
            Value::SharedString(s) => s.data().to_vec(),
            _ => return None,
        };
        physics::has_geometry(&bytes).then_some(bytes)
    };
    match blob("PhysicalConfigData") {
        Some(b) if physics::is_block(&b) => blob("PhysicsData").or(Some(b)),
        other => other,
    }
}

impl DataModel {
    /// Note unions that entered or left Workspace or changed geometry this
    /// frame, before the journal is trimmed (every union, the first time).
    pub(crate) fn check_unions(&mut self) {
        let w = self.runtime.world();
        let dom = &w.dom;
        let unions = &mut self.unions;
        let Some(ws) = dom.find_service("Workspace") else {
            return;
        };
        if !unions.scanned {
            unions.scanned = true;
            let all = dom.descendants(ws);
            unions
                .dirty
                .extend(all.into_iter().filter(|&i| dom.is_a(i, "PartOperation")));
        }
        let db = dom.database();
        for c in dom.journal().since(unions.cursor) {
            match c {
                Change::Property { id, property }
                    if is_geometry_property(db.property(*property).name)
                        && dom.is_a(*id, "PartOperation") =>
                {
                    unions.dirty.insert(*id);
                }
                Change::Reparented { subtree, .. } => {
                    let moved = subtree.iter().filter(|&&i| dom.is_a(i, "PartOperation"));
                    unions.dirty.extend(moved);
                }
                _ => {}
            }
        }
        unions.cursor = dom.journal().end();
    }

    /// Give physics the saved geometry of every union changed since the
    /// last look.
    pub(crate) fn update_unions(&mut self) {
        self.check_unions();
        if self.unions.dirty.is_empty() {
            return;
        }
        profiling::scope!("update_unions");
        let mut w = self.runtime.world_mut();
        let w = &mut *w;
        let (dom, physics) = (&w.dom, &mut w.physics);
        let unions = &mut self.unions;
        let ws = dom.find_service("Workspace");
        let mut dirty: Vec<_> = unions.dirty.drain().collect();
        dirty.sort();
        for id in dirty {
            let in_workspace =
                dom.get(id).is_some() && ws.is_some_and(|ws| dom.is_ancestor_of(ws, id));
            let collision = match saved_blob(dom, id) {
                Some(blob) if in_workspace => match unions.saved.get(&blob) {
                    Some(c) => c.clone(),
                    None => {
                        let c = physics::decode(&blob).ok().map(Arc::new);
                        unions.saved.insert(blob, c.clone());
                        c
                    }
                },
                _ => None,
            };
            physics.set_union_collision(id, collision);
        }
    }
}
