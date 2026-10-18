//! What the renderer draws: a snapshot built by the engine side. The
//! renderer never sees the DOM (architecture §8).

use std::collections::HashMap;
use std::sync::Arc;

use glam::{Mat3, Vec2, Vec3};

pub use rook_content::decode::image::Image;
pub use rook_content::decode::mesh::Mesh;
pub use rook_types::{
    Material, MaterialSet, NormalId, PartShape, SurfacePattern, Tiling, TrussShape,
};

mod emitters;
pub use emitters::{Curve, Emitter, EmitterLook};

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
        }
    }
}

/// Where each skybox image goes, in [`Lighting::skybox`] order: (the
/// direction it covers, the image's right, the image's up), measured in
/// Studio 0.742 (2026-10-13) with six labelled images. The four sides are
/// upright and unmirrored seen from inside, the Left image at +X and the
/// Right one at -X (the opposite of what the names suggest); Up's image has
/// its top towards +X, Down's towards -X.
pub const SKYBOX_FACES: [(Vec3, Vec3, Vec3); 6] = [
    (Vec3::Y, Vec3::NEG_Z, Vec3::X),
    (Vec3::Z, Vec3::NEG_X, Vec3::Y),
    (Vec3::NEG_Z, Vec3::X, Vec3::Y),
    (Vec3::X, Vec3::Z, Vec3::Y),
    (Vec3::NEG_X, Vec3::NEG_Z, Vec3::Y),
    (Vec3::NEG_Y, Vec3::NEG_Z, Vec3::NEG_X),
];

/// Scene lighting, mapped from the Lighting service.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Lighting {
    /// Unit vector pointing towards the sun.
    pub sun_direction: Vec3,
    /// sRGB colours.
    pub ambient: [f32; 3],
    pub outdoor_ambient: [f32; 3],
    pub brightness: f32,
    pub fog_color: [f32; 3],
    pub fog_start: f32,
    pub fog_end: f32,
    pub sky_top: [f32; 3],
    pub sky_horizon: [f32; 3],
    /// The sky below the horizon (the skybox's Down image).
    pub sky_bottom: [f32; 3],
    /// What reflective parts take on (`Reflectance`): the sky's colour,
    /// the average of the place's skybox when it has one (sRGB).
    pub reflection: [f32; 3],
    /// The skybox's six images, in [`SKYBOX_FACES`] order (Up, Back,
    /// Front, Left, Right, Down), drawn behind everything once all six
    /// are in [`Scene::images`]; `None` draws the plain sky colour.
    pub skybox: Option<[ImageHash; 6]>,
    /// `Lighting.GlobalShadows`: false draws no shadows at all.
    pub global_shadows: bool,
    /// `Lighting.EnvironmentSpecularScale`, 0 to 1: how much parts reflect
    /// the sky, and how far metals trade their diffuse light for it.
    pub environment_specular: f32,
    /// `Lighting.EnvironmentDiffuseScale`, 0 to 1: how much the sky lights
    /// surfaces (on top of Ambient), dimmed as the sky darkens at night.
    pub environment_diffuse: f32,
    /// `Lighting.ExposureCompensation` as a multiplier on linear light
    /// (2 to the power of it), for everything drawn, the sky included.
    pub exposure: f32,
}

impl Default for Lighting {
    fn default() -> Self {
        Self {
            sun_direction: Vec3::new(0.3, 0.8, 0.5).normalize(),
            ambient: [0.27; 3],
            outdoor_ambient: [0.27; 3],
            brightness: 2.0,
            fog_color: [0.75; 3],
            fog_start: 0.0,
            fog_end: 100_000.0,
            sky_top: [0.36, 0.56, 0.86],
            sky_horizon: [0.75, 0.84, 0.95],
            sky_bottom: [0.75, 0.84, 0.95],
            reflection: [0.75, 0.84, 0.95],
            skybox: None,
            global_shadows: true,
            environment_specular: 0.0,
            environment_diffuse: 0.0,
            exposure: 1.0,
        }
    }
}

