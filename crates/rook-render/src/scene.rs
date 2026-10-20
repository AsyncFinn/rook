//! What the renderer draws: a snapshot built by the engine side. The
//! renderer never sees the DOM (architecture §8).

use std::collections::HashMap;
use std::sync::Arc;

use glam::{Mat3, Vec2, Vec3};

use rook_content::decode::image::Image;
use rook_content::decode::mesh::Mesh;
use rook_types::{
    Material, MaterialSet, NormalId, PartShape, SurfacePattern, TilePattern, Tiling, TrussShape,
};

mod emitters;
mod lighting;
mod ribbons;
mod terrain;
pub use emitters::{
    Curve, Emitter, EmitterLook, EmitterShape, Flipbook, FlipbookMode, InOut, Orientation,
    Sequence, Spawn,
};
pub use lighting::{LightShape, Lighting, LocalLight, SKYBOX_FACES, Tonemapper};
pub use ribbons::{Anchor, Beam, RibbonLook, TextureMode, Trail};
pub use terrain::{TERRAIN_LAYERS, TerrainLayer, TerrainLook};

/// A mesh file, by the BLAKE3 hash of its bytes: parts that use the same
/// file share one GPU mesh, however they refer to it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct MeshHash(pub [u8; 32]);

/// An image (a decal, texture or mesh texture), by the BLAKE3 hash of its
/// file: parts using the same image share one GPU texture.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ImageHash(pub [u8; 32]);

/// What a scene entry draws: a part, or an image on a part.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Layer {
    /// The part (or its mesh) itself.
    Base,
    /// A Decal or Texture: `image` drawn over the part's shape, in
    /// `color` (its `Color3`) and `transparency`, whatever the part's own
    /// transparency (observed: decals on invisible parts show, and on a
    /// half-transparent part a decal isn't seen through the part). Images
    /// never cast shadows, and higher `z` (`ZIndex`) draws later.
    Image {
        image: ImageHash,
        placement: ImagePlacement,
        z: i32,
        /// Its part is partly transparent: the image is sorted with the
        /// transparent parts and drawn just after its own.
        on_transparent: bool,
    },
}

/// Where an image goes on a part's shape (`rook_types::NormalId` has the
/// face rules).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ImagePlacement {
    /// Stretched over one face (a Decal).
    Face(NormalId),
    /// Repeated over one face (a Texture).
    Tiled(NormalId, Tiling),
    /// Over the whole mesh through its own UVs, as Decals on file meshes
    /// and the Head are.
    MeshUv,
}

/// How a SurfaceAppearance uses its colour map's alpha (`Enum.AlphaMode`;
/// observed in Studio 0.742 and the creator docs).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MapAlpha {
    /// Ignored: material textures, MaterialVariants, the legacy set's
    /// terrain textures, and `Opaque`.
    Ignore = 0,
    /// The map over the part's Color, by its alpha.
    Overlay = 1,
    /// The alpha is the surface's opacity.
    Transparency = 2,
    /// The alpha says where the tint applies (on terrain, the 2022 set's
    /// tint, on the texel's luma).
    TintMask = 3,
}

/// A part's material textures (images in [`Scene::images`]): tiled over
/// each face of its shape's box from the face's top-left corner, like a
/// Texture (Studio's material textures, a MaterialVariant's), or laid
/// through the mesh's UVs (a SurfaceAppearance's). Each one that's missing
/// leaves the material's plain value: white for colour, a flat normal, and
/// the part's roughness and metalness.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MaterialMaps {
    /// Multiplies the part's colour (sRGB).
    pub color: Option<ImageHash>,
    /// Tangent-space normals: X in red, Y (up the image) in green.
    pub normal: Option<ImageHash>,
    /// Roughness, in green.
    pub roughness: Option<ImageHash>,
    /// Metalness, in red.
    pub metalness: Option<ImageHash>,
    /// Studs per repeat across and down a face.
    pub studs_per_tile: Vec2,
    /// How the repeats are laid (not for terrain, which [`Detile`]s).
    pub pattern: TilePattern,
    /// On terrain, how each point's copy is turned and shifted.
    pub detile: Detile,
    /// On a part, picks where its textures start, up to a repeat in from
    /// each face's corner (Studio rolls a random start per part each
    /// session: the same part exported again after reopening its place
    /// started elsewhere). 0 starts them at the corner, as terrain needs.
    pub seed: u64,
    /// Laid through the mesh's own UVs instead of over its faces.
    pub on_mesh_uv: bool,
    pub alpha: MapAlpha,
    /// Multiplies the colour map (a SurfaceAppearance's `Color`; sRGB).
    pub tint: [f32; 3],
}

impl MaterialMaps {
    pub const NONE: Self = Self {
        color: None,
        normal: None,
        roughness: None,
        metalness: None,
        studs_per_tile: Vec2::ONE,
        pattern: TilePattern::Regular,
        detile: Detile::NONE,
        seed: 0,
        on_mesh_uv: false,
        alpha: MapAlpha::Ignore,
        tint: [1.0; 3],
    };

