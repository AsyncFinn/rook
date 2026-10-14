//! What a part draws: its shape or mesh, colour, material and images.

use super::*;

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
    /// Through the mesh's own UVs (`MeshKind::decals_follow_uvs`).
    MeshUv,
}

/// A TrussPart without a mesh: its bars, along its long axis from its low
/// end (`rook_types::TrussLook`), in the part's colour, material and
/// transparency (observed in Studio 0.742, 2026-10-13: Color, Neon,
/// Reflectance and Transparency show as on any part, and Wood its wood).
/// Its Decals and Textures and its surfaces don't show (observed).
pub(super) fn truss(
    dom: &Dom,
    id: InstanceId,
    cf: &CFrame,
    size: Vec3,
    mut out: PartInstance,
    pose: &mut Pose,
) -> PartInstance {
    // Files save `Style` as `style`, which is what the DOM holds (scripts
    // see it as `Style`).
    let style = TrussStyle::from_u32(enum_value(dom, id, "style").unwrap_or(0));
    let look = TrussLook::of(size, style);
    out.shape = DrawShape::Truss(look.shape);
    out.size = Vec3::ONE;
    out.position = cf.position + cf.rotation * look.offset;
    out.rotation = cf.rotation * look.turn;
    pose.offset = look.offset;
    pose.turn = look.turn;
    out
}

impl SceneCache {
    /// What a part draws (`None` without a CFrame or Size): itself, its
    /// Decals and Textures, and how its CFrame places them. A mesh that
    /// draws nothing leaves the part fully transparent, so it stays in the
    /// scene for a script to change back.
    pub(super) fn part(&mut self, dom: &Dom, id: InstanceId) -> Option<Drawn> {
        let mut pose = Pose::default();
        let (mut part, decals) = self.look(dom, id, &mut pose)?;
        self.material_maps(dom, id, &mut part);
        let layers = match decals {
            Decals::None => Vec::new(),
            Decals::Faces | Decals::MeshUv => self.images(dom, id, &part, decals),
        };
        Some(Drawn { part, pose, layers })
    }

