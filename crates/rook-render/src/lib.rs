//! wgpu renderer for Rook. It draws a [`Scene`] snapshot and never touches
//! the DOM (docs/design/architecture.md#engine-systems). Terrain arrives as
//! mesh files too: the player meshes its chunks with `rook-terrain`.

mod batch;
mod effects;
mod gpu;
mod mesh;
mod renderer;
mod scene;
mod surfaces;
mod window_surface;

pub use scene::{
    AdornShape, Anchor, Beam, Camera, Curve, Detile, DrawShape, EffectSlots, Emitter, EmitterLook,
    EmitterShape, Fidelity, Flipbook, FlipbookMode, FloorWire, Grow, Highlight, ImageHash,
    ImagePlacement, InOut, Layer, Light, LightShape, Lighting, LocalLight, MapAlpha, MaterialMaps,
    MeshHash, Orientation, PartInstance, RibbonLook, SKYBOX_FACES, Scene, Sequence, Spawn,
    TerrainColor, TerrainLook, TextureMode, Tonemapper, Trail,
};

// Content hashes are 32 bytes: FxHash keeps lookups by them cheap.
use rustc_hash::FxHashMap;

use bytemuck::{Pod, Zeroable};
use glam::Vec3;
use rook_content::decode::image::Image;
use rook_content::decode::mesh::Mesh;
use rook_types::{GuiQuad, Material, NormalId, PartShape, WorldCanvases, WorldPlace};
use wgpu::util::DeviceExt;

use batch::cull::{Aabb, Frustum, MOTION_STUDS};
use gpu::cache::{GpuCache, Upload, UploadBudget, UploadContext};
use renderer::lighting::shadow::{SHADOW_DISTANCE, SHADOW_SIZE, ShadowView};

use batch::*;
use gpu::pool::{GpuMesh, MeshPool};
use gpu::*;

pub use renderer::Renderer;
pub use window_surface::WindowSurface;

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
    #[error("the window's surface: {0}")]
    Surface(String),
    #[error("reading back the frame failed: {0}")]
    Readback(String),
}

#[cfg(test)]
mod tests;
