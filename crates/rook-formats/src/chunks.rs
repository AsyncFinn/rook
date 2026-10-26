//! The chunks of a binary file, for the passes that read past
//! `rbx_binary`: a 32-byte header, then chunks each with a 16-byte header
//! (four-byte name, compressed size, size, reserved) and their bytes,
//! LZ4 or zstd compressed unless the compressed size is 0, up to `END`.

use crate::LoadError;

/// The file header's length.
pub(crate) const HEADER: usize = 32;

/// One chunk as stored.
pub(crate) struct Chunk<'a> {
    pub(crate) name: &'a [u8],
    /// The whole chunk, its header included, as stored.
    pub(crate) stored: &'a [u8],
    compressed: usize,
    size: usize,
}

impl Chunk<'_> {
    /// The chunk's bytes, decompressed.
    pub(crate) fn data(&self) -> Result<Vec<u8>, LoadError> {
        let raw = &self.stored[16..];
        if self.compressed == 0 {
            return Ok(raw.to_vec());
        }
        let out = if raw.starts_with(&[0x28, 0xB5, 0x2F, 0xFD]) {
            zstd::bulk::decompress(raw, self.size).ok()
        } else {
            lz4_flex::block::decompress(raw, self.size).ok()
        };
        out.ok_or_else(|| bad("bad compression"))
    }
}

/// A binary file's chunks in order, `END` last when the file has one.
pub(crate) fn chunks(bytes: &[u8]) -> Result<Vec<Chunk<'_>>, LoadError> {
    if !bytes.starts_with(b"<roblox!") || bytes.len() < HEADER {
        return Err(LoadError::UnknownFormat);
    }
    let mut out = Vec::new();
    let mut pos = HEADER;
    while pos + 16 <= bytes.len() {
        let compressed = read_u32(bytes, pos + 4) as usize;
        let size = read_u32(bytes, pos + 8) as usize;
        let end = pos + 16 + if compressed == 0 { size } else { compressed };
        let stored = bytes.get(pos..end).ok_or_else(|| bad("truncated chunk"))?;
        let name = &stored[..4];
        out.push(Chunk {
            name,
            stored,
            compressed,
            size,
        });
        pos = end;
        if name == b"END\0" {
            break;
        }
    }
    Ok(out)
}

/// A chunk named `name` holding `data`, stored uncompressed.
pub(crate) fn write_chunk(out: &mut Vec<u8>, name: &[u8], data: &[u8]) {
    out.extend_from_slice(name);
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&(data.len() as u32).to_le_bytes());
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(data);
}

fn read_u32(bytes: &[u8], at: usize) -> u32 {
    bytes
        .get(at..at + 4)
        .map_or(0, |b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
}

fn bad(what: &str) -> LoadError {
    LoadError::Binary(format!("binary chunks: {what}"))
}

/// Reads a chunk's bytes from the start.
pub(crate) struct Reader<'a> {
    pub(crate) bytes: &'a [u8],
    pub(crate) at: usize,
}

impl<'a> Reader<'a> {
    pub(crate) fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, at: 0 }
    }

    pub(crate) fn take(&mut self, n: usize) -> Result<&'a [u8], LoadError> {
        let out = (self.bytes)
            .get(self.at..self.at + n)
            .ok_or_else(|| bad("truncated chunk"))?;
        self.at += n;
        Ok(out)
    }

    pub(crate) fn u8(&mut self) -> Result<u8, LoadError> {
        Ok(self.take(1)?[0])
    }

    pub(crate) fn u32(&mut self) -> Result<u32, LoadError> {
        let b = self.take(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    pub(crate) fn string(&mut self) -> Result<&'a [u8], LoadError> {
        let n = self.u32()? as usize;
        self.take(n)
    }

    /// The rest of the chunk.
    pub(crate) fn rest(&mut self) -> &'a [u8] {
        let out = &self.bytes[self.at.min(self.bytes.len())..];
        self.at = self.bytes.len();
        out
    }

    /// `count` referents: big-endian i32s split into four byte planes,
    /// zigzag-encoded, each the difference from the one before.
    pub(crate) fn referents(&mut self, count: usize) -> Result<Vec<i32>, LoadError> {
        let mut last = 0i32;
        let values = interleaved(self.take(count * 4)?);
        Ok(values
            .map(|v| {
                last = last.wrapping_add(unzigzag(v));
                last
            })
            .collect())
    }
}

/// Big-endian u32s split into four byte planes, as most arrays are stored.
pub(crate) fn interleaved(planes: &[u8]) -> impl Iterator<Item = u32> + '_ {
    let count = planes.len() / 4;
    (0..count).map(move |i| {
        u32::from_be_bytes([
            planes[i],
            planes[count + i],
            planes[2 * count + i],
            planes[3 * count + i],
        ])
    })
}

/// `values` as [`interleaved`] stores them.
pub(crate) fn interleave(values: &[u32]) -> Vec<u8> {
    let count = values.len();
    let mut out = vec![0; count * 4];
    for (i, v) in values.iter().enumerate() {
        for (plane, byte) in v.to_be_bytes().into_iter().enumerate() {
            out[plane * count + i] = byte;
        }
    }
    out
}

pub(crate) fn unzigzag(v: u32) -> i32 {
    ((v >> 1) as i32) ^ -((v & 1) as i32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn referents_undo_zigzag_and_deltas() {
        // 1, then +2, then -1: zigzag 2, 4, 1, as four byte planes.
        let planes = [0, 0, 0, 0, 0, 0, 0, 0, 0, 2, 4, 1];
        assert_eq!(Reader::new(&planes).referents(3).unwrap(), vec![1, 3, 2]);
    }

    #[test]
    fn interleave_round_trips() {
        let values = [1, 0x0102_0304, u32::MAX];
        assert_eq!(
            interleaved(&interleave(&values)).collect::<Vec<_>>(),
            values
        );
    }
}
