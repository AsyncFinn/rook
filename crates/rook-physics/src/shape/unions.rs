//! Unions' collision shapes.
//!
//! A union (`UnionOperation`, `NegateOperation`, `IntersectOperation`)
//! collides and is queried as the geometry Studio saved for it (its
//! `PhysicsData`, decoded by `rook-content` into [`UnionCollision`]), not
//! its box. The engine loads that geometry and hands it over
//! ([`crate::Physics::set_union_collision`]); until it does, and for a union with
//! none or a block, the union is its box. So is a moving union with `CanCollide` off
//! (see `Physics::add_collider`).
//!
//! The geometry is in the union's own space at its `InitialSize`, so it is
//! scaled as its drawn geometry is ([`mesh_part_scale`]).

use std::sync::Arc;

use rapier3d::prelude::*;
use rustc_hash::FxHashMap as HashMap;

use glam::Vec3;
use rook_dom::{Dom, InstanceId};
use rook_types::{UnionCollision, Value, mesh_part_scale};

use crate::convert::to_rapier;

/// Shapes kept for reuse, keyed by geometry and scale; cleared when it
/// grows past this (scripts resizing unions every frame).
const CACHE_LIMIT: usize = 4096;

/// A geometry (by address) at a scale (by bits).
type ShapeKey = (usize, [u32; 3]);

#[derive(Default)]
pub(crate) struct Unions {
    geometry: HashMap<InstanceId, Arc<UnionCollision>>,
    /// Copies of a union share their geometry, so their shapes are built
    /// once per size. The `Arc` keeps the key's address from being reused.
    cache: HashMap<ShapeKey, (Arc<UnionCollision>, Option<SharedShape>)>,
    /// Unions whose geometry changed since the last sync.
    pub(crate) reshaped: Vec<InstanceId>,
}

impl Unions {
    pub(crate) fn set(&mut self, part: InstanceId, collision: Option<Arc<UnionCollision>>) {
        let changed = match collision {
            Some(c) => self
                .geometry
                .insert(part, c.clone())
                .is_none_or(|old| !Arc::ptr_eq(&old, &c)),
            None => self.geometry.remove(&part).is_some(),
        };
        if changed {
            self.reshaped.push(part);
        }
    }

    /// The shape `part` collides as, at `size`, if it is a union with
    /// geometry; `None` leaves it its box.
    pub(crate) fn shape(&mut self, dom: &Dom, part: InstanceId, size: Vec3) -> Option<SharedShape> {
        let geometry = self.geometry.get(&part)?;

        let initial = match dom.property_by_name(part, "InitialSize") {
            Some(Value::Vector3(v)) => v.0,
            _ => Vec3::ZERO,
        };
        let scale = mesh_part_scale(size, initial);
        let key = (
            Arc::as_ptr(geometry) as usize,
            scale.to_array().map(f32::to_bits),
        );
        if self.cache.len() >= CACHE_LIMIT {
            self.cache.clear();
        }
        let (_, shape) = self
            .cache
            .entry(key)
            .or_insert_with(|| (geometry.clone(), build(geometry, scale)));
        shape.clone()
    }
}

/// Build a union's shape at `scale`. `None` for a block, or for geometry
/// with nothing solid in it.
fn build(geometry: &UnionCollision, scale: Vec3) -> Option<SharedShape> {
    profiling::scope!("union shape");
    let points = |positions: &[[f32; 3]]| -> Vec<Vector> {
        positions
            .iter()
            .map(|p| to_rapier(Vec3::from_array(*p) * scale))
            .collect()
    };
    match geometry {
        UnionCollision::Block => None,
        UnionCollision::Hulls(hulls) => {
            let mut pieces: Vec<(Pose, SharedShape)> = hulls
                .iter()
                .filter_map(|h| SharedShape::convex_hull(&points(&h.positions)))
                .map(|s| (Pose::IDENTITY, s))
                .collect();
            match pieces.len() {
                0 => None,
                1 => pieces.pop().map(|(_, s)| s),
                _ => Some(SharedShape::compound(pieces)),
            }
        }
    }
}
