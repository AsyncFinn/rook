//! The ClickDetector hand. Roblox's is in no data file, so Rook keeps its
//! pixels here: the classic pointing hand, 17 by 22 cells, which Roblox
//! draws 1.82 times bigger with its pixels blended, about 30 by 41
//! pixels at 96 DPI. The cells and scale were fitted to a capture of
//! Roblox's hand (docs/behavior/gui/input.md#the-cursor).

use std::sync::Arc;

use glam::Vec2;
use rook_content::decode::image::{Image, ImageFormat};

/// The hand, a cell a character: `#` white, `o` black, `.` clear. The
/// clear edge all round lets the blend fade to nothing.
const CELLS: [&str; 24] = [
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
pub(crate) const SCALE: f32 = 1.82;

/// Where the cursor points, in cells: the top of the index fingertip, as
/// the center of the mouse's pixel (Inference: the fingertip points).
pub(crate) const POINT: Vec2 = Vec2::new(7.0, 1.0);

/// The hand's cells as an image, a pixel each.
pub(crate) fn image() -> Arc<Image> {
    let (w, h) = (CELLS[0].len() as u32, CELLS.len() as u32);
    let rgba = CELLS
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
            (CELLS.iter().enumerate())
                .flat_map(|(y, row)| row.bytes().enumerate().map(move |(x, b)| (x, y, b)))
                .filter(|&(_, _, b)| b == c)
                .map(|(x, y, _)| (x, y))
                .collect()
        };
        let drawn: Vec<(usize, usize)> = [cells(b'#'), cells(b'o')].concat();
        let span = |f: fn(&(usize, usize)) -> usize| {
            let (lo, hi) = (drawn.iter().map(f).min(), drawn.iter().map(f).max());
            (hi.unwrap() - lo.unwrap() + 1) as f32 * SCALE
        };
        assert_eq!(span(|c| c.0).round(), 31.0);
        assert_eq!(span(|c| c.1).round(), 40.0);
        let (x, y) = (POINT.x as usize, POINT.y as usize);
        assert_eq!(CELLS[y].as_bytes()[x - 1..=x], *b"oo");
    }
}
