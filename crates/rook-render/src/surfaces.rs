//! The surface patterns (studs, the weld/glue lattice, inlets, universal)
//! as four layers of one texture array, each tile 2 × 2 studs
//! (`rook_types::SURFACE_TILE_STUDS`). Gray 0.5 leaves a part's color as
//! it is; lighter and darker texels shade it, as Studio's do.
//!
//! The player hands over the user's own `textures/studs.dds` when it finds
//! an install ([`from_studs_atlas`]); otherwise Rook draws its own simple
//! patterns ([`rooks_own`]), an Approximation of the same shapes.

use glam::Vec2;
use rook_content::decode::image::{Image, mips};
use rook_types::SurfacePattern;

/// One layer per pattern, square, each with its mip chain.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Patterns {
    pub size: u32,
    /// By `SurfacePattern` discriminant; each layer's mips, largest first.
    pub layers: Vec<Vec<Vec<u8>>>,
}

/// The patterns from the install's `studs.dds`: a 128 × 2048 strip of 16
/// tiles, four variants each of studs, the lattice, inlets and universal
/// (measured); Rook uses each group's first. `None` for any other layout.
pub(crate) fn from_studs_atlas(atlas: &Image) -> Option<Patterns> {
    let size = atlas.width;
    if atlas.height != size * 16 {
        return None;
    }
    let tile_bytes = (size * size * 4) as usize;
    let level0 = &atlas.mips[0];
    let layers = SurfacePattern::ALL
        .iter()
        .map(|&p| {
            let start = p as usize * 4 * tile_bytes;
            mips(level0[start..start + tile_bytes].to_vec(), size, size)
        })
        .collect();
    Some(Patterns { size, layers })
}

const OWN_SIZE: u32 = 64;

/// Rook's own patterns: square studs raised (lit from the top left) or
/// sunk into each stud cell, and grooves along each cell's diagonals for
/// the lattice.
pub(crate) fn rooks_own() -> Patterns {
    let layers = SurfacePattern::ALL
        .iter()
        .map(|&pattern| {
            let mut pixels = Vec::with_capacity((OWN_SIZE * OWN_SIZE * 4) as usize);
            for y in 0..OWN_SIZE {
                for x in 0..OWN_SIZE {
                    let v = shade(pattern, x, y);
                    let g = (v.clamp(0.0, 1.0) * 255.0).round() as u8;
                    pixels.extend_from_slice(&[g, g, g, 255]);
                }
            }
            mips(pixels, OWN_SIZE, OWN_SIZE)
        })
        .collect();
    Patterns {
        size: OWN_SIZE,
        layers,
    }
}

/// Gray level of a pattern at texel (x, y).
fn shade(pattern: SurfacePattern, x: u32, y: u32) -> f32 {
    let cell = OWN_SIZE / 2;
    let (cx, cy) = (x / cell, y / cell);
    // Position in the stud cell, -1..1.
    let p = Vec2::new(
        ((x % cell) as f32 + 0.5) / cell as f32 * 2.0 - 1.0,
        ((y % cell) as f32 + 0.5) / cell as f32 * 2.0 - 1.0,
    );
    let raised = match pattern {
        SurfacePattern::Studs => true,
        SurfacePattern::Inlet => false,
        SurfacePattern::Universal => (cx + cy) % 2 == 0,
        SurfacePattern::Lattice => {
            let groove = (p.x - p.y).abs().min((p.x + p.y).abs());
            return if groove < 0.12 { 0.3 } else { 0.5 };
        }
    };
    // A square of half the cell with a bevel: edges facing the top left
    // catch the light when raised and are in shade when sunk.
    let edge = p.abs().max_element();
    if edge > 0.55 {
        return 0.5;
    }
    let toward_light = -(p.x + p.y) / 2.0;
    if edge > 0.4 {
        let lit = if raised { toward_light } else { -toward_light };
        return 0.5 + lit.signum() * 0.18;
    }
    if raised { 0.54 } else { 0.42 }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn own_patterns_are_mostly_neutral() {
        let p = rooks_own();
        assert_eq!(p.layers.len(), 4);
        for layer in &p.layers {
            let level0 = &layer[0];
            let mean =
                level0.chunks(4).map(|t| f32::from(t[0])).sum::<f32>() / (level0.len() / 4) as f32;
            assert!((mean - 127.5).abs() < 12.0, "{mean}");
            // 64 down to 1.
            assert_eq!(layer.len(), 7);
        }
    }

    /// The install's atlas: tile 4 × group of each pattern.
    #[test]
    fn slices_a_studs_atlas() {
        let mut level0 = Vec::new();
        for tile in 0..16u8 {
            level0.extend(std::iter::repeat_n([tile, tile, tile, 255], 4).flatten());
        }
        let atlas = Image {
            width: 2,
            height: 32,
            format: rook_content::decode::image::ImageFormat::Dds,
            encoding: rook_content::decode::image::Encoding::Rgba8,
            mips: vec![level0],
            translucent: false,
        };
        let p = from_studs_atlas(&atlas).unwrap();
        let firsts: Vec<u8> = p.layers.iter().map(|l| l[0][0]).collect();
        assert_eq!(firsts, [0, 4, 8, 12]);
    }
}
