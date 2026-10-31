//! The cursor drawn into the frame, as Roblox draws its own: on top of
//! everything, at the mouse, so captures of the window show it
//! (docs/behavior/gui/input.md#the-cursor). [`hand`] is the ClickDetector
//! hand Rook draws for itself.

mod hand;

use std::sync::Arc;

use glam::Vec2;
use rook_content::decode::image::Image;
use rook_engine::{CursorLook, DataModel};
use rook_render::Renderer;
use rook_types::{GuiQuad, ImagePaint, Paint};

use crate::*;

impl SceneCache {
    /// Draw `look` into the frame with its point at `at` (window pixels)
    /// from now on, or no cursor (`None`). Whether it's drawn: a system
    /// cursor isn't, nor an image still loading, and the window shows
    /// its own cursor for those instead.
    pub fn show_cursor(
        &mut self,
        dm: &DataModel,
        renderer: &mut Renderer,
        look: &CursorLook,
        at: Option<(f32, f32)>,
    ) -> bool {
        let drawn = at.and_then(|at| {
            let (image, hash, point) = self.cursor_image(dm, look)?;
            Some((cursor_quad(&image, hash, Vec2::from(at) - point), image))
        });
        let shown = drawn.is_some();
        renderer.set_cursor(drawn.as_ref().map(|(quad, image)| (quad, image)));
        shown
    }

    /// The image `look` draws, its hash, and where in it the cursor
    /// points.
    fn cursor_image(
        &mut self,
        dm: &DataModel,
        look: &CursorLook,
    ) -> Option<(Arc<Image>, ImageHash, Vec2)> {
        match look {
            CursorLook::DetectorHand => {
                let (hand, hash) = self.hand.get_or_insert_with(|| {
                    let hand = hand::image();
                    let hash = ImageHash(*blake3::hash(&hand.mips[0]).as_bytes());
                    (hand, hash)
                });
                Some((hand.clone(), *hash, hand::POINT))
            }
            CursorLook::Image(url) => {
                let hash = ImageHash(dm.gui_image(url)?.hash);
                let image = self.scene.images.get(&hash)?;
                // Inference: Roblox centers an icon on the cursor.
                let point = Vec2::new(image.width as f32, image.height as f32) * 0.5;
                Some((image.clone(), hash, point))
            }
            CursorLook::Hidden | CursorLook::System(_) => None,
        }
    }
}

/// `image` (hashed `hash`) drawn at its own size with its top left at
/// `corner`, on whole pixels so it stays sharp.
fn cursor_quad(image: &Image, hash: ImageHash, corner: Vec2) -> GuiQuad {
    let size = Vec2::new(image.width as f32, image.height as f32);
    GuiQuad {
        center: corner.round() + size * 0.5,
        size,
        rotation: 0.0,
        color: [1.0; 4],
        clip: GuiQuad::NO_CLIP,
        paint: Paint::Image(ImagePaint {
            hash: hash.0,
            uv: [0.0, 0.0, 1.0, 1.0],
            pixelated: false,
            tiled: false,
        }),
        shape: None,
        gradient: None,
    }
}
