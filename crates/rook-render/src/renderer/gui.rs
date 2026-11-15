//! The GUI pass: the GUI's rectangles drawn in order into the GUI layer,
//! premultiplied and in sRGB, which the tone pass lays over the frame, so
//! GUI blends in sRGB as Roblox's does (docs/behavior/gui/layout.md#drawing).
//! [`world`] draws the world GUI's rectangles into the world. Text quads
//! draw through their glyph's coverage in the glyph atlas, image quads
//! through their image ([`images`]), group quads through their group's
//! texture, drawn first ([`groups`]); gradients read their ramp
//! ([`ramps`]).

mod atlas;
mod changes;
mod groups;
mod images;
mod quads;
mod ramps;
mod recording;
mod screen;
mod setup;
pub(crate) mod world;

use std::sync::Arc;

use rook_types::{GuiGroup, GuiViewport, Paint, WorldCanvas, WorldCanvases, WorldPlace};
use wesl_core::include_wesl;

use super::*;
use atlas::Atlas;
use groups::Groups;
use images::{GuiImages, ImageKey, Run, runs};
use pipelines::BindLayouts;
use quads::*;
use ramps::Ramps;
use screen::ScreenPieces;
use world::{WorldDraw, WorldGui, WorldRuns};

/// The GUI layer's format: 8-bit sRGB values, blended as stored.
pub(crate) const GUI_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

#[derive(Debug)]
pub(crate) struct GuiPass {
    pipeline: wgpu::RenderPipeline,
    /// The layout targets bind with, and the glyph atlas's and ramps'
    /// views and the sampler they bind.
    layout: wgpu::BindGroupLayout,
    glyphs: wgpu::TextureView,
    ramps_view: wgpu::TextureView,
    sampler: wgpu::Sampler,
    screen: Target,
    /// The untoned world canvases, under the screen's GUI.
    overlay: Target,
    /// The cursor, over everything.
    cursor: Target,
    /// The screen's pieces last given, and what was made of each.
    screen_pieces: ScreenPieces,
    groups: Groups,
    /// The ViewportFrames the screen shows (`Held::viewports`), and how
    /// many times that list changed.
    viewports: Vec<GuiViewport>,
    viewports_version: u64,
    world: WorldGui,
    atlas: Atlas,
    ramps: Ramps,
    images: GuiImages,
}

impl GuiPass {}
