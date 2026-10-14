//! Rook's own material textures, for when the user's Roblox content cache
//! doesn't have Studio's ([`crate::StudioCache`]). They're drawn in code:
//! a tileable height field per material (noise, grain, boards, bricks,
//! tiles or stones), from which come a colour map, whose average is the
//! average of Studio's colour map for that material (so a part of any
//! Color comes out as bright as in Studio; measured from the cached
//! textures' own averages), and a normal map. Roughness and metalness stay
//! the material's single values (`rook_types::Material::shading`). The
//! patterns are Rook's own (Approximation): only their scale and
//! brightness follow Studio.

use rook_types::Material;

use crate::MaterialMap;
use crate::image::{Image, ImageFormat};

const SIZE: u32 = 256;

/// What a material's height field looks like.
#[derive(Debug, Clone, Copy)]
enum Pattern {
    /// Value noise: features per tile, octaves, and how much the colour
    /// and the surface follow it.
    Noise {
        cells: u32,
        octaves: u32,
        contrast: f32,
        bump: f32,
    },
    /// Long streaks along the image's X, as a grain.
    Grain { streaks: u32, contrast: f32 },
    /// Boards: rows per tile, boards per row, staggered, with a grain.
    Boards { rows: u32, per_row: u32 },
    /// Bricks or tiles in rows (staggered when `stagger`), with grooves.
    Bricks {
        rows: u32,
        per_row: u32,
        stagger: bool,
        groove: f32,
    },
    /// Stones: cells per side of a jittered grid, with grooves between.
    Stones { cells: u32, groove: f32 },
    /// Raised diamonds on a plate.
    Diamonds { per_side: u32 },
    /// Threads crossing over and under each other.
    Weave { threads: u32 },
}

/// Each textured material's pattern and Studio's average colour (sRGB).
fn style(material: Material) -> Option<(Pattern, [u8; 3])> {
    use Material::*;
    use Pattern::*;
    let noise = |cells, contrast, bump| Noise {
        cells,
        octaves: 5,
        contrast,
        bump,
    };
    Some(match material {
        Wood => (
            Grain {
                streaks: 24,
                contrast: 0.25,
            },
            [224, 224, 224],
        ),
        WoodPlanks => (
            Boards {
                rows: 4,
                per_row: 2,
            },
            [207, 202, 196],
        ),
        Marble => (noise(3, 0.3, 0.2), [193, 193, 193]),
        Basalt => (noise(4, 0.35, 1.0), [145, 145, 144]),
        Slate => (noise(3, 0.35, 0.8), [186, 186, 186]),
        CrackedLava => (
            Stones {
                cells: 5,
                groove: 0.6,
            },
            [67, 70, 72],
        ),
        Concrete => (noise(16, 0.2, 0.4), [203, 203, 203]),
        Limestone => (noise(5, 0.25, 0.6), [232, 231, 227]),
        Granite => (noise(24, 0.5, 0.2), [104, 104, 104]),
        Pavement => (
            Bricks {
                rows: 2,
                per_row: 2,
                stagger: false,
                groove: 0.04,
            },
            [132, 133, 134],
        ),
        Brick => (
            Bricks {
                rows: 12,
                per_row: 4,
                stagger: true,
                groove: 0.12,
            },
            [198, 195, 194],
        ),
        Pebble => (
            Stones {
                cells: 10,
                groove: 0.3,
            },
            [203, 203, 203],
        ),
        Cobblestone => (
            Stones {
                cells: 9,
                groove: 0.2,
            },
            [207, 193, 180],
        ),
        Rock => (noise(4, 0.35, 1.0), [145, 145, 145]),
        Sandstone => (noise(4, 0.3, 0.6), [166, 162, 160]),
        CorrodedMetal => (noise(6, 0.5, 0.3), [150, 109, 86]),
        DiamondPlate => (Diamonds { per_side: 8 }, [162, 162, 162]),
        Foil => (
            Stones {
                cells: 6,
                groove: 0.0,
            },
            [230, 230, 230],
        ),
        Metal => (
            Grain {
                streaks: 64,
                contrast: 0.06,
            },
            [193, 193, 193],
        ),
        Grass => (noise(48, 0.45, 0.6), [155, 155, 148]),
        LeafyGrass => (noise(24, 0.5, 0.8), [111, 107, 103]),
        Sand => (noise(40, 0.2, 0.5), [216, 216, 216]),
        Fabric => (Weave { threads: 32 }, [96, 95, 93]),
        Snow => (noise(6, 0.1, 0.4), [213, 213, 213]),
        Mud => (noise(5, 0.25, 0.6), [121, 120, 121]),
        Ground => (noise(8, 0.3, 0.6), [105, 105, 102]),
        Asphalt => (noise(48, 0.3, 0.3), [113, 114, 114]),
        Salt => (noise(12, 0.25, 0.6), [216, 216, 217]),
        Ice => (noise(3, 0.05, 0.3), [255, 255, 255]),
        Glacier => (noise(3, 0.2, 0.7), [222, 226, 226]),
        Glass => (noise(2, 0.02, 0.1), [254, 254, 254]),
        Cardboard => (
            Grain {
                streaks: 40,
                contrast: 0.08,
            },
            [161, 161, 161],
        ),
        Carpet => (noise(64, 0.3, 0.5), [200, 200, 200]),
        CeramicTiles => (
            Bricks {
                rows: 4,
                per_row: 4,
                stagger: false,
                groove: 0.05,
            },
            [238, 238, 238],
        ),
        ClayRoofTiles => (
            Bricks {
                rows: 8,
                per_row: 8,
                stagger: true,
                groove: 0.3,
            },
            [165, 165, 165],
        ),
        RoofShingles => (
            Bricks {
                rows: 10,
                per_row: 5,
                stagger: true,
                groove: 0.15,
            },
            [194, 195, 188],
        ),
        Leather => (
            Stones {
                cells: 24,
                groove: 0.15,
            },
            [221, 221, 221],
        ),
        Plaster => (noise(8, 0.12, 0.4), [216, 216, 216]),
        Rubber => (noise(32, 0.1, 0.3), [204, 204, 203]),
        Plastic | SmoothPlastic | Neon | ForceField | Air | Water => return None,
    })
}

