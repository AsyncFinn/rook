//! What the renderer draws: a snapshot built by the engine side. The
//! renderer never sees the DOM (architecture §8).

use std::collections::HashMap;
use std::sync::Arc;

use glam::{Mat3, Vec3};

pub use rook_content::mesh::Mesh;
pub use rook_types::PartShape;

/// A mesh file, by the BLAKE3 hash of its bytes: parts that use the same
/// file share one GPU mesh, however they refer to it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct MeshHash(pub [u8; 32]);

/// A mesh the renderer draws: a primitive part shape, a built-in mesh
/// that isn't one (the SpecialMesh Head), or a mesh file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DrawShape {
    Part(PartShape),
    /// Rook's own head mesh, at its own size (about 1.2 studs each way).
    Head,
    /// A mesh file from [`Scene::meshes`], at its own size.
    File(MeshHash),
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
    /// `BasePart.CastShadow`: whether it casts a sun shadow. Every part
    /// receives shadows either way. Only opaque parts cast (Inference).
    pub cast_shadow: bool,
}

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
    /// `Lighting.GlobalShadows`: false draws no shadows at all.
    pub global_shadows: bool,
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
            global_shadows: true,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Scene {
    pub parts: Vec<PartInstance>,
    pub lighting: Lighting,
    /// The mesh files parts may draw ([`DrawShape::File`]). The renderer
    /// uploads each the first time a part draws it.
    pub meshes: HashMap<MeshHash, Arc<Mesh>>,
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

    pub(crate) fn view_projection(&self, aspect: f32) -> glam::Mat4 {
        let view = glam::Mat4::from_mat3(self.rotation.transpose())
            * glam::Mat4::from_translation(-self.position);
        // Reverse Z (near at depth 1, infinitely far at 0) keeps a float
        // depth buffer precise far away; with ordinary Z, surfaces a few
        // hundred studs off z-fight where they meet.
        let proj = glam::camera::rh::proj::directx::perspective_infinite_reverse(
            self.fov_y.to_radians(),
            aspect,
            0.1,
        );
        proj * view
    }
}
