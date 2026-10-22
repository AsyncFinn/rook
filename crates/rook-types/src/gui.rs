//! What GUI drawing hands the renderer: rectangles in screen pixels, in
//! the order they're drawn, filled, text glyphs or images, and the world
//! GUI's canvases with where each is in the world. `rook-gui` makes them and
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
    pub paint: Paint,
}

impl GuiQuad {
    /// A clip rectangle that clips nothing.
    pub const NO_CLIP: [f32; 4] = [f32::MIN, f32::MIN, f32::MAX, f32::MAX];
}

/// What a [`GuiQuad`] is filled with, in its color.
#[derive(Debug, Clone, PartialEq)]
pub enum Paint {
    /// The color alone.
    Solid,
    /// The color drawn through a glyph's coverage, stretched over the
    /// rectangle.
    Glyph(Arc<GlyphMask>),
    /// Part of an image, tinted by the color.
    Image(ImagePaint),
}

/// Part of an image stretched over a [`GuiQuad`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ImagePaint {
    /// The image file, by the BLAKE3 hash of its bytes (how renderers keep
    /// images).
    pub hash: [u8; 32],
    /// The part shown: left, top, right and bottom, 0 to 1 across the
    /// image. Right left of left (or bottom above top) mirrors it.
    pub uv: [f32; 4],
    /// Each pixel shows its nearest texel instead of a blend
    /// (`ResampleMode` Pixelated).
    pub pixelated: bool,
    /// The image repeats past 0 and 1 (`ScaleType` Tile); otherwise its
    /// edge pixels carry on.
    pub tiled: bool,
}

/// A SurfaceGui's or BillboardGui's drawing (docs/behavior/gui/world.md).
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
