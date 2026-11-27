//! KTX2 textures, the form Roblox's content delivery serves images in: BC1
//! or BC3 blocks, each mip level Zstandard-compressed (supercompression
//! scheme 2) or stored as is, with key-value data describing the texture
//! (`contentHash`, `RobloxOriginalWidth`, ...).
//!
//! Material textures (`docs/behavior/rendering/materials.md`) are read for
//! their first (largest) level only, decoded to RGBA8 ([`decode`]); Rook
//! builds their smaller levels itself.
//!
//! Images asked for as textures come in mip packs
//! (`docs/behavior/content/asset-fetching.md`): each pack is a KTX2 file of
//! a few levels, `packIndex` naming which, and `pack_<n>` = `first,count`
//! which levels pack `n` holds. [`join`] makes one file of every level out
//! of them, and [`File::chain`] reads that file's blocks as they are, for
//! the GPU to sample.

use std::io::Cursor;

use super::{Encoding, ImageError};

pub(crate) const MAGIC: &[u8] = b"\xABKTX 20\xBB\r\n\x1A\n";

const VK_BC1_RGB_UNORM: u32 = 131;
const VK_BC1_RGB_SRGB: u32 = 132;
const VK_BC1_RGBA_UNORM: u32 = 133;
const VK_BC1_RGBA_SRGB: u32 = 134;
const VK_BC3_UNORM: u32 = 137;
const VK_BC3_SRGB: u32 = 138;
const SUPERCOMPRESSION_NONE: u32 = 0;
const SUPERCOMPRESSION_ZSTD: u32 = 2;

/// Bytes before the level index.
const HEADER: usize = 80;
/// Bytes per level in the level index: offset, length, uncompressed length.
const LEVEL_INDEX: usize = 24;
/// More levels than a 65536-pixel side has is a damaged file.
const MAX_LEVELS: usize = 17;

/// A texture's encoding and every level's blocks, largest first.
pub(crate) type Blocks = (Encoding, Vec<Vec<u8>>);

/// One level's place in the file.
#[derive(Debug, Clone, Copy)]
struct Level {
    offset: usize,
    len: usize,
    uncompressed: usize,
}

/// A KTX2 file's header, levels and key-value data, read without copying.
#[derive(Debug)]
pub(crate) struct File<'a> {
    bytes: &'a [u8],
    vk_format: u32,
    pub(crate) width: u32,
    pub(crate) height: u32,
    supercompression: u32,
    /// Level 0 (the largest) first.
    levels: Vec<Level>,
    /// The data format descriptor, copied as is into a joined file.
    dfd: &'a [u8],
    /// Each key and its value, the value without its closing NULs.
    kv: Vec<(&'a [u8], &'a [u8])>,
}

fn u32_at(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(at..at + 4)?.try_into().ok()?))
}

fn u64_at(b: &[u8], at: usize) -> Option<usize> {
    usize::try_from(u64::from_le_bytes(b.get(at..at + 8)?.try_into().ok()?)).ok()
}

fn damaged(why: &str) -> ImageError {
    ImageError::Damaged("KTX2", why.to_owned())
}

impl<'a> File<'a> {
    pub(crate) fn read(bytes: &'a [u8]) -> Result<Self, ImageError> {
        if !bytes.starts_with(MAGIC) {
            return Err(damaged("bad header"));
        }
        let field = |at| u32_at(bytes, at).ok_or_else(|| damaged("bad header"));
        let count = (field(40)? as usize).max(1);
        if count > MAX_LEVELS {
            return Err(damaged("too many levels"));
        }
        let levels = (0..count)
            .map(|i| {
                let at = HEADER + i * LEVEL_INDEX;
                let level = Level {
                    offset: u64_at(bytes, at)?,
                    len: u64_at(bytes, at + 8)?,
                    uncompressed: u64_at(bytes, at + 16)?,
                };
                (level.offset.checked_add(level.len)? <= bytes.len()).then_some(level)
            })
            .collect::<Option<Vec<_>>>()
            .ok_or_else(|| damaged("a level past the end"))?;
        let slice = |at: usize, len: usize| bytes.get(at..at.checked_add(len)?);
        let dfd = slice(field(48)? as usize, field(52)? as usize)
            .ok_or_else(|| damaged("format descriptor past the end"))?;
        let kvd = slice(field(56)? as usize, field(60)? as usize)
            .ok_or_else(|| damaged("key-value data past the end"))?;
        Ok(Self {
            bytes,
            vk_format: field(12)?,
            width: field(20)?,
            height: field(24)?.max(1),
            supercompression: field(44)?,
            levels,
            dfd,
            kv: key_values(kvd),
        })
    }

