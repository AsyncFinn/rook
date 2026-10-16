//! wgpu renderer for Rook.
//!
//! The renderer draws a [`Scene`] snapshot: it never touches the DOM
//! (architecture §8). It draws primitive parts, the built-in mesh shapes
//! and mesh files, and TrussParts' bars, with their colour, transparency, mesh textures, Plastic's
//! surface patterns and Neon, Decals and Textures over them, a sky colour,
//! a sun, ambient light, fog and the sun's shadows. Terrain arrives as
//! mesh files too: the player meshes its chunks with `rook-terrain`.

mod batch;
mod cache;
mod cull;
mod gpu;
mod mesh;
mod pool;
mod renderer;
mod scene;
mod shadow;
mod surfaces;

pub use scene::{
    Camera, DrawShape, Image, ImageHash, ImagePlacement, Layer, Lighting, MapAlpha, Material,
    MaterialMaps, MaterialSet, Mesh, MeshHash, NormalId, PartInstance, PartShape, SKYBOX_FACES,
    Scene, SurfacePattern, TerrainLook, Tiling, TrussShape,
};

// Content hashes are 32 bytes: FxHash keeps lookups by them cheap.
use rustc_hash::FxHashMap as HashMap;

use bytemuck::{Pod, Zeroable};
use glam::Vec3;
use rook_types::srgb_to_linear;
use wgpu::util::DeviceExt;

use cache::{FRAME_UPLOAD_BYTES, GpuCache, Upload, UploadContext};
use cull::{Aabb, Frustum, MOTION_STUDS};
use shadow::{SHADOW_DISTANCE, SHADOW_SIZE, ShadowView};

use batch::*;
use gpu::*;
use pool::{GpuMesh, MeshPool};

pub use renderer::{DrawCounts, GpuTimes, PassCounts, Renderer};

pub const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

#[derive(Debug, thiserror::Error)]
pub enum RenderError {
    #[error("no suitable GPU adapter: {0}")]
    Adapter(String),
    #[error("GPU device request failed: {0}")]
    Device(String),
    #[error("reading back the frame failed: {0}")]
    Readback(String),
}

#[cfg(test)]
mod tests;
