//! Signed distance fields for glyphs, which draw sharp at any size: a
//! renderer smooths each pixel across the field's middle level by how fast
//! the field changes on screen, as Roblox's GUI shader does for text
//! (docs/behavior/gui/world.md#drawing). A glyph's coverage is rendered
//! finely, each fine pixel's distance to the outline is measured exactly,
//! and the distances are averaged down to the field.

use zeno::{Command, Mask};

/// The line height fields are made at, in field pixels.
pub(crate) const FIELD_PX: f32 = 64.0;
/// How far the field reaches from the outline, in field pixels: 0 this
/// far outside, 255 this far inside.
const SPREAD: f32 = 6.0;
/// Coverage is rendered this many times finer than the field.
pub(super) const FINE: usize = 4;
/// Stands in for "no seed in reach" in the distance transform.
const FAR: f64 = 1e20;

/// A glyph's field and where its top left sits from the pen, in field
/// pixels (y down).
pub(crate) struct Field {
    pub(crate) pixels: Vec<u8>,
    pub(crate) width: u32,
    pub(crate) height: u32,
    pub(crate) left: f32,
    pub(crate) top: f32,
}

/// The field of an outline drawn `FINE` times larger than the field.
pub(crate) fn field(fine_outline: &[Command]) -> Option<Field> {
    let (coverage, placement) = Mask::new(fine_outline).render();
    let (w, h) = (placement.width as usize, placement.height as usize);
    if w == 0 || h == 0 {
        return None;
    }
    // Room for the spread all round, in whole field pixels.
    let pad = (SPREAD as usize + 1) * FINE;
    let (cols, rows) = ((w + 2 * pad).div_ceil(FINE), (h + 2 * pad).div_ceil(FINE));
    let (fw, fh) = (cols * FINE, rows * FINE);
    let mut inside = vec![false; fw * fh];
    for y in 0..h {
        for x in 0..w {
            inside[(y + pad) * fw + x + pad] = coverage[y * w + x] >= 128;
        }
    }
    let to_inside = transform(&inside, fw, fh, true);
    let to_outside = transform(&inside, fw, fh, false);
    // Each fine pixel's distance to the outline, which runs half a pixel
    // from the nearest pixel across it; positive inside.
    let signed: Vec<f32> = (0..fw * fh)
        .map(|i| {
            if inside[i] {
                to_outside[i].sqrt() as f32 - 0.5
            } else {
                0.5 - to_inside[i].sqrt() as f32
            }
        })
        .collect();
    let mut pixels = Vec::with_capacity(cols * rows);
    for row in 0..rows {
        for col in 0..cols {
            let mut sum = 0.0;
            for y in row * FINE..(row + 1) * FINE {
                for x in col * FINE..(col + 1) * FINE {
                    sum += signed[y * fw + x];
                }
            }
            let distance = sum / (FINE * FINE * FINE) as f32;
            let level = 0.5 + distance / (2.0 * SPREAD);
            pixels.push((level.clamp(0.0, 1.0) * 255.0).round() as u8);
        }
    }
    let fine = FINE as f32;
    Some(Field {
        pixels,
        width: cols as u32,
        height: rows as u32,
        left: (placement.left as f32 - pad as f32) / fine,
        top: (placement.top as f32 - pad as f32) / fine,
    })
}

/// Each pixel's squared distance to the nearest pixel whose `inside` is
/// `seed` (Felzenszwalb and Huttenlocher's exact transform, by rows then
/// columns).
fn transform(inside: &[bool], w: usize, h: usize, seed: bool) -> Vec<f64> {
    let mut d: Vec<f64> = inside
        .iter()
        .map(|&i| if i == seed { 0.0 } else { FAR })
        .collect();
    let n = w.max(h);
    let (mut f, mut out) = (vec![0.0; n], vec![0.0; n]);
    let (mut v, mut z) = (vec![0usize; n], vec![0.0; n + 1]);
    for y in 0..h {
        f[..w].copy_from_slice(&d[y * w..(y + 1) * w]);
        line(&f[..w], &mut out[..w], &mut v, &mut z);
        d[y * w..(y + 1) * w].copy_from_slice(&out[..w]);
    }
    for x in 0..w {
        for y in 0..h {
            f[y] = d[y * w + x];
        }
        line(&f[..h], &mut out[..h], &mut v, &mut z);
        for y in 0..h {
            d[y * w + x] = out[y];
        }
    }
    d
}

/// The 1D squared distance transform of `f` into `out`: the lower
/// envelope of the parabolas rooted at each sample.
fn line(f: &[f64], out: &mut [f64], v: &mut [usize], z: &mut [f64]) {
    let mut k = 0;
    v[0] = 0;
    z[0] = f64::NEG_INFINITY;
    z[1] = f64::INFINITY;
    for q in 1..f.len() {
        let qf = q as f64;
        let mut s;
        loop {
            let r = v[k] as f64;
            s = ((f[q] + qf * qf) - (f[v[k]] + r * r)) / (2.0 * (qf - r));
            if s > z[k] {
                break;
            }
            k -= 1;
        }
        k += 1;
        v[k] = q;
        z[k] = s;
        z[k + 1] = f64::INFINITY;
    }
    k = 0;
    for (q, o) in out.iter_mut().enumerate() {
        while z[k + 1] < q as f64 {
            k += 1;
        }
        let r = v[k] as f64;
        *o = (q as f64 - r).powi(2) + f[v[k]];
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zeno::Point;

    /// A square's field is 128 on its edge, falls off one level per
    /// 1/(2 × SPREAD) of a pixel each way, and reaches the spread.
    #[test]
    fn square_field() {
        // 16 field pixels square, drawn 4 times finer.
        let s = 16.0 * FINE as f32;
        let path = [
            Command::MoveTo(Point::new(0.0, 0.0)),
            Command::LineTo(Point::new(s, 0.0)),
            Command::LineTo(Point::new(s, s)),
            Command::LineTo(Point::new(0.0, s)),
            Command::Close,
        ];
        let f = field(&path).unwrap();
        let pad = SPREAD + 1.0;
        assert_eq!((f.left, f.top), (-pad, -pad));
        let at = |x: f32, y: f32| {
            let (col, row) = ((x + pad) as usize, (y + pad) as usize);
            f.pixels[row * f.width as usize + col] as f32
        };
        // A pixel's center half a pixel in from the edge, and out.
        let step = 255.0 / (2.0 * SPREAD);
        assert!((at(0.0, 8.0) - (127.5 + 0.5 * step)).abs() < 1.0);
        assert!((at(-1.0, 8.0) - (127.5 - 0.5 * step)).abs() < 1.0);
        assert_eq!(at(8.0, 8.0), 255.0);
        assert_eq!(at(-pad, 8.0), 0.0);
    }
}
