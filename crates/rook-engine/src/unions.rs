//! Unions' collision geometry: read from the file or loaded from their
//! asset, and handed to physics, which collides and queries unions as it
//! (`rook_physics::unions`).
//!
//! Every DataModel does this, server and client alike: the client's
//! world answers the mouse's queries. A union in Workspace with
//! `CollisionFidelity` Box is its box. Otherwise its geometry is the
//! `PhysicalConfigData` or `PhysicsData` saved in it, else what its
//! asset (`AssetId`) holds: saved hulls, or its drawn geometry when it has
//! none. Until that loads, and if it fails, the union is its box.
//!
//! A place's unions get theirs before its scripts start; after that,
//! changes are noticed at the end of a frame ([`DataModel::check_unions`],
//! before the journal is trimmed) and acted on at the start of the next.

use std::collections::HashSet;

use rook_content::mesh::union::physics;
use rook_content::{ContentRef, Failure, Fetched, Ticket};
use rook_types::UnionCollision;

use super::*;

/// `Enum.CollisionFidelity.Box`.
const FIDELITY_BOX: u32 = 2;

/// What one union asset came to.
#[derive(Debug, Clone)]
enum Load {
    Loading,
    Loaded(Arc<UnionCollision>),
    Failed,
}

#[derive(Debug, Default)]
pub(crate) struct Unions {
    /// Journal position up to which changes have been looked at.
    cursor: u64,
    /// Whether every union has been looked at once.
    scanned: bool,
    /// Unions to look at again.
    dirty: HashSet<InstanceId>,
    /// Unions waiting for their asset.
    waiting: HashMap<InstanceId, ContentRef>,
    /// Each asset once, however many unions share it.
    assets: HashMap<ContentRef, Load>,
    pub(crate) tickets: HashMap<Ticket, ContentRef>,
    /// Assets to hand the loader at its next run.
    pub(crate) queued: Vec<ContentRef>,
    /// Geometry saved in the file, by its bytes: copies share it.
    saved: HashMap<Vec<u8>, Option<Arc<UnionCollision>>>,
}

/// The properties whose change gives a union new geometry.
fn is_geometry_property(name: &str) -> bool {
    matches!(
        name,
        "PhysicalConfigData" | "PhysicsData" | "Content" | "AssetId" | "CollisionFidelity"
    )
}

impl Unions {
    /// Look at every union again (a client DataModel filled by hand).
    pub(crate) fn rescan(&mut self, cursor: u64) {
        self.cursor = cursor;
        self.scanned = false;
    }
}

/// The physics blob saved in a union, if it holds geometry. A block
/// doesn't count: it is also what a file without `PhysicalConfigData`
/// reads as (the property's default), and a union that collides as its
/// box has `CollisionFidelity` Box.
fn saved_blob(dom: &Dom, id: InstanceId) -> Option<Vec<u8>> {
    ["PhysicalConfigData", "PhysicsData"]
        .iter()
        .find_map(|name| {
            let bytes = match dom.property_by_name(id, name)? {
                Value::BinaryString(b) => b.into_vec(),
                Value::SharedString(s) => s.data().to_vec(),
                _ => return None,
            };
            (physics::has_geometry(&bytes) && !physics::is_block(&bytes)).then_some(bytes)
        })
}

fn asset(dom: &Dom, id: InstanceId) -> Option<ContentRef> {
    let value = dom
        .property_by_name(id, "Content")
        .or_else(|| dom.property_by_name(id, "AssetId"))?;
    ContentRef::parse(value.content_url()?)
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

    /// Look at every union changed since the last look: give physics each
    /// one's saved geometry, and queue the assets to load for the rest.
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
            unions.waiting.remove(&id);
            let in_workspace =
                dom.get(id).is_some() && ws.is_some_and(|ws| dom.is_ancestor_of(ws, id));
            if !in_workspace {
                physics.set_union_collision(id, None);
                continue;
            }
            if dom.property_by_name(id, "CollisionFidelity") == Some(Value::Enum(FIDELITY_BOX)) {
                physics.set_union_collision(id, None);
                continue;
            }
            if let Some(blob) = saved_blob(dom, id) {
                let collision = match unions.saved.get(&blob) {
                    Some(c) => c.clone(),
                    None => {
                        let c = physics::decode(&blob).ok().map(Arc::new);
                        unions.saved.insert(blob, c.clone());
                        c
                    }
                };
                physics.set_union_collision(id, collision);
                continue;
            }
            let Some(r) = asset(dom, id) else {
                physics.set_union_collision(id, None);
                continue;
            };
            let load = unions
                .assets
                .entry(r.clone())
                .or_insert_with(|| {
                    unions.queued.push(r.clone());
                    Load::Loading
                })
                .clone();
            if let Load::Loading = load {
                unions.waiting.insert(id, r);
            }
            let loaded = match load {
                Load::Loaded(c) => Some(c),
                Load::Loading | Load::Failed => None,
            };
            physics.set_union_collision(id, loaded);
        }
    }

    /// A union asset finished loading: every union waiting for it gets its
    /// geometry, or stays its box.
    pub(crate) fn union_loaded(&mut self, r: ContentRef, outcome: Result<Fetched, Failure>) {
        let load = match outcome.ok().and_then(|f| f.collision) {
            Some(c) => Load::Loaded(c),
            None => Load::Failed,
        };
        let mut w = self.runtime.world_mut();
        let mut users: Vec<_> = self
            .unions
            .waiting
            .iter()
            .filter(|(_, used)| **used == r)
            .map(|(&id, _)| id)
            .collect();
        users.sort();
        for id in users {
            self.unions.waiting.remove(&id);
            if let Load::Loaded(c) = &load {
                w.physics.set_union_collision(id, Some(c.clone()));
            }
        }
        self.unions.assets.insert(r, load);
    }
}