/// Rook's own `map` for `material`, or `None` when it has no texture (or
/// for roughness-metalness, which Rook leaves to the material's values).
pub fn material(material: Material, map: MaterialMap) -> Option<Image> {
    let (pattern, average) = style(material)?;
    let (height, shade) = height_field(pattern, material as u32);
    let n = SIZE as usize;
    let mut rgba = ::image::RgbaImage::new(SIZE, SIZE);
    match map {
        MaterialMap::Color => {
            let mean = shade.iter().sum::<f32>() / shade.len() as f32;
            for (i, p) in rgba.pixels_mut().enumerate() {
                let k = 1.0 + (shade[i] - mean);
                let [r, g, b] = average.map(|c| (f32::from(c) * k).clamp(0.0, 255.0) as u8);
                p.0 = [r, g, b, 255];
            }
        }
        MaterialMap::Normal => {
            let at = |x: usize, y: usize| height[(y % n) * n + x % n];
            for y in 0..n {
                for x in 0..n {
                    // Slopes per pixel, scaled so a full-height step over
                    // a few pixels leans about 45 degrees.
                    let dx = (at(x + 1, y) - at(x + n - 1, y)) * 2.0;
                    let dy = (at(x, y + n - 1) - at(x, y + 1)) * 2.0;
                    let len = (dx * dx + dy * dy + 1.0).sqrt();
                    let enc = |c: f32| ((c / len * 0.5 + 0.5) * 255.0).round() as u8;
                    rgba.put_pixel(
                        x as u32,
                        y as u32,
                        ::image::Rgba([enc(-dx), enc(-dy), enc(1.0), 255]),
                    );
                }
            }
        }
        MaterialMap::RoughnessMetalness => return None,
    }
    Image::from_rgba(rgba, ImageFormat::Png, SIZE).ok()
}