/// How the terrain's water and grass move (`docs/behavior/terrain.md`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TerrainLook {
    /// `Terrain.WaterWaveSize`: how high the waves are, in studs.
    pub wave_size: f32,
    /// `Terrain.WaterWaveSpeed`.
    pub wave_speed: f32,
    /// `Workspace.GlobalWind`, studs per second: grass bends with it.
    pub wind: Vec3,
    /// How far from the camera grass blades are drawn; they shrink away
    /// over the last quarter.
    pub grass_reach: f32,
    /// How each texture layer blends where layers meet on a seam, by the
    /// layer's index (`rook_terrain::Layer::index`).
    pub layers: [TerrainLayer; TERRAIN_LAYERS],
}

/// The most terrain texture layers: three facings of 23 materials.
pub const TERRAIN_LAYERS: usize = 69;

/// How a terrain texture layer weighs in on a seam: a terrain mesh whose
/// vertices name their triangle's corners' layers (`Mesh::terrain_corners`)
/// draws its share of the corners' blend, Studio's: each corner's
/// barycentric weight times its layer's height there, the highest winning
/// over a soft edge (read from Studio 0.742's terrain shader;
/// `docs/behavior/terrain.md`).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TerrainLayer {
    /// The image holding the layer's height map, in blue (its
    /// roughness-metalness map); none counts as full height.
    pub height: Option<ImageHash>,
    /// The height's multiplier and offset (`height * a + b`, 0 to 1).
    pub remap: [f32; 2],
    /// How soft the edge between layers is.
    pub scale: f32,
    /// Studs per repeat of its textures.
    pub studs: f32,
    /// Textures laid like a box's (Brick, WoodPlanks).
    pub cube: bool,
}

impl Default for TerrainLayer {
    fn default() -> Self {
        Self {
            height: None,
            remap: [0.0, 1.0],
            scale: 1.0,
            studs: 8.0,
            cube: false,
        }
    }
}

impl Default for TerrainLook {
    fn default() -> Self {
        Self {
            wave_size: 0.15,
            wave_speed: 10.0,
            wind: Vec3::ZERO,
            grass_reach: 0.0,
            layers: [TerrainLayer::default(); TERRAIN_LAYERS],
        }
    }
}

/// How a [`LocalLight`] shines.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LightShape {
    /// A PointLight: every way from `position`.
    Point,
    /// A SpotLight: a cone `angle` degrees across (`SpotLight.Angle`, 0 to
    /// 180) around `direction` (unit, the face it shines from).
    Spot { direction: Vec3, angle: f32 },
    /// A SurfaceLight: from the centre of a face of its part (`position`),
    /// over a cone `angle` degrees across around `direction` (unit, the
    /// face's normal).
    Surface { direction: Vec3, angle: f32 },
}

/// A PointLight, SpotLight or SurfaceLight, placed in the world
/// (`docs/behavior/lights.md`). The renderer turns its Roblox properties
/// into light as Studio does.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LocalLight {
    pub position: Vec3,
    pub shape: LightShape,
    /// `Light.Color`, sRGB 0..1.
    pub color: [f32; 3],
    /// `Light.Brightness`.
    pub brightness: f32,
    /// `Range`, in studs.
    pub range: f32,
    /// `Light.Shadows`: parts block its light.
    pub shadows: bool,
    /// The box (min, max) of the part it hangs from, which doesn't block
    /// its light.
    pub part: (Vec3, Vec3),
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Scene {
    pub parts: Vec<PartInstance>,
    /// The local lights in Workspace (disabled ones at brightness 0).
    pub lights: Vec<LocalLight>,
    /// Particle emitters (Fire, Smoke, Sparkles). An emitter keeps its
    /// index from one update to the next, as parts do, so that its
    /// particles stay with it.
    pub emitters: Vec<Emitter>,
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
