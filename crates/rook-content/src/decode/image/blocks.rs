//! Block-compressed images: 4 × 4 pixel blocks the GPU samples as they
//! are, at an eighth (BC1) or a quarter (BC3, BC7) of RGBA8's memory.
//! Material textures are kept this way at their full 1024 × 1024, as
//! Roblox serves them in BC1 and BC3 ([`super::ktx2`]); the
//! roughness-metalness-height map Rook packs from three images is BC7,
//! whose blocks keep the three channels apart. Encoding goes through
//! Intel's ISPC texture compressor (`intel_tex_2`), decoding (for devices
//! that can't sample blocks) through `bcdec_rs`.

use std::borrow::Cow;

use super::Image;

/// How an image's levels are stored.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Encoding {
    /// Four bytes a pixel, rows packed.
    Rgba8,
    /// 8 bytes a block: opaque color.
    Bc1,
    /// 16 bytes a block: BC1's color and its own alpha.
    Bc3,
    /// 16 bytes a block, each channel kept apart.
    Bc7,
}

impl Encoding {
    /// Bytes a 4 × 4 block takes (none for RGBA8, which isn't in blocks).
    pub fn block_bytes(self) -> Option<usize> {
        match self {
            Self::Rgba8 => None,
            Self::Bc1 => Some(8),
            Self::Bc3 | Self::Bc7 => Some(16),
        }
    }
}

impl Image {
    /// The image as `encoding`'s blocks, every level (a level under 4 × 4
    /// takes one block, its edge pixels repeated). An image already in
    /// blocks, or whose sides aren't multiples of 4, stays as it is.
    pub fn compressed(self, encoding: Encoding) -> Self {
        if self.encoding != Encoding::Rgba8
            || encoding == Encoding::Rgba8
            || !self.width.is_multiple_of(4)
            || !self.height.is_multiple_of(4)
        {
            return self;
        }
        profiling::scope!("compress image");
        let mips = self
            .mips
            .iter()
            .enumerate()
            .map(|(i, level)| {
                let (w, h) = level_size(self.width, self.height, i);
                let (pixels, w, h) = padded(level, w, h);
                let surface = intel_tex_2::RgbaSurface {
                    data: &pixels,
                    width: w,
                    height: h,
                    stride: w * 4,
                };
                match encoding {
                    Encoding::Bc1 => intel_tex_2::bc1::compress_blocks(&surface),
                    Encoding::Bc3 => intel_tex_2::bc3::compress_blocks(&surface),
                    // Its quickest settings (one mode): 20 ms for a 1024
                    // map where the next ones take 240 and 420.
                    Encoding::Bc7 => intel_tex_2::bc7::compress_blocks(
                        &intel_tex_2::bc7::opaque_ultra_fast_settings(),
                        &surface,
                    ),
                    Encoding::Rgba8 => unreachable!("returned above"),
                }
            })
            .collect();
        Self {
            mips,
            encoding,
            ..self
        }
    }

    /// The image as RGBA8, from its first level no larger than `max_size`
    /// on either side (for a device that can't sample blocks).
    pub fn decompressed(&self, max_size: u32) -> Self {
        let first = (0..self.mips.len())
            .find(|&i| {
                let (w, h) = level_size(self.width, self.height, i);
                w <= max_size && h <= max_size
            })
            .unwrap_or(self.mips.len() - 1);
        let (width, height) = level_size(self.width, self.height, first);
        let mips = (first..self.mips.len())
            .map(|i| {
                let (w, h) = level_size(self.width, self.height, i);
                self.level_rgba(i, w, h)
            })
            .collect();
        Self {
            width,
            height,
            format: self.format,
            encoding: Encoding::Rgba8,
            mips,
            translucent: self.translucent,
        }
    }

