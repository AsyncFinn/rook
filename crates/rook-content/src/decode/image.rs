//! Reading images (decal, texture and mesh texture files) into RGBA8 with
//! a full mip chain, the layout the renderer uploads, or block-compressed
//! ([`blocks`]: material textures).
//!
//! Formats are recognized by their bytes, not by a file name (an asset id
//! has none): PNG, JPEG, BMP and DDS (DXT1, DXT3 and DXT5; the install's
//! `.tex` files are DDS too), the formats found among the archive's images
//! that load (docs/behavior/rendering/textures-and-decals.md: no
//! GIF or TGA turned up), and KTX2 (BC1 and BC3), in which Roblox's material textures
//! come ([`ktx2`]). Anything else is not an image, as in Studio, where a
//! mesh or sound used as a texture draws nothing (docs/behavior/content/asset-fetching.md).
//!
//! Decoding goes through the `image` crate.

mod blocks;
pub(crate) mod ktx2;
mod mips;

pub use blocks::Encoding;
pub use mips::mips;

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

/// A decoded image: RGBA8 (sRGB color, straight alpha) or blocks of it,
/// each mip level half the one before down to 1 × 1.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub format: ImageFormat,
    /// How [`Image::mips`] are stored.
    pub encoding: Encoding,
    /// Level 0 first; level `i` is `max(1, width >> i)` by
    /// `max(1, height >> i)` pixels, rows packed (in blocks, rows of
    /// blocks, a partial block counting whole).
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
            let half = mips::downsample(&rgba, w, h);
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
            encoding: Encoding::Rgba8,
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

#[cfg(test)]
mod tests;
