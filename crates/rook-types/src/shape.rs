//! Primitive part shapes and their geometry: the one definition the
//! renderer's meshes, ray tests and physics colliders are all built from.

use glam::Vec3;

/// `Enum.PartType` shapes, plus the wedge classes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PartShape {
    Block,
    Wedge,
    CornerWedge,
    /// Along the part's X axis (Confirmed: creator-docs PartType).
    Cylinder,
    Ball,
}

impl PartShape {
    pub const ALL: [PartShape; 5] = [
        PartShape::Block,
        PartShape::Wedge,
        PartShape::CornerWedge,
        PartShape::Cylinder,
        PartShape::Ball,
    ];

    /// The shape of a part of class `class` whose `Shape` property (Part
    /// only) is `part_type`: `Enum.PartType` Ball 0, Block 1, Cylinder 2,
    /// Wedge 3, CornerWedge 4. MeshParts, unions and trusses count as blocks
    /// until meshes exist.
    pub fn of(class: &str, part_type: Option<u32>) -> Self {
        match class {
            "WedgePart" => Self::Wedge,
            "CornerWedgePart" => Self::CornerWedge,
            _ => match part_type {
                Some(0) => Self::Ball,
                Some(2) => Self::Cylinder,
                Some(3) => Self::Wedge,
                Some(4) => Self::CornerWedge,
                _ => Self::Block,
            },
        }
    }

    /// The box the shape actually fills for a part of `size`: a ball is a
    /// sphere of the smallest side; a cylinder runs along X with the smaller
    /// of Y and Z as its diameter (Confirmed: creator-docs PartType).
    pub fn extent(self, size: Vec3) -> Vec3 {
        match self {
            Self::Ball => Vec3::splat(size.min_element()),
            Self::Cylinder => {
                let d = size.y.min(size.z);
                Vec3::new(size.x, d, d)
            }
            _ => size,
        }
    }

    /// The faces of a flat-sided shape at unit size (extents -0.5..0.5),
    /// each a convex polygon wound counter-clockwise seen from outside.
    /// `None` for the round shapes.
    pub fn faces(self) -> Option<&'static [&'static [Vec3]]> {
        match self {
            Self::Block => Some(BLOCK),
            Self::Wedge => Some(WEDGE),
            Self::CornerWedge => Some(CORNER_WEDGE),
            Self::Cylinder | Self::Ball => None,
        }
    }

    /// The corners of a flat-sided shape at unit size.
    pub fn vertices(self) -> Option<Vec<Vec3>> {
        let mut out: Vec<Vec3> = Vec::new();
        for face in self.faces()? {
            for p in *face {
                if !out.contains(p) {
                    out.push(*p);
                }
            }
        }
        Some(out)
    }
}

const H: f32 = 0.5;

const fn v(x: f32, y: f32, z: f32) -> Vec3 {
    Vec3::new(x * H, y * H, z * H)
}

const BLOCK: &[&[Vec3]] = &[
    &[
        v(1., -1., 1.),
        v(1., -1., -1.),
        v(1., 1., -1.),
        v(1., 1., 1.),
    ], // +X
    &[
        v(-1., -1., -1.),
        v(-1., -1., 1.),
        v(-1., 1., 1.),
        v(-1., 1., -1.),
    ], // -X
    &[
        v(-1., 1., 1.),
        v(1., 1., 1.),
        v(1., 1., -1.),
        v(-1., 1., -1.),
    ], // +Y
    &[
        v(-1., -1., -1.),
        v(1., -1., -1.),
        v(1., -1., 1.),
        v(-1., -1., 1.),
    ], // -Y
    &[
        v(-1., -1., 1.),
        v(1., -1., 1.),
        v(1., 1., 1.),
        v(-1., 1., 1.),
    ], // +Z
    &[
        v(1., -1., -1.),
        v(-1., -1., -1.),
        v(-1., 1., -1.),
        v(1., 1., -1.),
    ], // -Z
];

/// Full bottom and back (+Z) faces; the slope faces the front (-Z) and rises
/// from the front bottom edge to the back top edge.
const WEDGE: &[&[Vec3]] = &[
    &[
        v(-1., -1., -1.),
        v(1., -1., -1.),
        v(1., -1., 1.),
        v(-1., -1., 1.),
    ], // bottom
    &[
        v(-1., -1., 1.),
        v(1., -1., 1.),
        v(1., 1., 1.),
        v(-1., 1., 1.),
    ], // back
    &[
        v(1., -1., -1.),
        v(-1., -1., -1.),
        v(-1., 1., 1.),
        v(1., 1., 1.),
    ], // slope
    &[v(1., -1., 1.), v(1., -1., -1.), v(1., 1., 1.)], // +X side
    &[v(-1., -1., -1.), v(-1., -1., 1.), v(-1., 1., 1.)], // -X side
];

/// A pyramid over the bottom face with its apex above the +X -Z corner:
/// vertical faces on +X and -Z, slopes facing -X and +Z (checked by eye
/// against archived places, 2026-10-11).
const CORNER_WEDGE: &[&[Vec3]] = &[
    &[
        v(-1., -1., -1.),
        v(1., -1., -1.),
        v(1., -1., 1.),
        v(-1., -1., 1.),
    ], // bottom
    &[v(1., -1., 1.), v(1., -1., -1.), v(1., 1., -1.)], // +X
    &[v(1., -1., -1.), v(-1., -1., -1.), v(1., 1., -1.)], // -Z
    &[v(-1., -1., 1.), v(1., -1., 1.), v(1., 1., -1.)], // +Z slope
    &[v(-1., -1., -1.), v(-1., -1., 1.), v(1., 1., -1.)], // -X slope
];

#[cfg(test)]
mod tests {
    use super::*;

    /// Every face is planar and faces away from the shape's centroid.
    #[test]
    fn faces_point_outward() {
        for shape in PartShape::ALL {
            let Some(faces) = shape.faces() else { continue };
            let verts = shape.vertices().unwrap();
            let centroid = verts.iter().copied().sum::<Vec3>() / verts.len() as f32;
            for face in faces {
                let n = (face[1] - face[0]).cross(face[2] - face[0]).normalize();
                assert!(n.dot(face[0] - centroid) > 0.0, "{shape:?}: inward face");
                for p in *face {
                    assert!(n.dot(*p - face[0]).abs() < 1e-6, "{shape:?}: bent face");
                }
            }
        }
    }
}
