//! Reading images (decal, texture and mesh texture files) into RGBA8 with
//! a full mip chain, the one layout the renderer uploads.
//!
//! Formats are recognised by their bytes, not by a file name (an asset id
//! has none): PNG, JPEG, BMP and DDS (DXT1, DXT3 and DXT5; the install's
//! `.tex` files are DDS too), the formats found among the archive's images
//! that load (ROADMAP M4 "Textures and decals (step 4)": no GIF or TGA
//! turned up), and KTX2 (BC1 and BC3), in which Roblox's material textures
//! come ([`ktx2`]). Anything else is not an image, as in Studio, where a
//! mesh or sound used as a texture draws nothing (ROADMAP M4 step 2).
//!
//! Decoding goes through the `image` crate.

pub(crate) mod ktx2;

use std::io::Cursor;

/// The largest side kept. Larger images are halved until they fit
/// (Inference: Roblox stores uploaded images at 1024 at most).
pub const MAX_SIZE: u32 = 1024;

/// A file format Rook reads images from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ImageFormat {
    Png,
    Jpeg,
    Bmp,
    Dds,
    Ktx2,
}

impl ImageFormat {
    /// The format a file's first bytes announce, if it's one Rook reads.
    pub fn sniff(bytes: &[u8]) -> Option<Self> {
        let starts = |magic: &[u8]| bytes.starts_with(magic);
        Some(if starts(b"\x89PNG\r\n\x1a\n") {
            Self::Png
        } else if starts(b"\xFF\xD8\xFF") {
            Self::Jpeg
        } else if starts(b"BM") && bytes.len() > 26 {
            Self::Bmp
        } else if starts(b"DDS ") {
            Self::Dds
        } else if starts(ktx2::MAGIC) {
            Self::Ktx2
        } else {
            return None;
        })
    }

    pub fn name(self) -> &'static str {
        match self {
            Self::Png => "PNG",
            Self::Jpeg => "JPEG",
            Self::Bmp => "BMP",
            Self::Dds => "DDS",
            Self::Ktx2 => "KTX2",
        }
    }

    fn codec(self) -> ::image::ImageFormat {
        match self {
            Self::Png => ::image::ImageFormat::Png,
            Self::Jpeg => ::image::ImageFormat::Jpeg,
            Self::Bmp => ::image::ImageFormat::Bmp,
            Self::Dds | Self::Ktx2 => ::image::ImageFormat::Dds,
        }
    }
}

