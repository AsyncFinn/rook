//! What GUI drawing hands the renderer: rectangles in screen pixels, in
//! the order they're drawn, some of them text glyphs. `rook-gui` makes
//! them and `rook-render` draws them, so the shape lives here, below both.

use std::sync::Arc;

use glam::Vec2;

/// A filled rectangle on the screen, drawn over everything before it.
#[derive(Debug, Clone, PartialEq)]
pub struct GuiQuad {
    /// The rectangle's centre, in pixels from the window's top left.
    pub center: Vec2,
    /// Its width and height before it turns, in pixels.
    pub size: Vec2,
    /// Clockwise on the screen, in radians, about `center`.
    pub rotation: f32,
    /// sRGB colour and opacity (1 is opaque). GUI blends in sRGB, as
    /// Roblox's does.
    pub color: [f32; 4],
    /// Only pixels inside this rectangle (left, top, right, bottom) are
    /// drawn.
    pub clip: [f32; 4],
    /// A glyph whose coverage the colour is drawn through, stretched over
    /// the rectangle; `None` fills it.
    pub glyph: Option<Arc<GlyphMask>>,
}

impl GuiQuad {
    /// A clip rectangle that clips nothing.
    pub const NO_CLIP: [f32; 4] = [f32::MIN, f32::MIN, f32::MAX, f32::MAX];
}

/// A glyph rasterised at one size: one coverage byte per pixel, rows top
/// first.
#[derive(Debug, PartialEq, Eq)]
pub struct GlyphMask {
    /// Names this mask among all others, so a renderer can keep it.
    pub key: u64,
    pub width: u32,
    pub height: u32,
    pub coverage: Vec<u8>,
}
