//! Collision geometry saved in unions and MeshParts.
//!
//! A union (`UnionOperation`, `NegateOperation`, `IntersectOperation`) or
//! MeshPart collides and is queried as the hulls Studio saved in its
//! `PhysicalConfigData` (or, in older files, `PhysicsData`), and weighs what
//! they do. With none or a block it is its box. Studio doesn't work them out
//! again from the mesh, and uses them whatever the part's `CollisionFidelity`
//! says or whether its mesh loads
//! (`docs/behavior/physics/collision-shapes.md`).
//!
//! The geometry is in the part's own space at its `InitialSize`, so it is
//! scaled as its drawn geometry is.

use std::cell::RefCell;
use std::sync::Arc;

use rapier3d::prelude::*;
use rustc_hash::FxHashMap;

use glam::Vec3;
use rook_content::decode::mesh::union::physics;
use rook_dom::{Dom, InstanceId};
use rook_types::{Hull, SavedCollision, SavedMass, Value};

/// Shapes kept for reuse, keyed by geometry and scale; cleared when it
/// grows past this (scripts resizing parts every frame).
const CACHE_LIMIT: usize = 4096;

/// A geometry (by address) at a scale (by bits).
type ShapeKey = (usize, [u32; 3]);

#[derive(Default)]
pub(crate) struct SavedShapes {
    /// Geometry decoded so far, by the blob's hash or bytes: copies of a
    /// part share it. Asking for a part's mass decodes too, from `&self`.
    decoded: RefCell<FxHashMap<Vec<u8>, Option<Arc<SavedCollision>>>>,
    /// Copies of a part share their geometry, so their shapes are built once
    /// per size. The `Arc` keeps the key's address from being reused.
    cache: FxHashMap<ShapeKey, (Arc<SavedCollision>, Option<SharedShape>)>,
}

/// The saved blob of `property`, if it holds geometry or a block, with what
/// identifies it.
// A raw read: the blob is either of two value types, and a shared one is
// identified by its hash.
#[allow(clippy::disallowed_methods)]
fn blob(dom: &Dom, part: InstanceId, property: &str) -> Option<(Vec<u8>, Value)> {
    let value = dom.property_by_name(part, property)?;
    let key = match &value {
        Value::SharedString(s) => s.hash().as_bytes().to_vec(),
        Value::BinaryString(_) => bytes_of(&value).to_vec(),
        _ => return None,
    };
    physics::has_geometry(bytes_of(&value)).then_some((key, value))
}

fn bytes_of(value: &Value) -> &[u8] {
    match value {
        Value::SharedString(s) => s.data(),
        Value::BinaryString(b) => AsRef::<[u8]>::as_ref(b),
        _ => &[],
    }
}

/// The blob a part collides by, if any.
pub fn saved_blob(dom: &Dom, part: InstanceId) -> Option<Vec<u8>> {
    blob_of(dom, part).map(|(_, value)| bytes_of(&value).to_vec())
}

/// [`saved_blob`] and what identifies it. `PhysicalConfigData` comes first:
/// older files saved `PhysicsData` instead, and a file without
/// `PhysicalConfigData` reads as its default, a block.
fn blob_of(dom: &Dom, part: InstanceId) -> Option<(Vec<u8>, Value)> {
    if !dom.is_a(part, "TriangleMeshPart") {
        return None;
    }
    match blob(dom, part, "PhysicalConfigData") {
        Some(b) if physics::is_block(bytes_of(&b.1)) => blob(dom, part, "PhysicsData").or(Some(b)),
        other => other,
    }
}

impl SavedShapes {
    fn geometry(&self, dom: &Dom, part: InstanceId) -> Option<Arc<SavedCollision>> {
        let (key, value) = blob_of(dom, part)?;
        let mut decoded = self.decoded.borrow_mut();
        if let Some(known) = decoded.get(&key) {
            return known.clone();
        }
        let geometry = match physics::decode(bytes_of(&value)) {
            Ok(g) => Some(Arc::new(g)),
            Err(e) => {
                tracing::debug!(
                    target: "physics",
                    "{}: saved collision didn't decode: {e}",
                    dom.full_name(part)
                );
                None
            }
        };
        decoded.insert(key, geometry.clone());
        geometry
    }

