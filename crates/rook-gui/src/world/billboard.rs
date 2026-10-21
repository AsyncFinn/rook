//! A BillboardGui: where it hangs and how big it is
//! (docs/behavior/gui-world.md#billboardgui). `Size`'s scales are studs,
//! seen at the billboard's depth; its offsets are pixels.

use glam::{Vec2, Vec3};
use rook_dom::{Dom, InstanceId};
use rook_types::WorldPlace;

use super::{Eye, Spot};

#[derive(Debug, Clone)]
pub(super) struct Billboard {
    /// `StudsOffsetWorldSpace` and `ExtentsOffsetWorldSpace`.
    world_studs: Vec3,
    world_extents: Vec3,
    /// `StudsOffset` and `ExtentsOffset`, in the camera's frame.
    studs: Vec3,
    extents: Vec3,
    /// `Size`'s scales and offsets.
    size_studs: Vec2,
    size_pixels: Vec2,
    shift: Vec2,
}

impl Billboard {
    pub(super) fn of(dom: &Dom, gui: InstanceId) -> Self {
        let v3 = |name| dom.vector3_property(gui, name).unwrap_or(Vec3::ZERO);
        let size = dom.udim2_property(gui, "Size").unwrap_or_default();
        Self {
            world_studs: v3("StudsOffsetWorldSpace"),
            world_extents: v3("ExtentsOffsetWorldSpace"),
            studs: v3("StudsOffset"),
            extents: v3("ExtentsOffset"),
            size_studs: Vec2::new(size.x.scale, size.y.scale),
            size_pixels: Vec2::new(size.x.offset as f32, size.y.offset as f32),
            // Measured: SizeOffset (1, 1) moved it right and up by its size.
            shift: dom
                .vector2_property(gui, "SizeOffset")
                .map_or(Vec2::ZERO, |v| v.0),
        }
    }

    pub(super) fn in_studs(&self) -> bool {
        self.size_studs != Vec2::ZERO
    }

    /// Where it hangs before the offsets in the camera's frame. Measured:
    /// `ExtentsOffset` 1 is half the part's height; Inference: an
    /// Attachment has no extents.
    fn anchor(&self, on: &Spot) -> Vec3 {
        on.frame.position + self.world_studs + self.world_extents * on.size * 0.5
    }

    /// Offsets in the camera's frame: x right, y up, z towards the camera.
    fn camera_studs(&self, on: &Spot) -> Vec3 {
        self.studs + self.extents * on.size * 0.5
    }

    /// `Size` in pixels: its offsets, plus its scales in studs seen at its
    /// depth from `eye` (Measured: 4 studs at depth 40 drew 56 pixels high
    /// in a 788-pixel window at FieldOfView 70; offsets stay pixels).
    /// Whole pixels (Inference: Studio's `AbsoluteSize` reads whole
    /// numbers). Away from the world or the camera, only its offsets.
    pub(super) fn pixel_size(&self, on: Option<&Spot>, eye: Option<&Eye>) -> Vec2 {
        let (Some(on), Some(eye), true) = (on, eye, self.in_studs()) else {
            return self.size_pixels;
        };
        let point = self.anchor(on) + eye.rotation * self.camera_studs(on);
        let depth = eye.depth(point).max(0.1);
        (self.size_pixels + self.size_studs * eye.pixels_per_stud / depth).round()
    }

    pub(super) fn place(&self, on: &Spot) -> WorldPlace {
        WorldPlace::Billboard {
            anchor: self.anchor(on),
            studs: self.camera_studs(on),
            size_studs: self.size_studs,
            size_pixels: self.size_pixels,
            shift: self.shift,
        }
    }
}
