//! How images map onto a part's faces: the one definition the
//! renderer's shaders and the scene builder follow. Measured in Studio
//! 0.742 on 2026-10-13 with a labelled test image (ROADMAP M4 "Textures and
//! decals (step 4)").
//!
//! - Each face has an image frame: seen from outside, the image is never
//!   mirrored; on the four sides its up is the part's +Y; on Top and Bottom
//!   its up is the part's +Z (Back). A Decal stretches the image over the
//!   face; a Texture tiles from the frame's top-left corner.
//! - On round and sloped shapes each point belongs to the face whose axis
//!   its normal is closest to, and the face's image is projected straight
//!   along that axis (a Front decal on a ball covers the front of it; a
//!   cylinder's curved side shows its Front, Top, Back and Bottom images).

use glam::{Vec2, Vec3};

/// `Enum.NormalId`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum NormalId {
    Right,
    Top,
    Back,
    Left,
    Bottom,
    Front,
}

impl NormalId {
    /// In `Enum.NormalId` order (Right 0 ... Front 5).
    pub const ALL: [NormalId; 6] = [
        Self::Right,
        Self::Top,
        Self::Back,
        Self::Left,
        Self::Bottom,
        Self::Front,
    ];

    pub fn from_u32(value: u32) -> Option<Self> {
        Self::ALL.get(value as usize).copied()
    }

    pub fn index(self) -> usize {
        self as usize
    }

    /// The face's outward normal in the part's axes.
    pub fn normal(self) -> Vec3 {
        match self {
            Self::Right => Vec3::X,
            Self::Top => Vec3::Y,
            Self::Back => Vec3::Z,
            Self::Left => Vec3::NEG_X,
            Self::Bottom => Vec3::NEG_Y,
            Self::Front => Vec3::NEG_Z,
        }
    }

    /// The image's right and up directions on this face, in the part's
    /// axes (observed: see the module docs).
    pub fn image_axes(self) -> (Vec3, Vec3) {
        match self {
            Self::Front => (Vec3::NEG_X, Vec3::Y),
            Self::Back => (Vec3::X, Vec3::Y),
            Self::Right => (Vec3::NEG_Z, Vec3::Y),
            Self::Left => (Vec3::Z, Vec3::Y),
            Self::Top => (Vec3::NEG_X, Vec3::Z),
            Self::Bottom => (Vec3::X, Vec3::Z),
        }
    }

    /// The face that a surface with normal `n` (in the part's axes) shows:
    /// the one whose axis `n` is closest to. On a tie, Z wins over X and X
    /// over Y (observed for Z over Y: a Front decal covered a 45-degree
    /// wedge slope, its Top decal didn't; the X order is Inference).
    pub fn of_normal(n: Vec3) -> Self {
        let a = n.abs();
        if a.z >= a.x && a.z >= a.y {
            if n.z >= 0.0 { Self::Back } else { Self::Front }
        } else if a.x >= a.y {
            if n.x >= 0.0 { Self::Right } else { Self::Left }
        } else if n.y >= 0.0 {
            Self::Top
        } else {
            Self::Bottom
        }
    }

    /// Where `p` (studs from the centre of a box of `size`, in the part's
    /// axes) is on this face's image, in studs from its top-left corner,
    /// and the face's size in studs (width, height).
    pub fn image_position(self, p: Vec3, size: Vec3) -> (Vec2, Vec2) {
        let (right, up) = self.image_axes();
        let extent = Vec2::new(right.abs().dot(size), up.abs().dot(size));
        let at = Vec2::new(p.dot(right) + extent.x / 2.0, extent.y / 2.0 - p.dot(up));
        (at, extent)
    }
}

/// How a Texture repeats its image (`StudsPerTileU`/`V`, `OffsetStudsU`/
/// `V`): the image coordinate at `s` studs from the face's top-left corner
/// is (s + offset) / studs per tile (observed: on a 5 × 3.5 face with tiles
/// 2 × 1.5 the whole tiles start at the top-left corner, and an offset of
/// 0.5 studs moved them 0.5 studs towards the left).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Tiling {
    pub studs_per_tile: Vec2,
    pub offset: Vec2,
}

impl Tiling {
    pub fn uv(self, studs: Vec2) -> Vec2 {
        (studs + self.offset) / self.studs_per_tile
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every image frame is seen unmirrored from outside: right × up is
    /// the outward normal.
    #[test]
    fn image_frames_face_outward() {
        for face in NormalId::ALL {
            let (right, up) = face.image_axes();
            assert_eq!(right.cross(up), face.normal(), "{face:?}");
            assert_eq!(NormalId::of_normal(face.normal()), face);
        }
    }

    #[test]
    fn positions_on_a_face() {
        let size = Vec3::new(4.0, 3.0, 2.0);
        // The Front face's top-left corner (seen from the front) is at +X,
        // +Y; its image is 4 wide and 3 tall.
        let (at, extent) = NormalId::Front.image_position(Vec3::new(2.0, 1.5, -1.0), size);
        assert_eq!((at, extent), (Vec2::ZERO, Vec2::new(4.0, 3.0)));
        // The Top face's top-left corner is at +X, +Z (Back).
        let (at, extent) = NormalId::Top.image_position(Vec3::new(2.0, 1.5, 1.0), size);
        assert_eq!((at, extent), (Vec2::ZERO, Vec2::new(4.0, 2.0)));
        let (at, _) = NormalId::Bottom.image_position(Vec3::new(2.0, -1.5, -1.0), size);
        assert_eq!(at, Vec2::new(4.0, 2.0));
    }

    #[test]
    fn ties_go_to_z_then_x() {
        let slope = Vec3::new(0.0, 1.0, -1.0).normalize();
        assert_eq!(NormalId::of_normal(slope), NormalId::Front);
        assert_eq!(
            NormalId::of_normal(Vec3::new(1.0, 1.0, 0.0)),
            NormalId::Right
        );
    }

    #[test]
    fn tiling() {
        let t = Tiling {
            studs_per_tile: Vec2::new(2.0, 1.5),
            offset: Vec2::new(0.5, 0.0),
        };
        assert_eq!(t.uv(Vec2::new(1.5, 3.0)), Vec2::new(1.0, 2.0));
    }
}