    pub fn any(&self) -> bool {
        self.color.is_some()
            || self.normal.is_some()
            || self.roughness.is_some()
            || self.metalness.is_some()
    }
}

/// How terrain breaks up a texture layer's repeats (Studio's terrain
/// shader, `docs/behavior/terrain.md`): each mesh vertex has a random
/// number *r* from 0 to 255, and the layer's texture coordinates there are
/// turned by asin((*r* / 128 - 1) × `turn`) and shifted by (*r*,
/// ⌊2.6651442 *r*⌋) × `shift` repeats; across a triangle the three
/// corners' copies blend by their weights.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Detile {
    /// Repeats per step of *r*: 1 / the table's `detiling` (0: none).
    pub shift: f32,
    /// The table's `rotation`: 1 turns up to 90 degrees either way.
    pub turn: f32,
}

impl Detile {
    pub const NONE: Self = Self {
        shift: 0.0,
        turn: 0.0,
    };
}

/// A mesh the renderer draws: a primitive part shape, a built-in mesh
/// that isn't one (the SpecialMesh Head), or a mesh file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DrawShape {
    Part(PartShape),
    /// Rook's own head mesh, at its own size (about 1.2 studs each way).
    Head,
    /// A mesh file from [`Scene::meshes`], at its own size.
    File(MeshHash),
    /// A TrussPart's bars, in studs (`rook_types::TrussLook` places it).
    /// The renderer builds each shape's mesh the first time one is drawn,
    /// and trusses of the same shape share it.
    Truss(TrussShape),
    /// A shape only adornments draw, built the first time one is drawn.
    Adorn(AdornShape),
}

impl DrawShape {
    pub const ALL: [DrawShape; 6] = [
        DrawShape::Part(PartShape::Block),
        DrawShape::Part(PartShape::Wedge),
        DrawShape::Part(PartShape::CornerWedge),
        DrawShape::Part(PartShape::Cylinder),
        DrawShape::Part(PartShape::Ball),
        DrawShape::Head,
    ];
}

/// The shapes adornments draw that parts don't, unit-sized like the part
/// shapes (extents -0.5..0.5). Hole sizes are in 255ths of the outer
/// size, so nearby sizes share a mesh.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AdornShape {
    /// A cone along Z: its base at z = 0.5, its tip at -0.5 (a
    /// ConeHandleAdornment points along its CFrame's LookVector).
    Cone,
    /// A cylinder along Z with a round hole of `hole`/255 its diameter
    /// (a CylinderHandleAdornment's `InnerRadius`).
    Tube { hole: u8 },
    /// A flat ring in the XY plane facing +Z, with a hole of `hole`/255
    /// its diameter (0: a disc).
    Ring { hole: u8 },
}

impl AdornShape {
    /// The hole of `inner` within `outer`, as a shape stores it.
    pub fn hole(inner: f32, outer: f32) -> u8 {
        if outer <= 0.0 {
            return 0;
        }
        (inner / outer * 255.0).round().clamp(0.0, 255.0) as u8
    }
}

/// A piece that keeps one size on screen, as Studio draws Handles: its
/// offset from `anchor` (world studs) and its size scale with its
/// distance from the camera × `per_stud`, so `position` and `size` are
/// what it draws at 1 / `per_stud` studs away.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Grow {
    pub anchor: Vec3,
    pub per_stud: f32,
}

/// How light falls on a part.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Light {
    #[default]
    Lit,
    /// Its own colour without light, through the tone curve, as Studio
    /// draws most adornments.
    Unlit,
    /// Exactly its own colour on screen, past the tone curve, as Studio
    /// draws a SelectionSphere's ring and adornments always on top.
    Exact,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PartInstance {
    pub shape: DrawShape,
    pub position: Vec3,
    /// Columns are the mesh's right, up and back vectors (the part's, unless
    /// the mesh is turned, as a CylinderMesh is).
    pub rotation: Mat3,
    /// Multiplies the mesh's vertices: the drawn size for the unit part
    /// shapes, a scale for the head and mesh files.
    pub size: Vec3,
    /// sRGB colour, 0..1.
    pub color: [f32; 3],
    /// 0 is opaque, 1 invisible.
    pub transparency: f32,
    /// The shape is turned onto the part's Y axis (a CylinderMesh, whose
    /// `rotation` includes the turn). Images and surfaces still go by the
    /// part's own faces: a Front decal on a CylinderMesh barrel stands
    /// upright on its side (observed).
    pub upright: bool,
    /// `BasePart.CastShadow`: whether it casts a sun shadow. Every part
    /// receives shadows either way. Only opaque parts cast (Inference).
    pub cast_shadow: bool,
    /// The mesh's own vertex colours are drawn instead of `color`: a
    /// union with `UsePartColor` off, whose vertices keep the colours of
    /// the parts it was made from.
    pub vertex_colors: bool,
    /// A texture drawn through the mesh's UVs (a mesh's `TextureId`),
    /// multiplied by `color`.
    pub texture: Option<ImageHash>,
    /// What it's made of: how it shades (`Material::shading`), whether
    /// it shows its surfaces (Plastic) and glows (Neon).
    pub material: Material,
    /// Which set of built-in materials its place draws (its shading
    /// values differ between the two).
    pub material_set: MaterialSet,
    /// Its material's textures.
    pub maps: MaterialMaps,
    /// `BasePart.Reflectance`, 0 to 1.
    pub reflectance: f32,
    /// Each face's pattern (by `NormalId` index), drawn when the material
    /// shows surfaces.
    pub surfaces: [Option<SurfacePattern>; 6],
    pub layer: Layer,
    /// Added over what's drawn at the same depth instead of covering it:
    /// a terrain seam's later layers, each its share of the blend
    /// ([`TerrainLayer`]). Never casts shadows.
    pub over: bool,
    pub light: Light,
    /// Turned to face the camera about `position` (its +Z towards it), as
    /// a SelectionSphere's ring is; `rotation` is ignored.
    pub faces_camera: bool,
    /// Drawn over everything, after the transparent parts, as adornments
    /// `AlwaysOnTop` are.
    pub on_top: bool,
    /// Scaled with the camera's distance (adornments only: it takes the
    /// slot material textures use).
    pub grows: Option<Grow>,
}

