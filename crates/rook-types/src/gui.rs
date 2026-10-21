//! What GUI drawing hands the renderer: rectangles in screen pixels, in
//! the order they're drawn, some of them text glyphs, and the world GUI's
//! canvases with where each is in the world. `rook-gui` makes them and
//! `rook-render` draws them, so the shapes live here, below both.

use std::sync::Arc;

use glam::{Vec2, Vec3};

/// A filled rectangle on the screen, drawn over everything before it.
#[derive(Debug, Clone, PartialEq)]
pub struct GuiQuad {
    /// The rectangle's center, in pixels from the window's top left.
    pub center: Vec2,
    /// Its width and height before it turns, in pixels.
    pub size: Vec2,
    /// Clockwise on the screen, in radians, about `center`.
    pub rotation: f32,
    /// sRGB color and opacity (1 is opaque). GUI blends in sRGB, as
    /// Roblox's does.
    pub color: [f32; 4],
    /// Only pixels inside this rectangle (left, top, right, bottom) are
    /// drawn.
    pub clip: [f32; 4],
    /// A glyph whose coverage the color is drawn through, stretched over
    /// the rectangle; `None` fills it.
    pub glyph: Option<Arc<GlyphMask>>,
}

impl GuiQuad {
    /// A clip rectangle that clips nothing.
    pub const NO_CLIP: [f32; 4] = [f32::MIN, f32::MIN, f32::MAX, f32::MAX];
}

/// A SurfaceGui's or BillboardGui's drawing (docs/behavior/gui-world.md).
#[derive(Debug, Clone, PartialEq)]
pub struct WorldCanvas {
    /// Bottom first, in pixels from the canvas's top left. Unclipped GUI
    /// spills past the canvas, and draws there too.
    pub quads: Vec<GuiQuad>,
    /// The canvas's size in pixels (its `AbsoluteSize`): the rectangle
    /// laid on the face, or the billboard's size.
    pub size: Vec2,
    /// `LightInfluence`: 0 its own color, 1 lit as a surface.
    pub light_influence: f32,
    pub brightness: f32,
    /// Drawn over the world, not hidden by what's in front.
    pub always_on_top: bool,
    /// Hidden past this many studs from the camera.
    pub max_distance: f32,
}

/// Where a [`WorldCanvas`] is this frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum WorldPlace {
    /// Laid on a face: the canvas's top left corner, and the world step
    /// for one canvas pixel right and one down.
    Surface {
        origin: Vec3,
        right: Vec3,
        down: Vec3,
    },
    /// Facing the camera.
    Billboard {
        /// Where it is before the camera-relative offset.
        anchor: Vec3,
        /// Studs moved in the camera's frame: x right, y up, z towards
        /// the camera.
        studs: Vec3,
        /// Its size: these studs at its depth, plus these pixels.
        size_studs: Vec2,
        size_pixels: Vec2,
        /// Moved this share of its own size, x right and y up.
        shift: Vec2,
    },
}

/// A glyph rasterized for drawing, one byte per pixel, rows top first:
/// its coverage at one size, or a distance field that draws it sharp at
/// any size.
#[derive(Debug, PartialEq, Eq)]
pub struct GlyphMask {
    /// Names this mask among all others, so a renderer can keep it.
    pub key: u64,
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
    /// The bytes are a signed distance field: 128 on the outline, rising
    /// inside and falling outside, so a renderer draws the glyph's edge
    /// where they cross 128 (Roblox's GUI shader draws text this way).
    pub distance: bool,
}