/// A decoded image: RGBA8 (sRGB colour, straight alpha), each mip level
/// half the one before down to 1 × 1.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub format: ImageFormat,
    /// Level 0 first; level `i` is `max(1, width >> i)` by
    /// `max(1, height >> i)` pixels, rows packed.
    pub mips: Vec<Vec<u8>>,
    /// Whether any pixel is less than fully opaque.
    pub translucent: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ImageError {
    #[error("not an image")]
    NotAnImage,
    #[error("{0} image is damaged or unsupported: {1}")]
    Damaged(&'static str, String),
}

impl Image {
    /// Decode an image file and build its mips.
    pub fn decode(bytes: &[u8]) -> Result<Self, ImageError> {
        let (rgba, format) = decode_rgba(bytes)?;
        Self::from_rgba(rgba, format, MAX_SIZE)
    }

    /// An image from decoded pixels, halved until neither side is over
    /// `max_size`, with its mips.
    pub fn from_rgba(
        mut rgba: ::image::RgbaImage,
        format: ImageFormat,
        max_size: u32,
    ) -> Result<Self, ImageError> {
        while rgba.width() > max_size || rgba.height() > max_size {
            let (w, h) = (rgba.width(), rgba.height());
            let half = downsample(&rgba, w, h);
            rgba = ::image::RgbaImage::from_raw((w / 2).max(1), (h / 2).max(1), half)
                .expect("halved image size");
        }
        let (width, height) = rgba.dimensions();
        if width == 0 || height == 0 {
            return Err(ImageError::Damaged(format.name(), "empty image".into()));
        }
        let level0 = rgba.into_raw();
        let translucent = level0.as_chunks::<4>().0.iter().any(|p| p[3] < 255);
        Ok(Self {
            width,
            height,
            format,
            mips: mips(level0, width, height),
            translucent,
        })
    }

    /// Bytes held, over every level.
    pub fn byte_len(&self) -> usize {
        self.mips.iter().map(Vec::len).sum()
    }
}

/// An image file's pixels, as RGBA8, and its format.
pub(crate) fn decode_rgba(bytes: &[u8]) -> Result<(::image::RgbaImage, ImageFormat), ImageError> {
    let format = ImageFormat::sniff(bytes).ok_or(ImageError::NotAnImage)?;
    let rgba = match format {
        ImageFormat::Ktx2 => ktx2::decode(bytes)?,
        _ => ::image::load(Cursor::new(bytes), format.codec())
            .map_err(|e| ImageError::Damaged(format.name(), e.to_string()))?
            .into_rgba8(),
    };
    Ok((rgba, format))
}

/// The mip chain of an RGBA8 image of `width` × `height`, starting with
/// `level0` itself.
pub fn mips(level0: Vec<u8>, width: u32, height: u32) -> Vec<Vec<u8>> {
    let mut mips = vec![level0];
    let (mut w, mut h) = (width, height);
    while w > 1 || h > 1 {
        let next = downsample(mips.last().expect("level"), w, h);
        mips.push(next);
        w = (w / 2).max(1);
        h = (h / 2).max(1);
    }
    mips
}

/// Halve an RGBA8 image (a side of 1 stays 1), averaging each 2 × 2 block
/// weighted by alpha, so fully transparent pixels don't bleed their colour
/// into the edges of what's drawn.
fn downsample(src: &[u8], w: u32, h: u32) -> Vec<u8> {
    let (nw, nh) = ((w / 2).max(1), (h / 2).max(1));
    let mut out = Vec::with_capacity((nw * nh * 4) as usize);
    let at = |x: u32, y: u32| {
        let i = ((y.min(h - 1) * w + x.min(w - 1)) * 4) as usize;
        &src[i..i + 4]
    };
    for y in 0..nh {
        for x in 0..nw {
            let (sx, sy) = (x * 2, y * 2);
            let block = [
                at(sx, sy),
                at(sx + 1, sy),
                at(sx, sy + 1),
                at(sx + 1, sy + 1),
            ];
            let alpha: u32 = block.iter().map(|p| u32::from(p[3])).sum();
            for c in 0..3 {
                let weighted: u32 = block
                    .iter()
                    .map(|p| u32::from(p[c]) * u32::from(p[3]))
                    .sum();
                // All transparent: a plain average.
                let v = (weighted + alpha / 2)
                    .checked_div(alpha)
                    .unwrap_or_else(|| block.iter().map(|p| u32::from(p[c])).sum::<u32>() / 4);
                out.push(v as u8);
            }
            out.push(((alpha + 2) / 4) as u8);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 4 × 2 PNG: the left half red, the right half transparent.
    fn png() -> Vec<u8> {
        let mut img = ::image::RgbaImage::new(4, 2);
        for (x, _, p) in img.enumerate_pixels_mut() {
            *p = if x < 2 {
                ::image::Rgba([255, 0, 0, 255])
            } else {
                ::image::Rgba([0, 255, 0, 0])
            };
        }
        let mut bytes = Vec::new();
        img.write_to(&mut Cursor::new(&mut bytes), ::image::ImageFormat::Png)
            .unwrap();
        bytes
    }

    #[test]
    fn decodes_png_with_mips() {
        let img = Image::decode(&png()).unwrap();
        assert_eq!(
            (img.width, img.height, img.format),
            (4, 2, ImageFormat::Png)
        );
        assert!(img.translucent);
        // 4x2, 2x1, 1x1.
        assert_eq!(
            img.mips.iter().map(Vec::len).collect::<Vec<_>>(),
            [32, 8, 4]
        );
        // The transparent half doesn't tint the red: 2x1 is red, then
        // transparent; 1x1 is red at half alpha.
        assert_eq!(&img.mips[1][..4], &[255, 0, 0, 255]);
        assert_eq!(img.mips[1][7], 0);
        assert_eq!(img.mips[2], [255, 0, 0, 128]);
    }

    #[test]
    fn other_formats() {
        let src = ::image::DynamicImage::ImageRgba8(::image::RgbaImage::from_pixel(
            8,
            8,
            ::image::Rgba([10, 200, 30, 255]),
        ));
        for (format, codec) in [
            (ImageFormat::Jpeg, ::image::ImageFormat::Jpeg),
            (ImageFormat::Bmp, ::image::ImageFormat::Bmp),
        ] {
            let mut bytes = Vec::new();
            let img = if format == ImageFormat::Jpeg {
                ::image::DynamicImage::ImageRgb8(src.to_rgb8())
            } else {
                src.clone()
            };
            img.write_to(&mut Cursor::new(&mut bytes), codec).unwrap();
            let decoded = Image::decode(&bytes).unwrap();
            assert_eq!((decoded.format, decoded.width), (format, 8));
            assert!(!decoded.translucent);
            assert_eq!(decoded.mips.len(), 4);
        }
    }

    /// A 4 × 4 DXT1 DDS: one block whose two colours are pure red and
    /// pure blue, every texel the first.
    #[test]
    fn decodes_dds() {
        let mut b = b"DDS ".to_vec();
        let mut header = [0u32; 31];
        header[0] = 124; // size
        header[1] = 0x1 | 0x2 | 0x4 | 0x1000 | 0x8_0000; // flags
        header[2] = 4; // height
        header[3] = 4; // width
        header[4] = 8; // linear size
        header[18] = 32; // pixel format size
        header[19] = 0x4; // FOURCC
        header[20] = u32::from_le_bytes(*b"DXT1");
        header[26] = 0x1000; // caps: texture
        for v in header {
            b.extend_from_slice(&v.to_le_bytes());
        }
        b.extend_from_slice(&0xF800u16.to_le_bytes());
        b.extend_from_slice(&0x001Fu16.to_le_bytes());
        b.extend_from_slice(&[0; 4]);
        let img = Image::decode(&b).unwrap();
        assert_eq!(
            (img.format, img.width, img.height),
            (ImageFormat::Dds, 4, 4)
        );
        assert_eq!(&img.mips[0][..4], &[255, 0, 0, 255]);
    }

    #[test]
    fn rejects_what_isnt_an_image() {
        assert_eq!(
            Image::decode(b"version 2.00\n..."),
            Err(ImageError::NotAnImage)
        );
        assert_eq!(Image::decode(b"OggS...."), Err(ImageError::NotAnImage));
        let full = png();
        assert!(matches!(
            Image::decode(&full[..full.len() / 2]),
            Err(ImageError::Damaged("PNG", _))
        ));
        // A PNG signature followed by junk.
        let mut junk = full[..16].to_vec();
        junk.extend_from_slice(&[7; 64]);
        assert!(matches!(
            Image::decode(&junk),
            Err(ImageError::Damaged("PNG", _))
        ));
    }

    #[test]
    fn halves_images_past_the_limit() {
        let big = ::image::RgbaImage::from_pixel(2048, 4, ::image::Rgba([1, 2, 3, 255]));
        let mut bytes = Vec::new();
        big.write_to(&mut Cursor::new(&mut bytes), ::image::ImageFormat::Png)
            .unwrap();
        let img = Image::decode(&bytes).unwrap();
        assert_eq!((img.width, img.height), (1024, 2));
    }
}
