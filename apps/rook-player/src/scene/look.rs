//! What a part draws: its shape, mesh ([`meshes`]), truss bars
//! ([`truss`]) or union geometry ([`union`]), colour, material
//! ([`materials`]) and Decals and Textures ([`decals`]).

mod decals;
pub(super) mod materials;
mod meshes;
mod truss;
mod union;

use super::*;
use truss::truss;
use union::is_union;

/// Turns a shape's X axis onto Y, for a CylinderMesh.
pub(super) const UPRIGHT: Mat3 = Mat3::from_cols(Vec3::Y, Vec3::NEG_X, Vec3::Z);

/// Each face's surface property, in `NormalId` order.
pub(super) const SURFACES: [&str; 6] = [
    "RightSurface",
    "TopSurface",
    "BackSurface",
    "LeftSurface",
    "BottomSurface",
    "FrontSurface",
];

/// What a part's mesh last drew, and which mesh instance drew it (the part
/// itself for a MeshPart).
#[derive(Debug, Clone, Copy)]
pub(super) struct Shown {
    pub(super) mesh: InstanceId,
    pub(super) settled: Settled,
}

/// How Decals and Textures go on what a part draws.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Decals {
    /// The part draws nothing, so neither do they.
    None,
    /// Projected onto the faces of the drawn shape's box.
    Faces,
    /// Projected like `Faces`, but Textures are stretched once like Decals
    /// (`MeshKind::stretches_textures`).
    Stretched,
    /// Through the mesh's own UVs (`MeshKind::decals_follow_uvs`).
    MeshUv,
}

impl SceneCache {
    /// What a part draws (`None` without a CFrame or Size): itself, its
    /// Decals and Textures, the SelectionBoxes on it, and how its CFrame
    /// places them. A mesh that
    /// draws nothing leaves the part fully transparent, so it stays in the
    /// scene for a script to change back.
    pub(super) fn part(&mut self, dom: &Dom, id: InstanceId) -> Option<Drawn> {
        let mut pose = Pose::default();
        let (mut part, decals) = self.look(dom, id, &mut pose)?;
        self.material_maps(dom, id, &mut part);
        let mut layers: Vec<(PartInstance, Pose)> = match decals {
            Decals::None => Vec::new(),
            Decals::Faces | Decals::Stretched | Decals::MeshUv => self
                .images(dom, id, &part, decals)
                .into_iter()
                .map(|layer| (layer, pose))
                .collect(),
        };
        layers.extend(adornments::selection_boxes(dom, id));
        Some(Drawn { part, pose, layers })
    }

    /// [`SceneCache::part`]'s look, filling in `pose` as it goes.
    pub(super) fn look(
        &mut self,
        dom: &Dom,
        id: InstanceId,
        pose: &mut Pose,
    ) -> Option<(PartInstance, Decals)> {
        let cf = dom.cframe_property(id, "CFrame")?;
        self.near = cf.position;
        let size = dom.vector3_property(id, "Size")?;
        let own = part_shape(dom, id);
        let class = dom.class_name(id);
        let mut out = PartInstance {
            color: dom
                .color3_property(id, "Color")
                .map(Color3::to_array)
                .unwrap_or([0.64, 0.64, 0.65]),
            transparency: dom.float_property(id, "Transparency").unwrap_or(0.0),
            // Confirmed: creator-docs BasePart.CastShadow, on by default;
            // places saved before it existed cast.
            cast_shadow: dom.bool_property(id, "CastShadow").unwrap_or(true),
            material: Material::of_part(dom.enum_property(id, "Material")),
            reflectance: dom.float_property(id, "Reflectance").unwrap_or(0.0),
            ..PartInstance::plain(
                DrawShape::Part(own),
                cf.position,
                cf.rotation,
                own.extent(size),
            )
        };
        // Unions, trusses and MeshParts have no surface patterns of their
        // own (observed for trusses; Inference for unions).
        let has_surfaces = !matches!(class, "MeshPart" | "TrussPart") && !is_union(dom, id);
        let surfaces = |out: &mut PartInstance| {
            for face in NormalId::ALL {
                out.surfaces[face.index()] = dom
                    .enum_property(id, SURFACES[face.index()])
                    .and_then(|s| SurfaceType::from_u32(s).pattern());
            }
        };
        // What the part draws without a mesh: its own shape with its
        // surfaces, or a truss's bars.
        let own = |mut out: PartInstance, pose: &mut Pose| {
            if class == "TrussPart" {
                return (truss(dom, id, &cf, size, out, pose), Decals::None);
            }
            if has_surfaces {
                surfaces(&mut out);
            }
            (out, Decals::Faces)
        };
        if class == "MeshPart" {
            return Some(self.mesh_part(dom, id, size, out));
        }
        let Some((kind, mesh, url)) = part_mesh(dom, id) else {
            if is_union(dom, id) {
                let decals = self.union(dom, id, size, &mut out);
                return Some((out, decals));
            }
            return Some(own(out, pose));
        };
        let scale = dom.vector3_property(mesh, "Scale").unwrap_or(Vec3::ONE);
        let host = if is_union(dom, id) {
            MeshHost::Union
        } else {
            MeshHost::Part
        };
        let Some(mut look) = kind.look_on(host, size, scale) else {
            return Some(own(out, pose));
        };
        if kind == MeshKind::EmptyFile {
            self.settle(id, mesh, Settled::Empty);
        }
        if let MeshLook::File { scale } = look {
            look = match self.file(id, mesh, &url) {
                FileDraw::Mesh(hash) => {
                    out.shape = DrawShape::File(hash);
                    out.size = scale;
                    MeshLook::File { scale }
                }
                FileDraw::Empty => MeshKind::EmptyFile
                    .look_on(host, size, scale)
                    .unwrap_or(MeshLook::PartShape),
                FileDraw::Nothing => MeshLook::Nothing,
            };
        }
        if kind.shows_surfaces() && has_surfaces {
            surfaces(&mut out);
        }
        match look {
            MeshLook::Shape {
                shape,
                size,
                upright,
            } => {
                out.shape = DrawShape::Part(shape);
                out.size = size;
                if upright {
                    out.rotation = cf.rotation * UPRIGHT;
                    out.upright = true;
                    pose.turn = UPRIGHT;
                }
            }
            MeshLook::Head { scale } => {
                out.shape = DrawShape::Head;
                out.size = scale;
            }
            MeshLook::File { .. } => {}
            MeshLook::Nothing => {
                out.transparency = 1.0;
                return Some((out, Decals::None));
            }
            MeshLook::PartShape => return Some(own(out, pose)),
        }
        if kind.takes_texture() {
            // The empty-MeshId box drew without the part's Neon glow
            // (observed); its material is otherwise unchecked.
            if out.material == Material::Neon {
                out.material = Material::SmoothPlastic;
            }
            let tint = dom
                .vector3_property(mesh, "VertexColor")
                .unwrap_or(Vec3::ONE);
            self.mesh_texture(dom, id, mesh, &mut out, tint.to_array());
        }
        // Moves the mesh in the part's own axes (Confirmed: creator-docs
        // DataModelMesh.Offset).
        let offset = dom.vector3_property(mesh, "Offset").unwrap_or(Vec3::ZERO);
        out.position += cf.rotation * offset;
        pose.offset = offset;
        let decals = if kind.decals_follow_uvs() {
            Decals::MeshUv
        } else if kind.stretches_textures() {
            Decals::Stretched
        } else {
            Decals::Faces
        };
        Some((out, decals))
    }
}