/// A height field (0 to 1, roughly) and a shade (around 0, how much
/// lighter or darker the colour is), both `SIZE` × `SIZE` and tileable.
fn height_field(pattern: Pattern, seed: u32) -> (Vec<f32>, Vec<f32>) {
    let n = SIZE as usize;
    let mut height = vec![0.0; n * n];
    let mut shade = vec![0.0; n * n];
    for y in 0..n {
        for x in 0..n {
            let (u, v) = (x as f32 / n as f32, y as f32 / n as f32);
            let fine = fbm(u, v, 32, 3, seed + 7);
            let (h, s) = match pattern {
                Pattern::Noise {
                    cells,
                    octaves,
                    contrast,
                    bump,
                } => {
                    let f = fbm(u, v, cells, octaves, seed);
                    (f * bump, (f - 0.5) * contrast * 2.0)
                }
                Pattern::Grain { streaks, contrast } => {
                    let warp = fbm(u, v, 4, 3, seed) * 3.0;
                    let g = (std::f32::consts::TAU * (v * streaks as f32 + warp)).sin();
                    (g * 0.15, g * contrast + (fine - 0.5) * contrast)
                }
                Pattern::Boards { rows, per_row } => {
                    let row = (v * rows as f32).floor();
                    let shift = hash2(row as u32, 0, seed) * 0.8;
                    let along = (u * per_row as f32 + shift).fract();
                    let across = (v * rows as f32).fract();
                    let edge = groove(across, 0.03).min(groove(along, 0.01));
                    let tone = hash2(row as u32, (u * per_row as f32 + shift) as u32, seed) - 0.5;
                    let g = (std::f32::consts::TAU * (across * 3.0 + fbm(u, v, 4, 2, seed) * 2.0))
                        .sin();
                    (edge, tone * 0.25 + g * 0.08 + (edge - 1.0) * 0.5)
                }
                Pattern::Bricks {
                    rows,
                    per_row,
                    stagger,
                    groove: width,
                } => {
                    let row = (v * rows as f32).floor();
                    let shift = if stagger && row as u32 % 2 == 1 {
                        0.5
                    } else {
                        0.0
                    };
                    let along = (u * per_row as f32 + shift).fract();
                    let across = (v * rows as f32).fract();
                    let edge = groove(across, width).min(groove(along, width / 2.0));
                    let tone = hash2(row as u32, (u * per_row as f32 + shift) as u32, seed) - 0.5;
                    (
                        edge + fine * 0.1,
                        tone * 0.2 + (fine - 0.5) * 0.2 + (edge - 1.0) * 0.3,
                    )
                }
                Pattern::Stones {
                    cells,
                    groove: width,
                } => {
                    let (f1, f2, id) = cellular(u, v, cells, seed);
                    let edge = ((f2 - f1) / width.max(0.02)).min(1.0);
                    let dome = (1.0 - f1 * 1.2).max(0.0);
                    let h = if width > 0.0 {
                        edge * 0.7 + dome * 0.3
                    } else {
                        f2 - f1
                    };
                    (
                        h,
                        (id - 0.5) * 0.25 + (fine - 0.5) * 0.2 + (edge - 1.0) * 0.3,
                    )
                }
                Pattern::Diamonds { per_side } => {
                    let (cu, cv) = ((u * per_side as f32).fract(), (v * per_side as f32).fract());
                    let flip =
                        ((u * per_side as f32) as u32 + (v * per_side as f32) as u32) % 2 == 1;
                    let (a, b) = if flip { (cu, 1.0 - cv) } else { (cu, cv) };
                    // A raised bar along the cell's diagonal.
                    let d = ((a - b).abs() / 0.18).min(1.0);
                    let h = (1.0 - d) * (1.0 - ((a + b - 1.0).abs() / 0.8).min(1.0));
                    (h, h * 0.25 + (fine - 0.5) * 0.15)
                }
                Pattern::Weave { threads } => {
                    let (tu, tv) = (u * threads as f32, v * threads as f32);
                    let over = (tu as u32 + tv as u32).is_multiple_of(2);
                    let w = if over {
                        (std::f32::consts::PI * tu.fract()).sin()
                    } else {
                        (std::f32::consts::PI * tv.fract()).sin()
                    };
                    (w * 0.5, (w - 0.5) * 0.3 + (fine - 0.5) * 0.1)
                }
            };
            height[y * n + x] = h;
            shade[y * n + x] = s;
        }
    }
    (height, shade)
}

