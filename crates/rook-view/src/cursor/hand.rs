//! Rook's own ClickDetector hand: Roblox's is in no data file, so Rook
//! draws one of its own, white with a black outline, matching the size and
//! shape of Roblox's in a capture at 96 DPI (30 by 41 pixels, the index
//! finger up, the thumb out to the left). It's drawn from rectangles, so
//! it's Rook's own art, not a copy of anyone's bitmap.

use std::sync::Arc;

use glam::Vec2;
use rook_content::decode::image::{Image, ImageFormat};

/// The image's size in pixels.
const SIZE: (u32, u32) = (32, 42);

/// Where the cursor points, in the image's pixels: the index fingertip
/// (Inference: Roblox's hand points where its fingertip is).
pub(crate) const POINT: Vec2 = Vec2::new(11.0, 1.0);

/// How thick the outline is, in pixels.
const OUTLINE: f32 = 1.5;

/// The hand's parts as left, top, right, bottom and corner radius:
/// fingers, then the palm's steps down to the cuff, then the thumb's
/// steps out to its tip.
const PARTS: [[f32; 5]; 12] = [
    [8.0, 0.6, 15.0, 22.0, 3.0],
    [14.0, 9.6, 20.6, 22.0, 2.5],
    [18.7, 11.8, 26.0, 22.0, 2.5],
    [24.25, 13.3, 31.2, 22.0, 3.0],
    [8.0, 18.0, 31.2, 29.0, 0.0],
    [6.0, 27.9, 29.6, 31.0, 0.0],
    [8.0, 29.0, 29.6, 34.8, 0.0],
    [10.0, 34.5, 28.0, 41.0, 0.0],
    [1.0, 17.3, 6.6, 22.3, 1.0],
    [5.0, 18.9, 8.5, 22.3, 0.0],
    [2.9, 21.5, 8.5, 24.2, 0.0],
    [4.0, 23.8, 8.5, 27.9, 0.0],
];

/// Black lines inside the hand: between the thumb and the index finger,
/// between the fingers down to the knuckles, and the cuff's thicker
/// bottom edge.
const LINES: [[f32; 4]; 5] = [
    [8.0, 18.0, 9.7, 25.8],
    [13.25, 9.0, 15.0, 18.8],
    [18.7, 11.0, 20.6, 18.8],
    [24.25, 12.5, 26.0, 18.8],
    [8.0, 38.9, 30.0, 41.5],
];

/// Samples per pixel along each side, for smooth edges.
const SAMPLES: u32 = 4;

/// The hand as an image, its rows top first.
pub(crate) fn image() -> Arc<Image> {
    let (w, h) = SIZE;
    let mut rgba = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h {
        for x in 0..w {
            let (mut cover, mut white) = (0.0, 0.0);
            for j in 0..SAMPLES {
                for i in 0..SAMPLES {
                    let at = Vec2::new(x as f32, y as f32)
                        + (Vec2::new(i as f32, j as f32) + 0.5) / SAMPLES as f32;
                    if let Some(shade) = shade(at) {
                        cover += 1.0;
                        white += shade;
                    }
                }
            }
            let n = (SAMPLES * SAMPLES) as f32;
            let value = if cover > 0.0 { white / cover } else { 0.0 };
            let byte = |v: f32| (v * 255.0).round() as u8;
            let v = byte(value);
            rgba.extend_from_slice(&[v, v, v, byte(cover / n)]);
        }
    }
    let rgba = image::RgbaImage::from_raw(w, h, rgba).expect("the hand's size");
    Arc::new(Image::from_rgba(rgba, ImageFormat::Png, w.max(h)).expect("the hand isn't empty"))
}

/// The hand at `at`: white (1) inside, black (0) on its outline and lines,
/// `None` outside.
fn shade(at: Vec2) -> Option<f32> {
    if !inside(at) {
        return None;
    }
    // On the outline when any point that far away is outside.
    let edge = (0..16).any(|k| {
        let turn = k as f32 * std::f32::consts::TAU / 16.0;
        !inside(at + OUTLINE * Vec2::from_angle(turn))
    });
    let on_line = LINES
        .iter()
        .any(|&[l, t, r, b]| (l..r).contains(&at.x) && (t..b).contains(&at.y));
    Some(if edge || on_line { 0.0 } else { 1.0 })
}

fn inside(at: Vec2) -> bool {
    PARTS.iter().any(|&[l, t, r, b, radius]| {
        let center = Vec2::new(l + r, t + b) * 0.5;
        let half = Vec2::new(r - l, b - t) * 0.5 - radius;
        let q = (at - center).abs() - half;
        q.max(Vec2::ZERO).length() + q.max_element().min(0.0) < radius
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The hand fills 30 by 41 pixels as Roblox's does at 96 DPI, and
    /// its point is on the outline at the fingertip's top.
    #[test]
    fn the_hand_is_the_size_of_roblox_s() {
        let hand = image();
        let rgba = &hand.mips[0];
        let alpha = |x: u32, y: u32| rgba[((y * hand.width + x) * 4 + 3) as usize];
        let rows: Vec<u32> = (0..hand.height)
            .filter(|&y| (0..hand.width).any(|x| alpha(x, y) > 127))
            .collect();
        let columns: Vec<u32> = (0..hand.width)
            .filter(|&x| (0..hand.height).any(|y| alpha(x, y) > 127))
            .collect();
        assert_eq!(columns.len(), 30);
        assert_eq!(rows.len(), 41);
        let (px, py) = (POINT.x as u32, POINT.y as u32);
        assert!(alpha(px, py) > 200);
        assert_eq!(rows[0], 0);
        assert!(rgba[((py * hand.width + px) * 4) as usize] < 60);
    }
}
