//! Part shapes and their Rapier collider shapes.

use rapier3d::prelude::*;
use rook_dom::{Dom, InstanceId};
use rook_types::Value;

use crate::convert::to_rapier;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PartShape {
    Block,
    Wedge,
    CornerWedge,
    /// Along the part's X axis.
    Cylinder,
    Ball,
}

/// `Enum.PartType`: Ball 0, Block 1, Cylinder 2, Wedge 3, CornerWedge 4.
/// MeshParts, unions and trusses count as blocks until meshes exist.
pub fn part_shape(dom: &Dom, id: InstanceId) -> PartShape {
    match dom.class_name(id) {
        "WedgePart" => PartShape::Wedge,
        "CornerWedgePart" => PartShape::CornerWedge,
        _ => match dom.property_by_name(id, "Shape") {
            Some(Value::Enum(0)) => PartShape::Ball,
            Some(Value::Enum(2)) => PartShape::Cylinder,
            Some(Value::Enum(3)) => PartShape::Wedge,
            Some(Value::Enum(4)) => PartShape::CornerWedge,
            _ => PartShape::Block,
        },
    }
}

/// The collider shape for a part of this shape and size, and its pose
/// relative to the part. `None` for a degenerate (zero-size) part.
pub(crate) fn collider_shape(shape: PartShape, size: glam::Vec3) -> Option<(SharedShape, Pose)> {
    // Roblox parts are at least 0.001 studs on each side.
    let h = size.max(glam::Vec3::splat(0.001)) * 0.5;
    let v = |x: f32, y: f32, z: f32| to_rapier(glam::Vec3::new(x * h.x, y * h.y, z * h.z));
    Some(match shape {
        PartShape::Block => (SharedShape::cuboid(h.x, h.y, h.z), Pose::IDENTITY),
        PartShape::Ball => (SharedShape::ball(h.min_element()), Pose::IDENTITY),
        // Rapier's cylinders run along Y; Roblox's along X.
        PartShape::Cylinder => (
            SharedShape::cylinder(h.x, h.y.min(h.z)),
            Pose::from_rotation(Rotation::from_rotation_z(-std::f32::consts::FRAC_PI_2)),
        ),
        // Full bottom and back (+Z) faces; the slope runs from the front
        // bottom edge up to the back top edge.
        PartShape::Wedge => (
            SharedShape::convex_hull(&[
                v(-1.0, -1.0, -1.0),
                v(1.0, -1.0, -1.0),
                v(-1.0, -1.0, 1.0),
                v(1.0, -1.0, 1.0),
                v(-1.0, 1.0, 1.0),
                v(1.0, 1.0, 1.0),
            ])?,
            Pose::IDENTITY,
        ),
        // The bottom face and an apex above the +X -Z corner.
        PartShape::CornerWedge => (
            SharedShape::convex_hull(&[
                v(-1.0, -1.0, -1.0),
                v(1.0, -1.0, -1.0),
                v(-1.0, -1.0, 1.0),
                v(1.0, -1.0, 1.0),
                v(1.0, 1.0, -1.0),
            ])?,
            Pose::IDENTITY,
        ),
    })
}
