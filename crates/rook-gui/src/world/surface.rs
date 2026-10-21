//! A SurfaceGui's canvas on its part's face: the canvas's size, and the
//! face it's stretched over (docs/behavior/gui/world.md#surfacegui).

use glam::{Vec2, Vec3};
use rook_dom::{Dom, InstanceId};
use rook_types::{NormalId, WorldPlace};

use super::Spot;

/// The canvas's right and up directions on each face, in the part's axes.
/// Measured: the sides match Decals; on the Top and Bottom the canvas is
/// turned a quarter from a Decal's, its x along −Z.
fn axes(face: NormalId) -> (Vec3, Vec3) {
    match face {
        NormalId::Top => (Vec3::NEG_Z, Vec3::NEG_X),
        NormalId::Bottom => (Vec3::NEG_Z, Vec3::X),
        side => side.image_axes(),
    }
}

#[derive(Debug, Clone)]
pub(super) struct Surface {
    face: NormalId,
    canvas: Vec2,
    /// `PixelsPerStud`, with SizingMode PixelsPerStud.
    per_stud: Option<f32>,
}

impl Surface {
    pub(super) fn of(dom: &Dom, gui: InstanceId) -> Self {
        let per_stud = dom.enum_item_property(gui, "SizingMode") == Some("PixelsPerStud");
        Self {
            face: dom
                .enum_property(gui, "Face")
                .and_then(NormalId::from_u32)
                .unwrap_or(NormalId::Front),
            canvas: dom
                .vector2_property(gui, "CanvasSize")
                .map_or(Vec2::ZERO, |v| v.0),
            per_stud: per_stud.then(|| dom.float_property(gui, "PixelsPerStud").unwrap_or(50.0)),
        }
    }

    pub(super) fn per_stud(&self) -> bool {
        self.per_stud.is_some()
    }

    /// The face's width and height in studs, along the canvas's axes.
    fn face_size(&self, part: Vec3) -> Vec2 {
        let (right, up) = axes(self.face);
        Vec2::new(right.dot(part).abs(), up.dot(part).abs())
    }

    /// `CanvasSize`, or with SizingMode PixelsPerStud the face's size
    /// times `PixelsPerStud` (measured: a Right face 1 × 2 studs at 25
    /// gave 25 × 50) when it's on a part.
    pub(super) fn canvas_size(&self, part: Option<Vec3>) -> Vec2 {
        match (self.per_stud, part) {
            (Some(per_stud), Some(part)) => self.face_size(part) * per_stud,
            _ => self.canvas,
        }
    }

    /// The canvas stretched over the face, its top left at the face's top
    /// left as seen from outside.
    pub(super) fn place(&self, part: &Spot) -> Option<WorldPlace> {
        let (right, up) = axes(self.face);
        let studs = self.face_size(part.size);
        let canvas = self.canvas_size(Some(part.size));
        if canvas.x <= 0.0 || canvas.y <= 0.0 {
            return None;
        }
        let center = self.face.normal() * part.size * 0.5;
        let corner = center - right * studs.x * 0.5 + up * studs.y * 0.5;
        let turn = part.frame.rotation;
        Some(WorldPlace::Surface {
            origin: part.frame.position + turn * corner,
            right: turn * right * (studs.x / canvas.x),
            down: turn * -up * (studs.y / canvas.y),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every canvas is seen unmirrored from outside.
    #[test]
    fn canvases_face_outward() {
        for face in NormalId::ALL {
            let (right, up) = axes(face);
            assert_eq!(right.cross(up), face.normal(), "{face:?}");
        }
    }
}
