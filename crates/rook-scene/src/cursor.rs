//! The cursor drawn into the frame, as Roblox draws its own: on top of
//! everything, at the mouse, so captures of the window show it
//! (docs/behavior/gui/input.md#the-cursor).
//!
//! The ClickDetector hand is Rook's own: Roblox's is in no data file, so
//! its pixels are here, the classic pointing hand, 17 by 22 cells, which
//! Roblox draws 1.82 times bigger with its pixels blended, about 30 by 41
//! pixels at 96 DPI. The cells and scale were fitted to a capture of
//! Roblox's hand.

use std::sync::Arc;

use glam::Vec2;
use rook_content::decode::image::{Image, ImageFormat};
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
        if matches!(look, CursorLook::DetectorHand) && self.hand.is_none() {
            let image = hand_image();
            let hash = ImageHash(*blake3::hash(&image.mips[0]).as_bytes());
            let size = Vec2::new(image.width as f32, image.height as f32);
            self.hand = Some((hash, size));
            renderer.take_images([(hash, image)]);
        }
        let drawn = at.and_then(|at| {
            let (hash, offset, size) = self.cursor_image(dm, look)?;
            let corner = Vec2::from(at).floor() + offset;
            Some(cursor_quad(hash, corner, size))
        });
        renderer.set_cursor(drawn.as_ref());
        drawn.is_some()
    }

    /// The image `look` draws, its hash, where its top left is from the
    /// top left of the mouse's pixel, and its size on screen.
    fn cursor_image(&self, dm: &DataModel, look: &CursorLook) -> Option<(ImageHash, Vec2, Vec2)> {
        match look {
            CursorLook::DetectorHand => {
                let (hash, size) = self.hand?;
                let offset = Vec2::splat(0.5) - HAND_POINT * HAND_SCALE;
                Some((hash, offset, size * HAND_SCALE))
            }
            CursorLook::Image(url) => {
                let hash = ImageHash(dm.gui_image(url)?.hash);
                let image = self.images.info(hash)?;
                // Inference: Roblox centers an icon on the cursor.
                let size = Vec2::new(image.width as f32, image.height as f32);
                Some((hash, -(size * 0.5).floor(), size))
            }
            CursorLook::Hidden | CursorLook::System(_) => None,
        }
    }
}

fn cursor_quad(hash: ImageHash, corner: Vec2, size: Vec2) -> GuiQuad {
    GuiQuad {
        center: corner + size * 0.5,
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

/// The hand, a cell a character: `#` white, `o` black, `.` clear. The
/// clear edge all round lets the blend fade to nothing.
const HAND_CELLS: [&str; 24] = [
    "...................",
    "......oo...........",
    ".....o##o..........",
    ".....o##o..........",
    ".....o##o..........",
    ".....o##o..........",
    ".....o##ooo........",
    ".....o##o##ooo.....",
    ".....o##o##o##oo...",
    ".....o##o##o##o#o..",
    "..oo.o##o##o##o##o.",
    ".o##oo########o##o.",
    ".o###o###########o.",
    "..o##o###########o.",
    "...o#o###########o.",
    "...o#############o.",
    "....o############o.",
    "....o###########o..",
    ".....o##########o..",
    ".....o##########o..",
    "......o########o...",
    "......o########o...",
    "......oooooooooo...",
    "...................",
];

/// How many window pixels a cell covers (fitted to the capture).
const HAND_SCALE: f32 = 1.82;

/// Where the cursor points, in cells: the top of the index fingertip, as
/// the center of the mouse's pixel (Inference: the fingertip points).
const HAND_POINT: Vec2 = Vec2::new(7.0, 1.0);

fn hand_image() -> Arc<Image> {
    let (w, h) = (HAND_CELLS[0].len() as u32, HAND_CELLS.len() as u32);
    let rgba = HAND_CELLS
        .iter()
        .flat_map(|row| row.bytes())
        .flat_map(|cell| match cell {
            b'#' => [255; 4],
            b'o' => [0, 0, 0, 255],
            _ => [0; 4],
        })
        .collect();
    let rgba = image::RgbaImage::from_raw(w, h, rgba).expect("every row is as long");
    Arc::new(Image::from_rgba(rgba, ImageFormat::Png, w.max(h)).expect("the hand isn't empty"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Drawn at its scale, the hand's cells span 31 by 40 pixels, as
    /// Roblox's do at 96 DPI, and its point is on the fingertip's
    /// outline.
    #[test]
    fn the_hand_is_the_size_of_roblox_s() {
        let cells = |c: u8| -> Vec<(usize, usize)> {
            (HAND_CELLS.iter().enumerate())
                .flat_map(|(y, row)| row.bytes().enumerate().map(move |(x, b)| (x, y, b)))
                .filter(|&(_, _, b)| b == c)
                .map(|(x, y, _)| (x, y))
                .collect()
        };
        let drawn: Vec<(usize, usize)> = [cells(b'#'), cells(b'o')].concat();
        let span = |f: fn(&(usize, usize)) -> usize| {
            let (lo, hi) = (drawn.iter().map(f).min(), drawn.iter().map(f).max());
            (hi.unwrap() - lo.unwrap() + 1) as f32 * HAND_SCALE
        };
        assert_eq!(span(|c| c.0).round(), 31.0);
        assert_eq!(span(|c| c.1).round(), 40.0);
        let (x, y) = (HAND_POINT.x as usize, HAND_POINT.y as usize);
        assert_eq!(HAND_CELLS[y].as_bytes()[x - 1..=x], *b"oo");
    }
}