    /// A key's value as text.
    pub(crate) fn text(&self, key: &str) -> Option<&'a str> {
        let (_, value) = self.kv.iter().find(|(k, _)| *k == key.as_bytes())?;
        std::str::from_utf8(value).ok()
    }

    /// A key's value as a whole number.
    pub(crate) fn number(&self, key: &str) -> Option<u32> {
        self.text(key)?.trim().parse().ok()
    }

    /// Which pack of its texture this file is (`packIndex`), if it's one.
    pub(crate) fn pack_index(&self) -> Option<u32> {
        self.number("packIndex")
    }

    /// The levels each of the texture's packs holds, `(first, count)` in
    /// pack order (`pack_0`, `pack_1`, ...); empty when it isn't in packs.
    pub(crate) fn packs(&self) -> Vec<(u32, u32)> {
        (0..)
            .map_while(|n| {
                let (first, count) = self.text(&format!("pack_{n}"))?.split_once(',')?;
                Some((first.trim().parse().ok()?, count.trim().parse().ok()?))
            })
            .collect()
    }

    /// How many packs past pack 0 make the texture at least `side` pixels
    /// across (its largest level's longer side), or all of them: what to
    /// fetch for a texture that size on screen. 0 when it isn't in packs.
    pub(crate) fn packs_up_to(&self, side: u32) -> u8 {
        let (width, height) = self.texture_size();
        let packs = self.packs();
        let last = packs.len().saturating_sub(1);
        packs
            .iter()
            .position(|&(first, _)| (width.max(height) >> first.min(31)) >= side)
            .unwrap_or(last) as u8
    }

    /// The whole texture's size when this file is one of its packs
    /// (`transcodedWidth`, `transcodedHeight`), else the file's own.
    fn texture_size(&self) -> (u32, u32) {
        match (
            self.number("transcodedWidth"),
            self.number("transcodedHeight"),
        ) {
            (Some(w), Some(h)) if w > 0 && h > 0 => (w, h),
            _ => (self.width, self.height),
        }
    }

    /// The size Roblox reports for the image: its upload's
    /// (`RobloxOriginalWidth`, `RobloxOriginalHeight`), which the
    /// transcode may have rounded or shrunk.
    pub(crate) fn original_size(&self) -> Option<(u32, u32)> {
        match (
            self.number("RobloxOriginalWidth"),
            self.number("RobloxOriginalHeight"),
        ) {
            (Some(w), Some(h)) if w > 0 && h > 0 => Some((w, h)),
            _ => None,
        }
    }

    fn encoding(&self) -> Option<Encoding> {
        match self.vk_format {
            VK_BC1_RGB_UNORM | VK_BC1_RGB_SRGB | VK_BC1_RGBA_UNORM | VK_BC1_RGBA_SRGB => {
                Some(Encoding::Bc1)
            }
            VK_BC3_UNORM | VK_BC3_SRGB => Some(Encoding::Bc3),
            _ => None,
        }
    }

    /// Level `i` as stored, and its length once decompressed.
    fn stored(&self, i: usize) -> (&'a [u8], usize) {
        let l = self.levels[i];
        (&self.bytes[l.offset..l.offset + l.len], l.uncompressed)
    }

    /// Level `i`'s blocks.
    fn level(&self, i: usize) -> Result<Vec<u8>, ImageError> {
        let (stored, uncompressed) = self.stored(i);
        match self.supercompression {
            SUPERCOMPRESSION_NONE => Ok(stored.to_vec()),
            SUPERCOMPRESSION_ZSTD => zstd::bulk::decompress(stored, uncompressed)
                .map_err(|e| damaged(&format!("zstd: {e}"))),
            other => Err(damaged(&format!("supercompression {other}"))),
        }
    }

    /// Every level's blocks, when the file holds a whole chain (down to
    /// 1 × 1) of BC1 or BC3 whose sides are multiples of 4, the blocks the
    /// GPU samples; `None` for anything else, which is read through
    /// [`decode`] instead.
    pub(crate) fn chain(&self) -> Result<Option<Blocks>, ImageError> {
        let Some(encoding) = self.encoding() else {
            return Ok(None);
        };
        let whole = self.levels.len() == super::level_count(self.width, self.height);
        if !whole || !self.width.is_multiple_of(4) || !self.height.is_multiple_of(4) {
            return Ok(None);
        }
        let block = encoding.block_bytes().expect("in blocks");
        let mips = (0..self.levels.len())
            .map(|i| {
                let (w, h) = ((self.width >> i).max(1), (self.height >> i).max(1));
                let level = self.level(i)?;
                let expected = w.div_ceil(4) as usize * h.div_ceil(4) as usize * block;
                if level.len() != expected {
                    return Err(damaged(&format!(
                        "level {i} is {} bytes, not {expected}",
                        level.len()
                    )));
                }
                Ok(level)
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Some((encoding, mips)))
    }
}