impl PartInstance {
    /// A plain part: no texture, surfaces or images.
    pub fn plain(shape: DrawShape, position: Vec3, rotation: Mat3, size: Vec3) -> Self {
        Self {
            shape,
            position,
            rotation,
            size,
            color: [1.0; 3],
            transparency: 0.0,
            upright: false,
            cast_shadow: true,
            vertex_colors: false,
            texture: None,
            material: Material::SmoothPlastic,
            material_set: MaterialSet::Legacy,
            maps: MaterialMaps::NONE,
            reflectance: 0.0,
            surfaces: [None; 6],
            layer: Layer::Base,
            over: false,
            light: Light::Lit,
            faces_camera: false,
            on_top: false,
            grows: None,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Scene {
    pub parts: Vec<PartInstance>,
    /// The local lights in Workspace (disabled ones at brightness 0).
    pub lights: Vec<LocalLight>,
    /// Particle emitters (Fire, Smoke, Sparkles, ParticleEmitter). An
    /// emitter keeps its index from one update to the next, as parts do,
    /// so that its particles stay with it.
    pub emitters: Vec<Emitter>,
    pub beams: Vec<Beam>,
    /// Trails, which keep their index as emitters do (for their pieces).
    pub trails: Vec<Trail>,
    pub lighting: Lighting,
    pub terrain: TerrainLook,
    /// The mesh files parts may draw ([`DrawShape::File`]). The renderer
    /// uploads each the first time a part draws it.
    pub meshes: HashMap<MeshHash, Arc<Mesh>>,
    /// The images parts and their layers may draw. The renderer uploads
    /// each the first time something draws it, a few per frame.
    pub images: HashMap<ImageHash, Arc<Image>>,
}

/// A camera: Roblox convention, looking down its -Z axis, Y up.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Camera {
    pub position: Vec3,
    pub rotation: Mat3,
    /// Vertical field of view in degrees (Roblox's default is 70).
    pub fov_y: f32,
}

impl Camera {
    pub fn look_at(position: Vec3, target: Vec3) -> Self {
        let forward = (target - position).normalize_or(Vec3::NEG_Z);
        let right = forward.cross(Vec3::Y).normalize_or(Vec3::X);
        let up = right.cross(forward);
        Self {
            position,
            rotation: Mat3::from_cols(right, up, -forward),
            fov_y: 70.0,
        }
    }

    pub fn forward(&self) -> Vec3 {
        -self.rotation.z_axis
    }

    /// From clip space to directions from the camera (its rotation only),
    /// for the skybox.
    pub(crate) fn sky_from_clip(&self, aspect: f32) -> glam::Mat4 {
        let view = glam::Mat4::from_mat3(self.rotation.transpose());
        (Self::projection(self.fov_y, aspect) * view).inverse()
    }

    fn projection(fov_y: f32, aspect: f32) -> glam::Mat4 {
        // Reverse Z (near at depth 1, infinitely far at 0) keeps a float
        // depth buffer precise far away; with ordinary Z, surfaces a few
        // hundred studs off z-fight where they meet.
        glam::camera::rh::proj::directx::perspective_infinite_reverse(
            fov_y.to_radians(),
            aspect,
            0.1,
        )
    }

    pub(crate) fn view_projection(&self, aspect: f32) -> glam::Mat4 {
        let view = glam::Mat4::from_mat3(self.rotation.transpose())
            * glam::Mat4::from_translation(-self.position);
        Self::projection(self.fov_y, aspect) * view
    }
}
