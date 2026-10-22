//! What GUI drawing hands the renderer: rectangles in screen pixels, in
//! the order they're drawn, filled, text glyphs, images or groups drawn
//! apart, cut to rounded or stroked shapes and colored by gradients, and
//! the world GUI's canvases with where each is in the world. `rook-gui` makes them and
//! `rook-render` draws them, so the shapes live here, below both.

use std::sync::Arc;

use glam::{Mat3, Vec2, Vec3};

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
    /// Only pixels inside this shape are drawn (UICorner, UIStroke).
    pub shape: Option<GuiShape>,
    /// Multiplies the color, opacity too (UIGradient).
    pub gradient: Option<GuiGradient>,
}

impl GuiQuad {
    /// A clip rectangle that clips nothing.
    pub const NO_CLIP: [f32; 4] = [f32::MIN, f32::MIN, f32::MAX, f32::MAX];
}

/// A rectangle with rounded or cut corners a [`GuiQuad`] is cut to,
/// turned with it, or the band inside its edge (a stroke).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GuiShape {
    /// Its center, in the quad's pixels.
    pub center: Vec2,
    /// Its width and height before it turns.
    pub size: Vec2,
    /// How far in from each side each corner starts: top left, top right,
    /// bottom right, bottom left.
    pub radii: [f32; 4],
    /// Only this band inside the edge is drawn (0: all of it); the inside
    /// edge's corners are `radii` less this.
    pub ring: f32,
    /// The corners are cut straight across instead of rounded.
    pub bevel: bool,
}

/// A gradient over a [`GuiQuad`]: the color at `t` is the ramp's texel
/// at `t` (0 to 1, held at the ends), where `t` is 0.5 at `origin` and
/// grows by `step` per pixel.
#[derive(Debug, Clone, PartialEq)]
pub struct GuiGradient {
    pub ramp: Arc<GuiRamp>,
    pub origin: Vec2,
    pub step: Vec2,
}

/// A gradient's colors and opacities from 0 to 1, sampled evenly.
#[derive(Debug, PartialEq, Eq)]
pub struct GuiRamp {
    /// Names this ramp among all others, so a renderer can keep it.
    pub key: u64,
    /// sRGB and opacity, not premultiplied.
    pub texels: [[u8; 4]; GuiRamp::TEXELS],
}

impl GuiRamp {
    pub const TEXELS: usize = 256;
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
    /// A group of quads drawn apart into a texture of its own, then laid
    /// over the rectangle, tinted by the color (a CanvasGroup).
    Group(Arc<GuiGroup>),
}

/// Quads a [`Paint::Group`] draws into its texture.
#[derive(Debug, PartialEq)]
pub struct GuiGroup {
    /// Names this group's drawing among all others, so a renderer can keep
    /// its texture while it's unchanged.
    pub key: u64,
    /// The texture's size in pixels.
    pub size: [u32; 2],
    /// Bottom first, in pixels from the texture's top left.
    pub quads: Vec<GuiQuad>,
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

/// Where a world canvas is as seen from a camera: its top left corner and
/// the world step per canvas pixel right and down.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WorldFrame {
    pub corner: Vec3,
    pub right: Vec3,
    pub down: Vec3,
    /// Seen only from in front (a SurfaceGui), not from behind.
    pub one_sided: bool,
}

/// The camera a world canvas is seen from.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Viewer {
    pub position: Vec3,
    pub rotation: Mat3,
    /// Vertical field of view in degrees.
    pub fov_y: f32,
    /// The viewport's height in pixels.
    pub height: f32,
}

impl WorldPlace {
    /// Where a canvas of `size` pixels placed here is, seen from `eye`;
    /// `None` when it can't be seen (a billboard behind the camera, or no
    /// size).
    pub fn frame(&self, size: Vec2, eye: &Viewer) -> Option<WorldFrame> {
        match *self {
            Self::Surface {
                origin,
                right,
                down,
            } => Some(WorldFrame {
                corner: origin,
                right,
                down,
                one_sided: true,
            }),
            Self::Billboard {
                anchor,
                studs,
                size_studs,
                size_pixels,
                shift,
            } => {
                let [right, up, back] = [
                    eye.rotation.x_axis,
                    eye.rotation.y_axis,
                    eye.rotation.z_axis,
                ];
                let center = anchor + right * studs.x + up * studs.y + back * studs.z;
                let depth = (center - eye.position).dot(-back);
                if depth <= 0.05 || size.x <= 0.0 || size.y <= 0.0 {
                    return None;
                }
                let pixels_per_stud =
                    eye.height / (2.0 * (eye.fov_y.to_radians() * 0.5).tan() * depth);
                let world = size_studs + size_pixels / pixels_per_stud;
                let center = center + right * shift.x * world.x + up * shift.y * world.y;
                let corner = center - right * world.x * 0.5 + up * world.y * 0.5;
                Some(WorldFrame {
                    corner,
                    right: right * world.x / size.x,
                    down: -up * world.y / size.y,
                    one_sided: false,
                })
            }
        }
    }
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
