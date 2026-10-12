//! Part shapes and their Rapier collider shapes.

use rapier3d::prelude::*;
use rook_dom::{Dom, InstanceId};
use rook_types::{PartShape, Value};

use crate::convert::to_rapier;

/// Roblox parts are at least this many studs on each side.
pub(crate) const MIN_PART_SIZE: f32 = 0.001;

/// A BasePart with a shape of its own: everything but Terrain, which is
/// drawn and simulated separately (M8).
pub fn is_part(dom: &Dom, id: InstanceId) -> bool {
    dom.is_a(id, "BasePart") && dom.class_name(id) != "Terrain"
}

/// Every part in Workspace, in tree order.
pub fn workspace_parts(dom: &Dom) -> Vec<InstanceId> {
    let Some(ws) = dom.find_service("Workspace") else {
        return Vec::new();
    };
    let mut parts = dom.descendants(ws);
    parts.retain(|id| is_part(dom, *id));
    parts
}

/// A part's shape, from its class and `Shape` property.
pub fn part_shape(dom: &Dom, id: InstanceId) -> PartShape {
    let part_type = match dom.property_by_name(id, "Shape") {
        Some(Value::Enum(n)) => Some(n),
        _ => None,
    };
    PartShape::of(dom.class_name(id), part_type)
}

/// The collider shape for a part of this shape and size, and its pose
/// relative to the part. `None` for a degenerate (zero-size) part.
pub(crate) fn collider_shape(shape: PartShape, size: glam::Vec3) -> Option<(SharedShape, Pose)> {
    let size = shape.extent(size.max(glam::Vec3::splat(MIN_PART_SIZE)));
    let h = size * 0.5;
    Some(match shape {
        PartShape::Block => (SharedShape::cuboid(h.x, h.y, h.z), Pose::IDENTITY),
        PartShape::Ball => (SharedShape::ball(h.x), Pose::IDENTITY),
        // Rapier's cylinders run along Y; Roblox's along X.
        PartShape::Cylinder => (
            SharedShape::cylinder(h.x, h.y),
            Pose::from_rotation(Rotation::from_rotation_z(-std::f32::consts::FRAC_PI_2)),
        ),
        PartShape::Wedge | PartShape::CornerWedge => {
            let points: Vec<Vector> = shape
                .vertices()?
                .into_iter()
                .map(|p| to_rapier(p * size))
                .collect();
            (SharedShape::convex_hull(&points)?, Pose::IDENTITY)
        }
    })
}