/// The key-value data's entries: each a length, `key NUL value`, padded
/// to 4 bytes.
fn key_values(kvd: &[u8]) -> Vec<(&[u8], &[u8])> {
    let mut out = Vec::new();
    let mut at = 0;
    while let Some(len) = u32_at(kvd, at) {
        let Some(entry) = kvd.get(at + 4..at + 4 + len as usize) else {
            break;
        };
        if let Some(nul) = entry.iter().position(|&b| b == 0) {
            let value = &entry[nul + 1..];
            let end = value.iter().rposition(|&b| b != 0).map_or(0, |i| i + 1);
            out.push((&entry[..nul], &value[..end]));
        }
        at = (at + 4 + len as usize).next_multiple_of(4);
    }
    out
}

/// One file of a texture's levels from some of its packs (in any order):
/// each pack's levels as they are stored, the first pack's format
/// descriptor, and its key-value data without the pack keys. The packs
/// must be one texture's, between them holding each level from their
/// largest down to 1 × 1 once: pack 0 alone, or pack 0 to pack `n`, which
/// make the texture at the size pack `n` starts at.
pub(crate) fn join(packs: &[&[u8]]) -> Result<Vec<u8>, ImageError> {
    let files = packs
        .iter()
        .map(|p| File::read(p))
        .collect::<Result<Vec<_>, _>>()?;
    let first = files.first().ok_or_else(|| damaged("no packs"))?;
    let (width, height) = first.texture_size();
    let count = super::level_count(width, height);
    let layout = first.packs();
    let mut levels: Vec<Option<(&[u8], usize)>> = vec![None; count];
    for file in &files {
        let same = file.vk_format == first.vk_format
            && file.supercompression == first.supercompression
            && file.texture_size() == (width, height);
        if !same {
            return Err(damaged("packs of different textures"));
        }
        let index = file.pack_index().ok_or_else(|| damaged("not a pack"))? as usize;
        let &(start, n) = layout
            .get(index)
            .ok_or_else(|| damaged("a pack the first doesn't list"))?;
        let start = start as usize;
        let fits = n as usize == file.levels.len()
            && start + file.levels.len() <= count
            && file.width == (width >> start).max(1)
            && file.height == (height >> start).max(1);
        if !fits {
            return Err(damaged("a pack's levels don't match its place"));
        }
        for i in 0..file.levels.len() {
            if levels[start + i].replace(file.stored(i)).is_some() {
                return Err(damaged("a level in two packs"));
            }
        }
    }
    let top = levels
        .iter()
        .position(Option::is_some)
        .ok_or_else(|| damaged("no levels"))?;
    let levels = levels[top..]
        .iter()
        .copied()
        .collect::<Option<Vec<_>>>()
        .ok_or_else(|| damaged("a level no pack holds"))?;
    let count = levels.len();
    let (width, height) = ((width >> top).max(1), (height >> top).max(1));

    let mut kvd = Vec::new();
    for (key, value) in &first.kv {
        if *key == b"packIndex" || key.starts_with(b"pack_") {
            continue;
        }
        let entry = [key, &b"\0"[..], value, &b"\0"[..]].concat();
        kvd.extend_from_slice(&(entry.len() as u32).to_le_bytes());
        kvd.extend_from_slice(&entry);
        kvd.resize(kvd.len().next_multiple_of(4), 0);
    }
    let dfd_at = HEADER + count * LEVEL_INDEX;
    let kvd_at = dfd_at + first.dfd.len();
    let data_at = kvd_at + kvd.len();

    let mut out = MAGIC.to_vec();
    let mut put = |v: u32| out.extend_from_slice(&v.to_le_bytes());
    let header = [
        first.vk_format,
        1, // type size: blocks
        width,
        height,
        0, // depth
        0, // layers
        1, // faces
        count as u32,
        first.supercompression,
        dfd_at as u32,
        first.dfd.len() as u32,
        kvd_at as u32,
        kvd.len() as u32,
    ];
    for v in header {
        put(v);
    }
    out.extend_from_slice(&[0; 16]); // no supercompression global data
    // The smallest level first, as Roblox's packs keep them.
    let mut at = data_at;
    let mut index = vec![[0u64; 3]; count];
    for i in (0..count).rev() {
        let (stored, uncompressed) = levels[i];
        index[i] = [at as u64, stored.len() as u64, uncompressed as u64];
        at += stored.len();
    }
    for entry in index {
        for v in entry {
            out.extend_from_slice(&v.to_le_bytes());
        }
    }
    out.extend_from_slice(first.dfd);
    out.extend_from_slice(&kvd);
    for (stored, _) in levels.iter().rev() {
        out.extend_from_slice(stored);
    }
    Ok(out)
}