    /// Level `i` (`w` × `h`) as RGBA8.
    fn level_rgba(&self, i: usize, w: u32, h: u32) -> Vec<u8> {
        let level = &self.mips[i];
        let decode = match self.encoding {
            Encoding::Rgba8 => return level.clone(),
            Encoding::Bc1 => bcdec_rs::bc1,
            Encoding::Bc3 => bcdec_rs::bc3,
            Encoding::Bc7 => bcdec_rs::bc7,
        };
        let block_bytes = self.encoding.block_bytes().expect("in blocks");
        let (bw, bh) = (w.div_ceil(4) as usize, h.div_ceil(4) as usize);
        // Whole blocks, then cut to the level's size.
        let pitch = bw * 16;
        let mut whole = vec![0; pitch * bh * 4];
        for (b, block) in level.chunks_exact(block_bytes).enumerate() {
            let (bx, by) = (b % bw, b / bw);
            decode(block, &mut whole[by * 4 * pitch + bx * 16..], pitch);
        }
        let row = w as usize * 4;
        (0..h as usize)
            .flat_map(|y| &whole[y * pitch..y * pitch + row])
            .copied()
            .collect()
    }
}

/// Level `level`'s size in an image of `width` × `height`.
fn level_size(width: u32, height: u32, level: usize) -> (u32, u32) {
    ((width >> level).max(1), (height >> level).max(1))
}

/// An RGBA8 level of `w` × `h`, made whole blocks by repeating its last
/// column and row, and its new size.
fn padded(level: &[u8], w: u32, h: u32) -> (Cow<'_, [u8]>, u32, u32) {
    let (pw, ph) = (w.next_multiple_of(4), h.next_multiple_of(4));
    if (pw, ph) == (w, h) {
        return (Cow::Borrowed(level), w, h);
    }
    let mut out = Vec::with_capacity((pw * ph * 4) as usize);
    for y in 0..ph {
        for x in 0..pw {
            let i = ((y.min(h - 1) * w + x.min(w - 1)) * 4) as usize;
            out.extend_from_slice(&level[i..i + 4]);
        }
    }
    (Cow::Owned(out), pw, ph)
}

#[cfg(test)]
mod tests {
    use super::super::ImageFormat;
    use super::*;

    /// An 8 × 8 image: four flat quarters.
    fn quarters() -> Image {
        let colors = [
            [200, 30, 70, 255],
            [10, 120, 250, 255],
            [0, 0, 0, 255],
            [255; 4],
        ];
        let img = ::image::RgbaImage::from_fn(8, 8, |x, y| {
            ::image::Rgba(colors[(x / 4 + y / 4 * 2) as usize])
        });
        Image::from_rgba(img, ImageFormat::Png, 1024).unwrap()
    }

    /// Each encoding keeps every level (small ones in one block), and its
    /// flat blocks decode back to about the same pixels.
    #[test]
    fn compresses_and_decompresses() {
        let plain = quarters();
        for encoding in [Encoding::Bc1, Encoding::Bc3, Encoding::Bc7] {
            let packed = plain.clone().compressed(encoding);
            let block = encoding.block_bytes().unwrap();
            // 8x8: 4 blocks, 4x4, 2x2 and 1x1: one each.
            assert_eq!(
                packed.mips.iter().map(Vec::len).collect::<Vec<_>>(),
                [4 * block, block, block, block]
            );
            let back = packed.decompressed(1024);
            assert_eq!(back.encoding, Encoding::Rgba8);
            let sizes = |i: &Image| i.mips.iter().map(Vec::len).collect::<Vec<_>>();
            assert_eq!(sizes(&plain), sizes(&back));
            for (x, y) in plain.mips[0].iter().zip(&back.mips[0]) {
                assert!(x.abs_diff(*y) <= 8, "{encoding:?}: {x} vs {y}");
            }
        }
    }

    /// Decompressing for a smaller device starts at the first level that
    /// fits.
    #[test]
    fn decompresses_from_a_smaller_level() {
        let back = quarters().compressed(Encoding::Bc1).decompressed(4);
        assert_eq!((back.width, back.height, back.mips.len()), (4, 4, 3));
        assert_eq!(back.mips[0].len(), 4 * 4 * 4);
    }

    /// Sides that aren't multiples of 4 stay RGBA8.
    #[test]
    fn odd_sizes_stay_rgba() {
        let img = ::image::RgbaImage::from_pixel(6, 4, ::image::Rgba([1, 2, 3, 255]));
        let img = Image::from_rgba(img, ImageFormat::Png, 1024).unwrap();
        assert_eq!(img.compressed(Encoding::Bc1).encoding, Encoding::Rgba8);
    }
}