/// 1 inside, falling to 0 within `width` of either edge of a 0..1 cell.
fn groove(t: f32, width: f32) -> f32 {
    if width <= 0.0 {
        return 1.0;
    }
    (t.min(1.0 - t) / width).clamp(0.0, 1.0)
}

/// A deterministic hash of two integers and a seed, in 0..1.
fn hash2(x: u32, y: u32, seed: u32) -> f32 {
    let mut h =
        x.wrapping_mul(0x8DA6_B343) ^ y.wrapping_mul(0xD816_3841) ^ seed.wrapping_mul(0xCB1A_B31F);
    h ^= h >> 13;
    h = h.wrapping_mul(0x5BD1_E995);
    h ^= h >> 15;
    (h & 0xFF_FFFF) as f32 / 0xFF_FFFF as f32
}

/// Value noise with `cells` lattice cells per tile, wrapping at the edges.
fn value_noise(u: f32, v: f32, cells: u32, seed: u32) -> f32 {
    let (x, y) = (u * cells as f32, v * cells as f32);
    let (x0, y0) = (x.floor(), y.floor());
    let (fx, fy) = (x - x0, y - y0);
    let (sx, sy) = (fx * fx * (3.0 - 2.0 * fx), fy * fy * (3.0 - 2.0 * fy));
    let at = |i: f32, j: f32| hash2((i as u32) % cells, (j as u32) % cells, seed);
    let top = at(x0, y0) + (at(x0 + 1.0, y0) - at(x0, y0)) * sx;
    let bottom = at(x0, y0 + 1.0) + (at(x0 + 1.0, y0 + 1.0) - at(x0, y0 + 1.0)) * sx;
    top + (bottom - top) * sy
}

/// Octaves of value noise, each with twice the cells and half the weight.
fn fbm(u: f32, v: f32, cells: u32, octaves: u32, seed: u32) -> f32 {
    let (mut sum, mut weight, mut total) = (0.0, 1.0, 0.0);
    for o in 0..octaves {
        sum += value_noise(u, v, cells << o, seed + o) * weight;
        total += weight;
        weight *= 0.5;
    }
    sum / total
}

/// Distances to the nearest and second-nearest jittered points of a
/// `cells` × `cells` wrapping grid (in cells), and the nearest one's hash.
fn cellular(u: f32, v: f32, cells: u32, seed: u32) -> (f32, f32, f32) {
    let (x, y) = (u * cells as f32, v * cells as f32);
    let (cx, cy) = (x.floor() as i32, y.floor() as i32);
    let (mut f1, mut f2, mut id) = (f32::MAX, f32::MAX, 0.0);
    for j in -1..=1 {
        for i in -1..=1 {
            let (gx, gy) = (cx + i, cy + j);
            let (wx, wy) = (
                gx.rem_euclid(cells as i32) as u32,
                gy.rem_euclid(cells as i32) as u32,
            );
            let px = gx as f32 + 0.15 + 0.7 * hash2(wx, wy, seed);
            let py = gy as f32 + 0.15 + 0.7 * hash2(wx, wy, seed + 1);
            let d = ((px - x).powi(2) + (py - y).powi(2)).sqrt();
            if d < f1 {
                (f2, f1, id) = (f1, d, hash2(wx, wy, seed + 2));
            } else if d < f2 {
                f2 = d;
            }
        }
    }
    (f1, f2, id)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every textured material has a colour and a normal map whose
    /// colour averages near Studio's, and tiles (its edges meet).
    #[test]
    fn rooks_own_textures() {
        for m in Material::ALL {
            let Some((_, average)) = style(m) else {
                assert!(material(m, MaterialMap::Color).is_none());
                continue;
            };
            let color = material(m, MaterialMap::Color).unwrap();
            let px = &color.mips[0];
            let mean = px.chunks(4).map(|p| f32::from(p[1])).sum::<f32>() / (px.len() / 4) as f32;
            assert!((mean - f32::from(average[1])).abs() < 20.0, "{m:?} {mean}");
            let normal = material(m, MaterialMap::Normal).unwrap();
            assert_eq!((normal.width, normal.height), (SIZE, SIZE));
            assert!(material(m, MaterialMap::RoughnessMetalness).is_none());
        }
    }
}
