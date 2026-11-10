pub(crate) mod saved;
pub(crate) mod terrain;

use rapier3d::prelude::*;
use rook_dom::{Dom, InstanceId};
use rook_types::{CFrame, PartShape};

use crate::convert::pose_of;
use crate::shape::saved::SavedShapes;

/// Roblox parts are at least this many studs on each side.
pub(crate) const MIN_PART_SIZE: f32 = 0.001;

/// A BasePart with a shape of its own: everything but Terrain, which
/// collides as its voxels' surface (`terrain`).
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

pub fn part_shape(dom: &Dom, id: InstanceId) -> PartShape {
    PartShape::of(dom.class_name(id), dom.enum_property(id, "Shape"))
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
            let points: Vec<Vector> = shape.vertices()?.into_iter().map(|p| p * size).collect();
            (SharedShape::convex_hull(&points)?, Pose::IDENTITY)
        }
    })
}

/// A part's collider shape placed in the world, read from the DOM (a
/// union's or MeshPart's from its saved geometry, if it has some).
pub(crate) fn world_shape(
    dom: &Dom,
    saved: &mut SavedShapes,
    part: InstanceId,
) -> Option<(SharedShape, Pose)> {
    let (shape, local) = local_shape(dom, saved, part)?;
    Some((
        shape,
        pose_of(
            &dom.cframe_property(part, "CFrame")
                .unwrap_or(CFrame::IDENTITY),
        ) * local,
    ))
}

/// A part's shape, and its pose relative to the part.
pub(crate) fn local_shape(
    dom: &Dom,
    saved: &mut SavedShapes,
    part: InstanceId,
) -> Option<(SharedShape, Pose)> {
    let size = dom.vector3_property(part, "Size")?;
    match saved.shape(dom, part, size) {
        Some(s) => Some((s, Pose::IDENTITY)),
        None => collider_shape(part_shape(dom, part), size),
    }
}

/// A part's `Size` box placed in the world, whatever its shape: what
/// Region3 and the `GetPartBounds*` queries test against (observed: a
/// ball counts in its box's corners, and a rotated block doesn't count in
/// its world bounding box's corners).
pub(crate) fn world_box(dom: &Dom, part: InstanceId) -> Option<(SharedShape, Pose)> {
    let (cf, size) = size_box(dom, part)?;
    let h = size * 0.5;
    Some((SharedShape::cuboid(h.x, h.y, h.z), pose_of(&cf)))
}

/// Where `part`'s `Size` box is and how big, at least [`MIN_PART_SIZE`]
/// each way, as the DOM has it now: what every query and rule that goes
/// by a part's box reads.
///
/// `None` for a box with a NaN or infinite number in it, whatever wrote
/// it: scripts can't (`CFrame::written`), but a file or a body thrown far
/// enough could, and nothing physical can be made of it.
pub(crate) fn size_box(dom: &Dom, part: InstanceId) -> Option<(CFrame, glam::Vec3)> {
    let size = dom.vector3_property(part, "Size")?;
    let cf = dom
        .cframe_property(part, "CFrame")
        .unwrap_or(CFrame::IDENTITY);
    let finite = size.is_finite() && cf.position.is_finite() && cf.rotation.is_finite();
    finite.then(|| (cf, size.max(glam::Vec3::splat(MIN_PART_SIZE))))
}

/// The world-axis-aligned box around `part`'s `Size` box, as its lowest
/// and highest corners.
pub fn part_box(dom: &Dom, part: InstanceId) -> Option<(glam::Vec3, glam::Vec3)> {
    let (cf, size) = size_box(dom, part)?;
    let reach = cf.box_reach(size);
    Some((cf.position - reach, cf.position + reach))
}
