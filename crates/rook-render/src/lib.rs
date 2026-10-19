//! wgpu renderer for Rook.
//!
//! The renderer draws a [`Scene`] snapshot: it never touches the DOM
//! (architecture §8). It draws primitive parts, the built-in mesh shapes
//! and mesh files, and TrussParts' bars, with their colour, transparency, mesh textures, Plastic's
//! surface patterns and Neon, Decals and Textures over them, a sky colour,
//! a sun, ambient light, fog and the sun's shadows, and particles (Fire,
//! Smoke and Sparkles). Terrain arrives as mesh files too: the player
//! meshes its chunks with `rook-terrain`.

mod batch;
mod gpu;
mod mesh;
mod particles;
mod renderer;
mod scene;
mod surfaces;

pub use scene::{
    Camera, Curve, Detile, DrawShape, Emitter, EmitterLook, Image, ImageHash, ImagePlacement,
    Layer, LightShape, Lighting, LocalLight, MapAlpha, Material, MaterialMaps, MaterialSet, Mesh,
    MeshHash, NormalId, PartInstance, PartShape, SKYBOX_FACES, Scene, SurfacePattern,
    TERRAIN_LAYERS, TerrainLayer, TerrainLook, TilePattern, Tiling, Tonemapper, TrussShape,
};

// Content hashes are 32 bytes: FxHash keeps lookups by them cheap.
use rustc_hash::FxHashMap as HashMap;

use bytemuck::{Pod, Zeroable};
use glam::Vec3;
use wgpu::util::DeviceExt;

use batch::cull::{Aabb, Frustum, MOTION_STUDS};
use gpu::cache::{FRAME_UPLOAD_BYTES, GpuCache, Upload, UploadContext};
use renderer::shadow::{SHADOW_DISTANCE, SHADOW_SIZE, ShadowView};

use batch::*;
use gpu::pool::{GpuMesh, MeshPool};
use gpu::*;

pub use renderer::{DrawCounts, GpuTimes, PassCounts, Renderer};

/// The shadow map's depth, and the main pass's where the device has no
/// stencil to go with it (`main_depth_format`).
pub const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

/// The main pass's depth: with a stencil where the device has one, so
/// that See-through Glass filters each pixel once (`Pipelines::new`).
pub(crate) fn main_depth_format(device: &wgpu::Device) -> wgpu::TextureFormat {
    if device
        .features()
        .contains(wgpu::Features::DEPTH32FLOAT_STENCIL8)
    {
        wgpu::TextureFormat::Depth32FloatStencil8
    } else {
        DEPTH_FORMAT
    }
}

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
