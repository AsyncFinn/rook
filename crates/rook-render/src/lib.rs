//! wgpu renderer for Rook.
//!
//! The renderer draws a [`Scene`] snapshot: it never touches the DOM
//! (architecture §8). It draws primitive parts, the built-in mesh shapes
//! and mesh files, and TrussParts' bars, with their colour, transparency, mesh textures, Plastic's
//! surface patterns and Neon, Decals and Textures over them, a sky colour,
//! a sun, ambient light, fog and the sun's shadows. Terrain comes later.

mod batch;
mod cache;
mod gpu;
mod mesh;
mod renderer;
mod scene;
mod shadow;
mod surfaces;

pub use scene::{
    Camera, DrawShape, Image, ImageHash, ImagePlacement, Layer, Lighting, Material, Mesh, MeshHash,
    NormalId, PartInstance, PartShape, SKYBOX_FACES, Scene, SurfacePattern, Tiling, TrussShape,
};

use std::collections::HashMap;

use bytemuck::{Pod, Zeroable};
use glam::Vec3;
use wgpu::util::DeviceExt;

use cache::{FRAME_UPLOAD_BYTES, GpuCache, Upload, UploadContext};
use shadow::{SHADOW_DISTANCE, SHADOW_SIZE, ShadowView};

use batch::*;
use gpu::*;

pub use renderer::Renderer;

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
