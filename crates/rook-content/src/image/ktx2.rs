//! KTX2 textures, the form Roblox's content delivery serves material
//! textures in (`docs/behavior/materials.md`): BC1 or BC3 blocks, each mip
//! level Zstandard-compressed (supercompression scheme 2) or stored as is,
//! with key-value data naming the texture (`contentHash`).
//!
//! Only the first (largest) level is read; Rook builds the smaller ones
//! itself. The blocks are decoded by wrapping them in a DDS header and
//! handing them to the same DDS decoder the install's textures go through.

use std::io::Cursor;

use super::ImageError;

pub(crate) const MAGIC: &[u8] = b"\xABKTX 20\xBB\r\n\x1A\n";

const VK_BC1_RGB_UNORM: u32 = 131;
const VK_BC1_RGB_SRGB: u32 = 132;
const VK_BC1_RGBA_UNORM: u32 = 133;
const VK_BC1_RGBA_SRGB: u32 = 134;
const VK_BC3_UNORM: u32 = 137;
const VK_BC3_SRGB: u32 = 138;
const SUPERCOMPRESSION_NONE: u32 = 0;
const SUPERCOMPRESSION_ZSTD: u32 = 2;

/// The fixed part of a KTX2 file's header.
#[derive(Debug, Clone, Copy)]
struct Header {
    vk_format: u32,
    width: u32,
    height: u32,
    supercompression: u32,
    /// The first level's offset, length and uncompressed length.
    level0: (usize, usize, usize),
}

fn u32_at(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(at..at + 4)?.try_into().ok()?))
}

fn u64_at(b: &[u8], at: usize) -> Option<usize> {
    usize::try_from(u64::from_le_bytes(b.get(at..at + 8)?.try_into().ok()?)).ok()
}

fn header(b: &[u8]) -> Option<Header> {
    if !b.starts_with(MAGIC) {
        return None;
    }
    Some(Header {
        vk_format: u32_at(b, 12)?,
        width: u32_at(b, 20)?,
        height: u32_at(b, 24)?.max(1),
        supercompression: u32_at(b, 44)?,
        level0: (u64_at(b, 80)?, u64_at(b, 88)?, u64_at(b, 96)?),
    })
}

/// The first level as RGBA8 (as stored: no colour conversion).
pub(crate) fn decode(b: &[u8]) -> Result<::image::RgbaImage, ImageError> {
    let damaged = |why: &str| ImageError::Damaged("KTX2", why.to_owned());
    let h = header(b).ok_or_else(|| damaged("bad header"))?;
    let fourcc: &[u8; 4] = match h.vk_format {
        VK_BC1_RGB_UNORM | VK_BC1_RGB_SRGB | VK_BC1_RGBA_UNORM | VK_BC1_RGBA_SRGB => b"DXT1",
        VK_BC3_UNORM | VK_BC3_SRGB => b"DXT5",
        other => return Err(damaged(&format!("format {other} isn't BC1 or BC3"))),
    };
    let (offset, len, uncompressed) = h.level0;
    let stored = b
        .get(offset..offset + len)
        .ok_or_else(|| damaged("level past the end"))?;
    let blocks = match h.supercompression {
        SUPERCOMPRESSION_NONE => stored.to_vec(),
        SUPERCOMPRESSION_ZSTD => zstd::bulk::decompress(stored, uncompressed)
            .map_err(|e| damaged(&format!("zstd: {e}")))?,
        other => return Err(damaged(&format!("supercompression {other}"))),
    };
    let dds = dds_file(fourcc, h.width, h.height, &blocks);
    let decoded = ::image::load(Cursor::new(dds), ::image::ImageFormat::Dds)
        .map_err(|e| damaged(&e.to_string()))?;
    Ok(decoded.into_rgba8())
}

/// A DDS file holding one level of `fourcc` blocks.
fn dds_file(fourcc: &[u8; 4], width: u32, height: u32, blocks: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(128 + blocks.len());
    let mut put = |v: u32| out.extend_from_slice(&v.to_le_bytes());
    put(u32::from_le_bytes(*b"DDS "));
    put(124); // header size
    put(0x1 | 0x2 | 0x4 | 0x1000 | 0x80000); // caps, height, width, pixel format, linear size
    put(height);
    put(width);
    put(blocks.len() as u32);
    put(0); // depth
    put(0); // mip count
    for _ in 0..11 {
        put(0);
    }
    put(32); // pixel format size
    put(0x4); // four-cc
    put(u32::from_le_bytes(*fourcc));
    for _ in 0..5 {
        put(0);
    }
    put(0x1000); // caps: texture
    for _ in 0..4 {
        put(0);
    }
    out.extend_from_slice(blocks);
    out
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A 4 × 4 KTX2 holding one BC1 block of `color` (RGB565), with
    /// `contentHash` set, Zstandard-compressed when `zstd`.
    pub fn bc1(color: u16, content_hash: &str, zstd: bool) -> Vec<u8> {
        let mut block = Vec::new();
        block.extend_from_slice(&color.to_le_bytes());
        block.extend_from_slice(&color.to_le_bytes());
        block.extend_from_slice(&[0; 4]);
        let stored = if zstd {
            zstd::bulk::compress(&block, 3).unwrap()
        } else {
            block.clone()
        };
        let mut kvd = Vec::new();
        let entry = [b"contentHash\0".as_slice(), content_hash.as_bytes(), b"\0"].concat();
        kvd.extend_from_slice(&(entry.len() as u32).to_le_bytes());
        kvd.extend_from_slice(&entry);
        while kvd.len() % 4 != 0 {
            kvd.push(0);
        }
        let kvd_at = 80 + 24;
        let level_at = kvd_at + kvd.len();
        let mut out = MAGIC.to_vec();
        let mut put = |v: u32| out.extend_from_slice(&v.to_le_bytes());
        for v in [131, 1, 4, 4, 0, 0, 1, 1, if zstd { 2 } else { 0 }] {
            put(v);
        }
        for v in [0, 0, kvd_at as u32, kvd.len() as u32] {
            put(v);
        }
        out.extend_from_slice(&0u64.to_le_bytes());
        out.extend_from_slice(&0u64.to_le_bytes());
        out.extend_from_slice(&(level_at as u64).to_le_bytes());
        out.extend_from_slice(&(stored.len() as u64).to_le_bytes());
        out.extend_from_slice(&(block.len() as u64).to_le_bytes());
        out.extend_from_slice(&kvd);
        out.extend_from_slice(&stored);
        out
    }

    #[test]
    fn reads_bc1_levels() {
        for zstd in [false, true] {
            let file = bc1(0xF800, "abc123", zstd);
            let img = decode(&file).unwrap();
            assert_eq!(img.dimensions(), (4, 4));
            assert_eq!(img.get_pixel(1, 2).0, [255, 0, 0, 255]);
        }
    }
}
