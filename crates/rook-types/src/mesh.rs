//! Built-in mesh shapes: what a part with a `SpecialMesh`, `BlockMesh` or
//! `CylinderMesh` child draws, and at what size. A mesh changes only how a
//! part looks; physics and queries keep the part's own shape.
//!
//! The rules were measured in Studio (ROADMAP.md, M4: "What each
//! SpecialMesh MeshType draws" and "How `Scale` combines with Size").

use glam::Vec3;

use crate::PartShape;

/// The kind of a part's mesh: a `DataModelMesh` class and, for a
/// `SpecialMesh`, its `MeshType`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MeshKind {
    /// A primitive shape fitted to the scaled size. Brick and Torso (the
    /// same since 2018) and `BlockMesh` are blocks; Sphere is a ball
    /// stretched on every axis (creator-docs); CornerWedge is the same shape
    /// as a CornerWedgePart (observed by overlapping the two).
    Shape(PartShape),
    /// `CylinderMesh`: a cylinder standing along the part's Y axis, unlike
    /// the SpecialMesh Cylinder, which lies along X (observed).
    UprightCylinder,
    /// The Head mesh (`content/avatar/heads/head.mesh` in Roblox).
    Head,
    /// Prism, Pyramid, ParallelRamp and RightAngleRamp draw nothing
    /// (observed: the part disappears but still collides).
    Nothing,
    /// A mesh file (`FileMesh`, or `MeshType` FileMesh). Not loaded yet:
    /// the part keeps its own shape (stubs.md).
    File,
    /// A mesh file with an empty `MeshId`: a 1 × 1 × 1 box times `Scale`,
    /// whatever the part's Size (observed for both the FileMesh class and
    /// SpecialMesh FileMesh, from the front and from above).
    EmptyFile,
}

impl MeshKind {
    /// The kind of a `DataModelMesh` of class `class`; `mesh_type` is a
    /// SpecialMesh's `Enum.MeshType` value (Head 0, Torso 1, Wedge 2,
    /// Sphere 3, Cylinder 4, FileMesh 5, Brick 6, Prism 7, Pyramid 8,
    /// ParallelRamp 9, RightAngleRamp 10, CornerWedge 11) and `mesh_id` its
    /// `MeshId`. `None` for a class that isn't a mesh.
    pub fn of(class: &str, mesh_type: Option<u32>, mesh_id: &str) -> Option<Self> {
        let file = if mesh_id.is_empty() {
            Self::EmptyFile
        } else {
            Self::File
        };
        Some(match class {
            "BlockMesh" => Self::Shape(PartShape::Block),
            "CylinderMesh" => Self::UprightCylinder,
            "FileMesh" => file,
            "SpecialMesh" => match mesh_type.unwrap_or(0) {
                0 => Self::Head,
                1 | 6 => Self::Shape(PartShape::Block),
                2 => Self::Shape(PartShape::Wedge),
                3 => Self::Shape(PartShape::Ball),
                4 => Self::Shape(PartShape::Cylinder),
                11 => Self::Shape(PartShape::CornerWedge),
                7..=10 => Self::Nothing,
                _ => file,
            },
            _ => return None,
        })
    }

    /// What a part of `size` draws with this mesh at `scale`. `Scale`
    /// multiplies the part's Size per axis first, and the type's own rule
    /// then works on that scaled size (observed: a CylinderMesh on a
    /// 2 × 3 × 4 part at Scale (0.5, 1.5, 0.75) was 1 stud across, not 1.5).
    pub fn look(self, size: Vec3, scale: Vec3) -> MeshLook {
        let s = size * scale;
        match self {
            // A Sphere stretches, so it skips the ball's own extent rule.
            Self::Shape(PartShape::Ball) => MeshLook::Shape {
                shape: PartShape::Ball,
                size: s,
                upright: false,
            },
            Self::Shape(shape) => MeshLook::Shape {
                shape,
                size: shape.extent(s),
                upright: false,
            },
            // The X-axis cylinder turned onto Y: its length is S.Y and its
            // diameter min(S.X, S.Z).
            Self::UprightCylinder => MeshLook::Shape {
                shape: PartShape::Cylinder,
                size: PartShape::Cylinder.extent(Vec3::new(s.y, s.x, s.z)),
                upright: true,
            },
            Self::Head => MeshLook::Head {
                scale: head_scale(s),
            },
            Self::Nothing => MeshLook::Nothing,
            Self::File => MeshLook::PartShape,
            Self::EmptyFile => MeshLook::Shape {
                shape: PartShape::Block,
                size: scale,
                upright: false,
            },
        }
    }
}

/// How the head mesh is scaled for a (scaled) part size: by
/// (min(X, Z), Y, min(X, Z)) / 1.25, so it is round from above and never
/// stretched along the longer of X and Z (observed by overlapping it with a
/// FileMesh of the same mesh, ROADMAP M4 "The Head MeshType").
pub fn head_scale(size: Vec3) -> Vec3 {
    let across = size.x.min(size.z);
    Vec3::new(across, size.y, across) / 1.25
}