    /// [`SceneCache::part`]'s look, filling in `pose` as it goes.
    pub(super) fn look(
        &mut self,
        dom: &Dom,
        id: InstanceId,
        pose: &mut Pose,
    ) -> Option<(PartInstance, Decals)> {
        let Value::CFrame(cf) = dom.property_by_name(id, "CFrame")? else {
            return None;
        };
        let size = vector3(dom, id, "Size")?;
        let own = part_shape(dom, id);
        let class = dom.class_name(id);
        let mut out = PartInstance {
            color: color(dom, id, "Color").unwrap_or([0.64, 0.64, 0.65]),
            transparency: number(dom, id, "Transparency").unwrap_or(0.0),
            // Confirmed: creator-docs BasePart.CastShadow, on by default;
            // places saved before it existed cast.
            cast_shadow: boolean(dom, id, "CastShadow").unwrap_or(true),
            material: Material::of_part(enum_value(dom, id, "Material")),
            reflectance: number(dom, id, "Reflectance").unwrap_or(0.0),
            ..PartInstance::plain(
                DrawShape::Part(own),
                cf.position,
                cf.rotation,
                own.extent(size),
            )
        };
        // Unions (drawn as their box for now), trusses and MeshParts have
        // no surface patterns of their own (observed for trusses; Inference
        // for unions).
        let has_surfaces = !matches!(
            class,
            "MeshPart"
                | "TrussPart"
                | "UnionOperation"
                | "NegateOperation"
                | "IntersectOperation"
                | "PartOperation"
        );
        let surfaces = |out: &mut PartInstance| {
            for face in NormalId::ALL {
                out.surfaces[face.index()] = enum_value(dom, id, SURFACES[face.index()])
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
            // Its own mesh file, scaled by Size / InitialSize; with an empty
            // MeshId, its Size box (observed).
            let url = mesh_id(dom, id);
            if url.is_empty() {
                self.settle(id, id, Settled::Empty);
                return Some((out, Decals::Faces));
            }
            let initial = vector3(dom, id, "InitialSize").unwrap_or(Vec3::ZERO);
            let scale = mesh_part_scale(size, initial);
            match self.file(id, id, &url) {
                FileDraw::Mesh(hash) => {
                    out.shape = DrawShape::File(hash);
                    out.size = scale;
                    self.mesh_texture(dom, id, id, &mut out, [1.0; 3]);
                }
                FileDraw::Empty => {}
                FileDraw::Nothing => {
                    out.transparency = 1.0;
                    return Some((out, Decals::None));
                }
            }
            // Decals on a MeshPart are projected onto its faces (observed).
            return Some((out, Decals::Faces));
        }
        let Some((kind, mesh, url)) = part_mesh(dom, id) else {
            return Some(own(out, pose));
        };
        let scale = vector3(dom, mesh, "Scale").unwrap_or(Vec3::ONE);
        let Some(mut look) = kind.look_on(class, size, scale) else {
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
                    .look_on(class, size, scale)
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
            let tint = vector3(dom, mesh, "VertexColor").unwrap_or(Vec3::ONE);
            self.mesh_texture(dom, id, mesh, &mut out, tint.to_array());
        }
        // Moves the mesh in the part's own axes (Confirmed: creator-docs
        // DataModelMesh.Offset).
        let offset = vector3(dom, mesh, "Offset").unwrap_or(Vec3::ZERO);
        out.position += cf.rotation * offset;
        pose.offset = offset;
        let decals = if kind.decals_follow_uvs() {
            Decals::MeshUv
        } else {
            Decals::Faces
        };
        Some((out, decals))
    }

    /// A mesh's texture (`TextureId` of `mesh`, a MeshPart's `TextureID`),
    /// observed in Studio 0.742 (2026-10-13): once loaded it replaces the
    /// part's colour, times `tint` (the mesh's `VertexColor`); while it
    /// loads the mesh is black; if it fails, or there is none, the mesh
    /// keeps the part's colour.
    pub(super) fn mesh_texture(
        &mut self,
        dom: &Dom,
        part: InstanceId,
        mesh: InstanceId,
        out: &mut PartInstance,
        tint: [f32; 3],
    ) {
        let url = texture_id(dom, mesh);
        if url.is_empty() {
            return;
        }
        match self.file_state(part, &url, Want::Image) {
            FileState::Loaded(hash) => {
                out.texture = Some(ImageHash(hash));
                out.color = tint;
            }
            FileState::Loading => out.color = [0.0; 3],
            FileState::Failed => {}
        }
    }

    /// A part's Decals and Textures, in child order, each drawn over `part`
    /// once its image has loaded. While an image loads, or after it fails,
    /// it draws nothing: a Decal whose `Texture` changes doesn't keep its
    /// old image (observed in Studio 0.742, 2026-10-13).
    pub(super) fn images(
        &mut self,
        dom: &Dom,
        id: InstanceId,
        part: &PartInstance,
        decals: Decals,
    ) -> Vec<PartInstance> {
        let mut out = Vec::new();
        for &child in dom.children(id) {
            let class = dom.class_name(child);
            if class != "Decal" && class != "Texture" {
                continue;
            }
            let url = content_url(dom, child, &["TextureContent", "Texture"]);
            if url.is_empty() {
                continue;
            }
            let FileState::Loaded(hash) = self.file_state(id, &url, Want::Image) else {
                continue;
            };
            let face = enum_value(dom, child, "Face")
                .and_then(NormalId::from_u32)
                .unwrap_or(NormalId::Front);
            let placement = match (decals, class) {
                (Decals::MeshUv, _) => ImagePlacement::MeshUv,
                (_, "Texture") => ImagePlacement::Tiled(
                    face,
                    Tiling {
                        studs_per_tile: Vec2::new(
                            number(dom, child, "StudsPerTileU").unwrap_or(2.0),
                            number(dom, child, "StudsPerTileV").unwrap_or(2.0),
                        ),
                        offset: Vec2::new(
                            number(dom, child, "OffsetStudsU").unwrap_or(0.0),
                            number(dom, child, "OffsetStudsV").unwrap_or(0.0),
                        ),
                    },
                ),
                _ => ImagePlacement::Face(face),
            };
            out.push(PartInstance {
                color: color(dom, child, "Color3").unwrap_or([1.0; 3]),
                transparency: number(dom, child, "Transparency").unwrap_or(0.0),
                texture: None,
                surfaces: [None; 6],
                material: Material::SmoothPlastic,
                maps: MaterialMaps::NONE,
                layer: Layer::Image {
                    image: ImageHash(hash),
                    placement,
                    z: number(dom, child, "ZIndex").unwrap_or(1.0) as i32,
                    on_transparent: part.transparency > 0.0 && part.transparency < 1.0,
                },
                ..*part
            });
        }
        out
    }
}
