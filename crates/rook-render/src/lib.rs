//! wgpu renderer for Rook.
//!
//! The renderer draws a [`Scene`] snapshot: it never touches the DOM
//! (docs/design/architecture.md#engine-systems). It draws primitive parts, the built-in mesh shapes
//! and mesh files, and TrussParts' bars, with their color, transparency, mesh textures, Plastic's
//! surface patterns and Neon, Decals and Textures over them, a sky color,
//! a sun, ambient light, fog and the sun's shadows, and particles (Fire,
//! Smoke and Sparkles). Terrain arrives as mesh files too: the player
//! meshes its chunks with `rook-terrain`.

mod batch;
mod effects;
mod gpu;
mod mesh;
mod renderer;
mod scene;
mod surfaces;

pub use scene::{
    AdornShape, Anchor, Beam, Camera, Curve, Detile, DrawShape, Emitter, EmitterLook, EmitterShape,
    Fidelity, Flipbook, FlipbookMode, FloorWire, Grow, Highlight, ImageHash, ImagePlacement, InOut,
    Layer, Light, LightShape, Lighting, LocalLight, MapAlpha, MaterialMaps, MeshHash, Orientation,
    PartInstance, RibbonLook, SKYBOX_FACES, Scene, Sequence, Spawn, TERRAIN_LAYERS, TerrainLayer,
    TerrainLook, TextureMode, Tonemapper, Trail,
};

// Content hashes are 32 bytes: FxHash keeps lookups by them cheap.
use rustc_hash::FxHashMap as HashMap;

use bytemuck::{Pod, Zeroable};
use glam::Vec3;
use rook_content::decode::image::Image;
use rook_content::decode::mesh::Mesh;
use rook_types::{GuiQuad, Material, NormalId, PartShape, WorldCanvas, WorldPlace};
use wgpu::util::DeviceExt;

use batch::cull::{Aabb, Frustum, MOTION_STUDS};
use gpu::cache::{FRAME_UPLOAD_BYTES, GpuCache, Upload, UploadContext};
use renderer::lighting::shadow::{SHADOW_DISTANCE, SHADOW_SIZE, ShadowView};

use batch::*;
use gpu::pool::{GpuMesh, MeshPool};
use gpu::*;

pub use renderer::Renderer;

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