/// The first level as RGBA8 (as stored: no color conversion).
pub(crate) fn decode(b: &[u8]) -> Result<::image::RgbaImage, ImageError> {
    let file = File::read(b)?;
    let fourcc: &[u8; 4] = match file.encoding() {
        Some(Encoding::Bc1) => b"DXT1",
        Some(Encoding::Bc3) => b"DXT5",
        _ => {
            return Err(damaged(&format!(
                "format {} isn't BC1 or BC3",
                file.vk_format
            )));
        }
    };
    let blocks = file.level(0)?;
    let dds = dds_file(fourcc, file.width, file.height, &blocks);
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

    /// A KTX2 file of `levels` (largest first) in `vk_format`, each
    /// Zstandard-compressed when `zstd`, with these key-value entries.
    pub(crate) fn file(
        vk_format: u32,
        (width, height): (u32, u32),
        levels: &[Vec<u8>],
        kv: &[(&str, &str)],
        zstd: bool,
    ) -> Vec<u8> {
        let stored: Vec<Vec<u8>> = levels
            .iter()
            .map(|l| {
                if zstd {
                    zstd::bulk::compress(l, 3).unwrap()
                } else {
                    l.clone()
                }
            })
            .collect();
        let mut kvd = Vec::new();
        for (k, v) in kv {
            let entry = [k.as_bytes(), b"\0", v.as_bytes(), b"\0"].concat();
            kvd.extend_from_slice(&(entry.len() as u32).to_le_bytes());
            kvd.extend_from_slice(&entry);
            kvd.resize(kvd.len().next_multiple_of(4), 0);
        }
        let kvd_at = HEADER + levels.len() * LEVEL_INDEX;
        let mut at = kvd_at + kvd.len();
        let mut out = MAGIC.to_vec();
        let mut put = |v: u32| out.extend_from_slice(&v.to_le_bytes());
        for v in [
            vk_format,
            1,
            width,
            height,
            0,
            0,
            1,
            levels.len() as u32,
            if zstd { 2 } else { 0 },
            kvd_at as u32,
            0,
            kvd_at as u32,
            kvd.len() as u32,
        ] {
            put(v);
        }
        out.extend_from_slice(&[0; 16]);
        for (s, l) in stored.iter().zip(levels) {
            for v in [at, s.len(), l.len()] {
                out.extend_from_slice(&(v as u64).to_le_bytes());
            }
            at += s.len();
        }
        out.extend_from_slice(&kvd);
        for s in &stored {
            out.extend_from_slice(s);
        }
        out
    }

    /// One BC1 block of `color` (RGB565) in 4-color mode.
    pub(crate) fn bc1_block(color: u16) -> Vec<u8> {
        [color.to_le_bytes(), color.to_le_bytes(), [0; 2], [0; 2]].concat()
    }

    /// A 4 × 4 KTX2 holding one BC1 block of `color` (RGB565), with
    /// `contentHash` set, Zstandard-compressed when `zstd`.
    pub(crate) fn bc1(color: u16, content_hash: &str, zstd: bool) -> Vec<u8> {
        file(
            VK_BC1_RGB_UNORM,
            (4, 4),
            &[bc1_block(color)],
            &[("contentHash", content_hash)],
            zstd,
        )
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

    /// Level `i` of a 16 × 16 BC1 texture: flat, its color telling the
    /// levels apart.
    fn level16(i: usize) -> Vec<u8> {
        let blocks = (16usize >> i).div_ceil(4).pow(2);
        bc1_block(0x0841 * (i as u16 + 1)).repeat(blocks)
    }

    /// The 16 × 16 texture as Roblox sends it: pack 0 the levels from 4 × 4
    /// down, pack 1 the 16 and 8.
    fn packs16() -> [Vec<u8>; 2] {
        let kv = |index: &'static str| {
            vec![
                ("RobloxOriginalHeight", "20"),
                ("RobloxOriginalWidth", "20"),
                ("packIndex", index),
                ("pack_0", "2,3"),
                ("pack_1", "0,2"),
                ("transcodedHeight", "16"),
                ("transcodedWidth", "16"),
            ]
        };
        [
            file(
                VK_BC1_RGB_UNORM,
                (4, 4),
                &[level16(2), level16(3), level16(4)],
                &kv("0"),
                true,
            ),
            file(
                VK_BC1_RGB_UNORM,
                (16, 16),
                &[level16(0), level16(1)],
                &kv("1"),
                true,
            ),
        ]
    }

    #[test]
    fn reads_pack_layout() {
        let [tail, _] = packs16();
        let file = File::read(&tail).unwrap();
        assert_eq!(file.pack_index(), Some(0));
        assert_eq!(file.packs(), [(2, 3), (0, 2)]);
        assert_eq!(file.texture_size(), (16, 16));
        assert_eq!(file.original_size(), Some((20, 20)));
        // Pack 0 alone is the texture's smallest levels, a whole chain.
        let (_, mips) = file.chain().unwrap().unwrap();
        assert_eq!(mips, (2..5).map(level16).collect::<Vec<_>>());
    }

    /// Joined in either order, the packs make one file of every level,
    /// without the pack keys.
    #[test]
    fn joins_packs_into_one_chain() {
        let [tail, top] = packs16();
        for order in [[&tail, &top], [&top, &tail]] {
            let joined = join(&order.map(Vec::as_slice)).unwrap();
            let file = File::read(&joined).unwrap();
            assert_eq!((file.width, file.height), (16, 16));
            assert_eq!(file.pack_index(), None);
            assert!(file.packs().is_empty());
            assert_eq!(file.original_size(), Some((20, 20)));
            let (encoding, mips) = file.chain().unwrap().unwrap();
            assert_eq!(encoding, Encoding::Bc1);
            assert_eq!(mips, (0..5).map(level16).collect::<Vec<_>>());
        }
    }

    /// The packs a texture needs to be a size: pack 0 up to the first that
    /// reaches it, or all of them.
    #[test]
    fn picks_the_packs_for_a_size() {
        let [tail, _] = packs16();
        let file = File::read(&tail).unwrap();
        let picks: Vec<u8> = [1, 4, 5, 16, 64].map(|s| file.packs_up_to(s)).to_vec();
        assert_eq!(picks, [0, 0, 1, 1, 1]);
        assert_eq!(File::read(&bc1(0, "x", false)).unwrap().packs_up_to(64), 0);
    }

    /// Pack 0 alone joins into the texture's smaller version.
    #[test]
    fn joins_the_smallest_pack_alone() {
        let [tail, _] = packs16();
        let joined = join(&[&tail]).unwrap();
        let file = File::read(&joined).unwrap();
        assert_eq!((file.width, file.height), (4, 4));
        assert_eq!(file.original_size(), Some((20, 20)));
        let (_, mips) = file.chain().unwrap().unwrap();
        assert_eq!(mips, (2..5).map(level16).collect::<Vec<_>>());
    }

    #[test]
    fn refuses_incomplete_or_mixed_packs() {
        let [tail, top] = packs16();
        assert!(join(&[&top]).is_err(), "missing the smallest levels");
        assert!(join(&[&tail, &tail]).is_err(), "a level twice");
        let other = bc1(0, "x", false);
        assert!(join(&[&tail, &other]).is_err());
        assert!(join(&[&top, &other]).is_err());
    }
}