    /// The shape `part` collides as, at `size`, if it has saved geometry;
    /// `None` leaves it its box.
    pub(crate) fn shape(&mut self, dom: &Dom, part: InstanceId, size: Vec3) -> Option<SharedShape> {
        let geometry = self.geometry(dom, part)?;
        let scale = hull_scale(&geometry, size, initial_size(dom, part))?;
        let key = (
            Arc::as_ptr(&geometry) as usize,
            scale.to_array().map(f32::to_bits),
        );
        if self.cache.len() >= CACHE_LIMIT {
            self.cache.clear();
        }
        let (_, shape) = self
            .cache
            .entry(key)
            .or_insert_with(|| (geometry.clone(), build(&geometry, scale)));
        shape.clone()
    }

    pub(crate) fn has_hulls(&self, dom: &Dom, part: InstanceId) -> bool {
        self.geometry(dom, part)
            .is_some_and(|g| matches!(*g, SavedCollision::Hulls { .. }))
    }

    /// What `part` weighs per unit density at `size`, if not a box's share.
    /// A part saved without `InitialSize` is its box (observed) though it
    /// collides as its hulls.
    pub(crate) fn mass(&self, dom: &Dom, part: InstanceId, size: Vec3) -> Option<SavedMass> {
        let geometry = self.geometry(dom, part)?;
        let SavedCollision::Hulls { mass, .. } = geometry.as_ref() else {
            return None;
        };
        let initial = initial_size(dom, part);
        initial
            .cmpgt(Vec3::ZERO)
            .all()
            .then(|| mass.scaled(size / initial))
    }
}

fn initial_size(dom: &Dom, part: InstanceId) -> Vec3 {
    dom.vector3_property(part, "InitialSize")
        .unwrap_or(Vec3::ZERO)
}

/// How far hulls stretch at `size`: from `InitialSize`, or from their own
/// bounds without one (observed in Studio 0.746).
fn hull_scale(geometry: &SavedCollision, size: Vec3, initial: Vec3) -> Option<Vec3> {
    let SavedCollision::Hulls { hulls, .. } = geometry else {
        return None;
    };
    if initial.cmpgt(Vec3::ZERO).all() {
        return Some(size / initial);
    }
    let (mut low, mut high) = (Vec3::INFINITY, Vec3::NEG_INFINITY);
    for p in hulls.iter().flat_map(|h| &h.positions) {
        low = low.min(Vec3::from_array(*p));
        high = high.max(Vec3::from_array(*p));
    }
    let bounds = high - low;
    bounds.cmpgt(Vec3::ZERO).all().then(|| size / bounds)
}

/// A hull's shape. Its saved faces make it directly; working the hull out
/// from its points took a 2,000-part place 2.8 s, and is left for hulls whose
/// faces don't close into one.
fn hull_shape(hull: &Hull, scale: Vec3) -> Option<SharedShape> {
    let points: Vec<Vector> = (hull.positions.iter())
        .map(|p| Vec3::from_array(*p) * scale)
        .collect();
    let faces = hull.indices.as_chunks::<3>().0;
    let in_range = faces.iter().flatten().all(|&i| (i as usize) < points.len());
    (in_range && !faces.is_empty())
        .then(|| SharedShape::convex_mesh(points.clone(), faces))
        .flatten()
        .or_else(|| SharedShape::convex_hull(&points))
}

/// Build a part's shape at `scale`. `None` for a block, or for geometry
/// with nothing solid in it.
fn build(geometry: &SavedCollision, scale: Vec3) -> Option<SharedShape> {
    profiling::scope!("saved shape");
    let SavedCollision::Hulls { hulls, .. } = geometry else {
        return None;
    };
    let mut pieces: Vec<(Pose, SharedShape)> = hulls
        .iter()
        .filter_map(|h| hull_shape(h, scale))
        .map(|s| (Pose::IDENTITY, s))
        .collect();
    match pieces.len() {
        0 => None,
        1 => pieces.pop().map(|(_, s)| s),
        _ => Some(SharedShape::compound(pieces)),
    }
}
