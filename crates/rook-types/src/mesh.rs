//! What a part with a mesh draws, and at what size: a `SpecialMesh`,
//! `BlockMesh`, `CylinderMesh` or `FileMesh` child, or a MeshPart's own
//! mesh. A mesh changes only how a part looks; physics and queries keep the
//! part's own shape.
//!
//! The rules were measured in Studio (ROADMAP.md, M4: "What each
//! SpecialMesh MeshType draws", "How `Scale` combines with Size" and "File
//! meshes (step 3)").

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
    /// A mesh file (`FileMesh`, or `MeshType` FileMesh), sized by `Scale`
    /// alone (observed: it ignores the part's Size).
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

    /// Whether Decals on the part are drawn through the mesh's own UVs,
    /// whatever their `Face`, instead of projected onto a face (observed: a
    /// Decal on the Head, or on a FileMesh of the head or of a cube, covered
    /// the whole mesh through its UVs, and a Top or Right decal drew the
    /// same as a Front one). The empty-`MeshId` box follows the file meshes
    /// (Inference).
    pub fn decals_follow_uvs(self) -> bool {
        matches!(self, Self::Head | Self::File | Self::EmptyFile)
    }

    /// What a mesh child draws on a part of class `class` and `size`, or
    /// `None` when the part ignores it (observed in Studio 0.742,
    /// 2026-10-13):
    ///
    /// - A MeshPart ignores mesh children: a Sphere SpecialMesh under one
    ///   left its own mesh drawn.
    /// - On a union the mesh is drawn as if Size and `Scale` were 1: a
    ///   Sphere mesh on a 4 × 4 × 4 union, and a Brick mesh at Scale
    ///   (2, 3, 1), each drew a 1-stud shape (Studio also left it untinted,
    ///   which Rook doesn't copy).
    /// - Every other part, trusses included, follows [`look`](Self::look)
    ///   (a Sphere mesh filled a 2 × 10 × 2 truss).
    pub fn look_on(self, class: &str, size: Vec3, scale: Vec3) -> Option<MeshLook> {
        match class {
            "MeshPart" => None,
            "UnionOperation" | "NegateOperation" | "IntersectOperation" | "PartOperation" => {
                Some(self.look(Vec3::ONE, Vec3::ONE))
            }
            _ => Some(self.look(size, scale)),
        }
    }

    /// What a part of `size` draws with this mesh at `scale`. `Scale`
    /// multiplies the part's Size per axis first, and the type's own rule
    /// then works on that scaled size (observed: a CylinderMesh on a
    /// 2 × 3 × 4 part at Scale (0.5, 1.5, 0.75) was 1 stud across, not 1.5).
    ///
    /// A negative `Scale` doesn't mirror the primitive shapes (observed in
    /// Studio 0.742, 2026-10-13: a Wedge at Scale (1, 1, -1) drew the same
    /// wedge as at Scale 1), so they use its size. A mesh file is mirrored
    /// (a mesh at Scale (1, 1, -1) drew flipped and inside-out), so it
    /// keeps the sign.
    pub fn look(self, size: Vec3, scale: Vec3) -> MeshLook {
        let s = size * scale;
        let shape_size = s.abs();
        match self {
            // A Sphere stretches, so it skips the ball's own extent rule.
            Self::Shape(PartShape::Ball) => MeshLook::Shape {
                shape: PartShape::Ball,
                size: shape_size,
                upright: false,
            },
            Self::Shape(shape) => MeshLook::Shape {
                shape,
                size: shape.extent(shape_size),
                upright: false,
            },
            // The X-axis cylinder turned onto Y: its length is S.Y and its
            // diameter min(S.X, S.Z).
            Self::UprightCylinder => MeshLook::Shape {
                shape: PartShape::Cylinder,
                size: PartShape::Cylinder.extent(Vec3::new(
                    shape_size.y,
                    shape_size.x,
                    shape_size.z,
                )),
                upright: true,
            },
            Self::Head => MeshLook::Head {
                scale: head_scale(s),
            },
            Self::Nothing => MeshLook::Nothing,
            Self::File => MeshLook::File { scale },
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

/// How a MeshPart's mesh file is scaled: by Size / `InitialSize` per axis
/// (`InitialSize` is what scripts read as `MeshSize`: the mesh's size when
/// it was imported). Observed in Studio 0.742: a MeshPart made from a mesh
/// had Size and MeshSize equal to the mesh's bounds; setting Size to
/// (0.5, 0.5, 0.25) of it shrank the drawing by exactly that, about the
/// mesh's own origin. A MeshPart saved without `InitialSize` (it reads
/// (0, 0, 0)) draws the mesh at Size times its own size, as if `InitialSize`
/// were 1: a 1.2-stud mesh on a 2 × 4 × 8 part drew 2.4 × 4.8 × 9.6.
pub fn mesh_part_scale(size: Vec3, initial_size: Vec3) -> Vec3 {
    if initial_size.cmpgt(Vec3::ZERO).all() {
        size / initial_size
    } else {
        size
    }
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
    /// A mesh file, its vertices times `scale`, with the mesh's own origin
    /// at the part's position (observed: a mesh whose bounds are centred 11
    /// studs off its origin drew 11 studs off the part, for a FileMesh and a
    /// MeshPart alike).
    File { scale: Vec3 },
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
            // Scale alone, Size ignored.
            assert_eq!(kind.look(SIZE, SCALE), MeshLook::File { scale: SCALE });
            // Without a MeshId: a unit box times Scale, Size ignored.
            assert_eq!(
                look(class, mesh_type, SCALE),
                shape(PartShape::Block, SCALE, false)
            );
        }
        assert_eq!(MeshKind::of("Part", None, ""), None);
    }

    /// A negative Scale doesn't mirror a primitive shape, but does a file.
    #[test]
    fn negative_scale() {
        let flipped = Vec3::new(1.0, 1.0, -1.0);
        for mesh_type in [2, 3, 4] {
            assert_eq!(
                look("SpecialMesh", Some(mesh_type), flipped),
                look("SpecialMesh", Some(mesh_type), Vec3::ONE)
            );
        }
        assert_eq!(
            look("CylinderMesh", None, -Vec3::ONE),
            look("CylinderMesh", None, Vec3::ONE)
        );
        assert_eq!(
            MeshKind::File.look(SIZE, flipped),
            MeshLook::File { scale: flipped }
        );
    }

    #[test]
    fn hosts() {
        let sphere = MeshKind::of("SpecialMesh", Some(3), "").unwrap();
        assert_eq!(sphere.look_on("MeshPart", SIZE, SCALE), None);
        assert_eq!(
            sphere.look_on("UnionOperation", SIZE, SCALE),
            Some(shape(PartShape::Ball, Vec3::ONE, false))
        );
        assert_eq!(
            sphere.look_on("TrussPart", SIZE, SCALE),
            Some(shape(PartShape::Ball, SIZE * SCALE, false))
        );
    }

    #[test]
    fn mesh_parts_scale_by_size_over_initial_size() {
        let head = Vec3::new(1.2, 1.2, 1.2);
        assert_eq!(mesh_part_scale(head, head), Vec3::ONE);
        assert_eq!(
            mesh_part_scale(Vec3::new(0.6, 0.6, 0.3), head),
            Vec3::new(0.5, 0.5, 0.25)
        );
        // No InitialSize: Size itself.
        assert_eq!(
            mesh_part_scale(Vec3::new(2.0, 4.0, 8.0), Vec3::ZERO),
            Vec3::new(2.0, 4.0, 8.0)
        );
    }
}