/// What a part with a mesh draws.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum MeshLook {
    /// A primitive shape of `size` (in the shape's own axes). `upright`
    /// turns the shape's X axis onto the part's Y axis (a CylinderMesh).
    Shape {
        shape: PartShape,
        size: Vec3,
        upright: bool,
    },
    /// The head mesh, at its own size times `scale`.
    Head { scale: Vec3 },
    /// Nothing at all.
    Nothing,
    /// The part's own shape, as if it had no mesh.
    PartShape,
}

#[cfg(test)]
mod tests {
    use super::*;

    const SIZE: Vec3 = Vec3::new(2.0, 3.0, 4.0);
    const SCALE: Vec3 = Vec3::new(0.5, 1.5, 0.75);

    fn look(class: &str, mesh_type: Option<u32>, scale: Vec3) -> MeshLook {
        MeshKind::of(class, mesh_type, "")
            .unwrap()
            .look(SIZE, scale)
    }

    fn shape(shape: PartShape, size: Vec3, upright: bool) -> MeshLook {
        MeshLook::Shape {
            shape,
            size,
            upright,
        }
    }

    /// At Scale 1 every type draws at the part's Size (measured on a
    /// 2 × 3 × 4 part).
    #[test]
    fn scale_one_draws_at_size() {
        for (mesh_type, s) in [
            (1, PartShape::Block),
            (6, PartShape::Block),
            (2, PartShape::Wedge),
            (11, PartShape::CornerWedge),
            (3, PartShape::Ball),
        ] {
            assert_eq!(
                look("SpecialMesh", Some(mesh_type), Vec3::ONE),
                shape(s, SIZE, false)
            );
        }
        assert_eq!(
            look("BlockMesh", None, Vec3::ONE),
            shape(PartShape::Block, SIZE, false)
        );
        // Cylinder 3 across, CylinderMesh 2 across.
        assert_eq!(
            look("SpecialMesh", Some(4), Vec3::ONE),
            shape(PartShape::Cylinder, Vec3::new(2.0, 3.0, 3.0), false)
        );
        assert_eq!(
            look("CylinderMesh", None, Vec3::ONE),
            shape(PartShape::Cylinder, Vec3::new(3.0, 2.0, 2.0), true)
        );
    }

    /// Scale applies before each type's shape rule: S = (1, 4.5, 3).
    #[test]
    fn scale_applies_before_the_shape_rule() {
        let s = SIZE * SCALE;
        assert_eq!(
            look("SpecialMesh", Some(3), SCALE),
            shape(PartShape::Ball, s, false)
        );
        assert_eq!(
            look("BlockMesh", None, SCALE),
            shape(PartShape::Block, s, false)
        );
        // 1 long, 3 across.
        assert_eq!(
            look("SpecialMesh", Some(4), SCALE),
            shape(PartShape::Cylinder, Vec3::new(1.0, 3.0, 3.0), false)
        );
        // 4.5 tall, 1 across (Scale after the rule would give 1.5 across Z).
        assert_eq!(
            look("CylinderMesh", None, SCALE),
            shape(PartShape::Cylinder, Vec3::new(4.5, 1.0, 1.0), true)
        );
    }

    #[test]
    fn head_is_fitted_to_the_smaller_of_x_and_z() {
        let MeshLook::Head { scale } = look("SpecialMesh", Some(0), SCALE) else {
            panic!("not a head");
        };
        // S = (1, 4.5, 3): 0.8 × 3.6 × 0.8 of the mesh.
        assert!((scale - Vec3::new(0.8, 3.6, 0.8)).abs().max_element() < 1e-6);
        // Never stretched along the longer side.
        assert_eq!(
            head_scale(Vec3::new(2.0, 1.0, 50.0)),
            head_scale(Vec3::new(50.0, 1.0, 2.0))
        );
        // The classic R6 head (2 × 1 × 1, Scale 1.25) draws the mesh at its
        // own size.
        assert_eq!(
            MeshKind::Head.look(Vec3::new(2.0, 1.0, 1.0), Vec3::splat(1.25)),
            MeshLook::Head { scale: Vec3::ONE }
        );
    }

    #[test]
    fn deprecated_types_draw_nothing_and_file_meshes() {
        for mesh_type in 7..=10 {
            assert_eq!(
                look("SpecialMesh", Some(mesh_type), Vec3::ONE),
                MeshLook::Nothing
            );
        }
        let id = "rbxasset://fonts/head.mesh";
        for (class, mesh_type) in [("SpecialMesh", Some(5)), ("FileMesh", None)] {
            let kind = MeshKind::of(class, mesh_type, id).unwrap();
            assert_eq!(kind.look(SIZE, Vec3::ONE), MeshLook::PartShape);
            // Without a MeshId: a unit box times Scale, Size ignored.
            assert_eq!(
                look(class, mesh_type, SCALE),
                shape(PartShape::Block, SCALE, false)
            );
        }
        assert_eq!(MeshKind::of("Part", None, ""), None);
    }
}
