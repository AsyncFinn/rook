//! Reading images (decal, texture and mesh texture files) into RGBA8 with
//! a full mip chain, the layout the renderer uploads, or block-compressed
//! ([`blocks`]: material textures, Roblox's transcoded textures and large
//! uploads).
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

/// The largest side kept, Roblox's limit for uploads and the largest
/// texture it renders (docs/behavior/content/asset-fetching.md#textures).
/// Larger images are halved until they fit.
pub const MAX_SIZE: u32 = 4096;

/// Textures decoded with a side past this ([`Image::decode_texture`]) are
/// kept block-compressed (BC1, or BC3 where they have transparency), as
/// Roblox keeps every texture: a 4096 × 4096 image is 85 MB in RGBA8 with
/// its mips and 11 to 22 MB in blocks. Smaller ones stay RGBA8, as they
/// always have in Rook.
pub const COMPRESS_OVER: u32 = 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum ImageFormat {
    Png,
    Jpeg,
    Bmp,
    Dds,
    Ktx2,
}

impl ImageFormat {
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
    pub encoding: Encoding,
    /// Level 0 first; level `i` is `max(1, width >> i)` by
    /// `max(1, height >> i)` pixels, rows packed (in blocks, rows of
    /// blocks, a partial block counting whole).
    pub mips: Vec<Vec<u8>>,
    /// Whether any pixel is less than fully opaque.
    pub translucent: bool,
    /// The size GUI measures the image in (`ImageRectOffset`,
    /// `ImageRectSize`), whatever size its pixels are ([`gui_size`]).
    pub gui_size: (u32, u32),
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ImageError {
    #[error("not an image")]
    NotAnImage,
    #[error("{0} image is damaged or unsupported: {1}")]
    Damaged(&'static str, String),
}

impl Image {
    /// An image, RGBA8 with Rook's mips, or in its blocks when it's a KTX2
    /// texture holding every level (Roblox's transcode).
    pub fn decode(bytes: &[u8]) -> Result<Self, ImageError> {
        if ImageFormat::sniff(bytes) == Some(ImageFormat::Ktx2)
            && let Some(image) = Self::from_ktx2_chain(bytes)?
        {
            return Ok(image);
        }
        let (rgba, format) = decode_rgba(bytes)?;
        Self::from_rgba(rgba, format, MAX_SIZE)
    }

    /// A texture for the GPU to sample only (what the loader decodes for
    /// `Want::Texture`): as [`Self::decode`], but kept in blocks past
    /// [`COMPRESS_OVER`]. What reads pixels itself (the install's surface
    /// and effect textures) decodes with [`Self::decode`].
    pub fn decode_texture(bytes: &[u8]) -> Result<Self, ImageError> {
        let image = Self::decode(bytes)?;
        if image.encoding == Encoding::Rgba8
            && (image.width > COMPRESS_OVER || image.height > COMPRESS_OVER)
        {
            let encoding = if image.translucent {
                Encoding::Bc3
            } else {
                Encoding::Bc1
            };
            return Ok(image.compressed(encoding));
        }
        Ok(image)
    }

    /// An image for GUI, the sky and other `Want::Image` uses: as
    /// [`Self::decode`], fitted to [`GUI_MAX_SIZE`] (the aspect kept), the
    /// size Roblox draws an upload at outside surfaces in the world. GUI
    /// measures image rects in that size (`gui_size`), and asset delivery
    /// stores uploads at it (a 4096 × 4096 decal's upload is a 1024 × 1024
    /// PNG); only textures on surfaces come larger, transcoded.
    pub fn decode_gui(bytes: &[u8]) -> Result<Self, ImageError> {
        if ImageFormat::sniff(bytes) == Some(ImageFormat::Ktx2)
            && let Some(mut image) = Self::from_ktx2_chain(bytes)?
        {
            while image.width.max(image.height) > GUI_MAX_SIZE && image.mips.len() > 1 {
                image.mips.remove(0);
                image.width = (image.width / 2).max(1);
                image.height = (image.height / 2).max(1);
            }
            return Ok(image);
        }
        let (mut rgba, format) = decode_rgba(bytes)?;
        let (fit_w, fit_h) = gui_size(rgba.dimensions());
        if (fit_w, fit_h) != rgba.dimensions() {
            rgba = ::image::imageops::resize(
                &rgba,
                fit_w,
                fit_h,
                ::image::imageops::FilterType::Triangle,
            );
        }
        let mut image = Self::from_rgba(rgba, format, GUI_MAX_SIZE)?;
        image.gui_size = (fit_w, fit_h);
        Ok(image)
    }

    /// A KTX2 texture holding every level (Roblox's transcode, its packs
    /// joined), kept in its blocks; `None` when the file isn't one.
    fn from_ktx2_chain(bytes: &[u8]) -> Result<Option<Self>, ImageError> {
        let file = ktx2::File::read(bytes)?;
        let Some((encoding, mips)) = file.chain()? else {
            return Ok(None);
        };
        let translucent = blocks::translucent(encoding, &mips[0]);
        Ok(Some(Self {
            width: file.width,
            height: file.height,
            format: ImageFormat::Ktx2,
            encoding,
            mips,
            translucent,
            gui_size: gui_size(file.original_size().unwrap_or((file.width, file.height))),
        }))
    }

    /// An image from decoded pixels, halved until neither side is over
    /// `max_size`, with its mips.
    pub fn from_rgba(
        mut rgba: ::image::RgbaImage,
        format: ImageFormat,
        max_size: u32,
    ) -> Result<Self, ImageError> {
        if max_size == 0 {
            return Err(ImageError::Damaged(
                format.name(),
                "maximum size must be greater than zero".into(),
            ));
        }
        let gui_size = gui_size(rgba.dimensions());
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
            gui_size,
        })
    }

    /// Bytes held, over every level.
    pub fn byte_len(&self) -> usize {
        self.mips.iter().map(Vec::len).sum()
    }
}

/// The size GUI measures an image of `size` in: its upload's, scaled
/// down (the aspect kept) to fit 1024 × 1024, the size Roblox stored
/// uploads at before it drew larger textures. Measured in Studio 0.748
/// (docs/behavior/content/asset-fetching.md#textures): a 4096 × 4096 upload
/// measures 1024 × 1024, a 1264 × 632 one (served as 1216 × 576) 1024 × 512.
pub fn gui_size((width, height): (u32, u32)) -> (u32, u32) {
    let longest = width.max(height);
    if longest <= GUI_MAX_SIZE {
        return (width, height);
    }
    let scale = |side: u32| {
        ((u64::from(side) * u64::from(GUI_MAX_SIZE) + u64::from(longest) / 2) / u64::from(longest))
            .max(1) as u32
    };
    (scale(width), scale(height))
}

/// The largest side GUI measures an image in ([`gui_size`]).
pub const GUI_MAX_SIZE: u32 = 1024;

/// Levels in a whole mip chain of `width` × `height`, down to 1 × 1.
pub(crate) fn level_count(width: u32, height: u32) -> usize {
    (u32::BITS - width.max(height).max(1).leading_zeros()) as usize
}

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
