//! Reading and writing `Terrain.SmoothGrid`: a version byte and the chunk
//! size, then chunks, each a position offset and its 32,768 voxels in runs.
//! The byte layout is in `docs/behavior/terrain.md`.

use glam::IVec3;

use crate::TerrainMaterial;
use crate::grid::{CHUNK_VOXELS, Grid, Voxel};

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DecodeError {
    #[error("not a SmoothGrid (header {0:02x?})")]
    Header(Vec<u8>),
    #[error("the data ends inside chunk {0}")]
    Truncated(usize),
    #[error("unknown terrain material {0}")]
    Material(u8),
    #[error("a run goes past the end of chunk {0}")]
    Overrun(usize),
}

const VERSION: u8 = 1;
const CHUNK_SIZE_LOG2: u8 = 5;
const CHUNK_LEN: usize = (CHUNK_VOXELS * CHUNK_VOXELS * CHUNK_VOXELS) as usize;

const HAS_OCCUPANCY: u8 = 0x40;
const HAS_COUNT: u8 = 0x80;

struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl Reader<'_> {
    fn byte(&mut self) -> Option<u8> {
        let b = *self.bytes.get(self.at)?;
        self.at += 1;
        Some(b)
    }

    fn take(&mut self, n: usize) -> Option<&[u8]> {
        let out = self.bytes.get(self.at..self.at + n)?;
        self.at += n;
        Some(out)
    }
}

impl Grid {
    /// Decode a `SmoothGrid`. An empty value is an empty grid.
    pub fn decode(bytes: &[u8]) -> Result<Grid, DecodeError> {
        let mut grid = Grid::new();
        if bytes.is_empty() {
            return Ok(grid);
        }
        if bytes.len() < 2 || bytes[0] != VERSION || bytes[1] != CHUNK_SIZE_LOG2 {
            return Err(DecodeError::Header(bytes[..bytes.len().min(2)].to_vec()));
        }
        let mut r = Reader { bytes, at: 2 };
        let mut position = IVec3::ZERO;
        let mut number = 0;
        while r.at < bytes.len() {
            let offset = r.take(12).ok_or(DecodeError::Truncated(number))?;
            let axis = |i: usize| {
                i32::from_be_bytes([offset[i], offset[3 + i], offset[6 + i], offset[9 + i]])
            };
            position += IVec3::new(axis(0), axis(1), axis(2));
            let mut voxels = Vec::with_capacity(CHUNK_LEN);
            while voxels.len() < CHUNK_LEN {
                let flag = r.byte().ok_or(DecodeError::Truncated(number))?;
                let index = flag & 0x3f;
                let material =
                    TerrainMaterial::from_index(index).ok_or(DecodeError::Material(index))?;
                let occupancy = if flag & HAS_OCCUPANCY != 0 {
                    r.byte().ok_or(DecodeError::Truncated(number))?
                } else {
                    255
                };
                let mut voxel = Voxel::new(material, occupancy);
                let mut count = 1;
                if flag & HAS_COUNT != 0 {
                    match r.byte().ok_or(DecodeError::Truncated(number))? {
                        0 => {
                            let water = r.byte().ok_or(DecodeError::Truncated(number))?;
                            if voxel.is_solid() {
                                voxel.water = water;
                            }
                        }
                        n => count += usize::from(n),
                    }
                }
                if voxels.len() + count > CHUNK_LEN {
                    return Err(DecodeError::Overrun(number));
                }
                voxels.extend(std::iter::repeat_n(voxel, count));
            }
            grid.insert_chunk(position, voxels);
            number += 1;
        }
        Ok(grid)
    }

    /// Encode the grid as a `SmoothGrid`, in runs of up to 256 equal
    /// voxels as Studio writes them. An empty grid is empty.
    pub fn encode(&self) -> Vec<u8> {
        let chunks = self.stored();
        if chunks.is_empty() {
            return Vec::new();
        }
        let mut out = vec![VERSION, CHUNK_SIZE_LOG2];
        let mut last = IVec3::ZERO;
        for (at, voxels) in chunks {
            let d = (at - last).to_array().map(i32::to_be_bytes);
            for byte in 0..4 {
                out.extend(d.iter().map(|axis| axis[byte]));
            }
            last = at;
            let mut i = 0;
            while i < voxels.len() {
                let v = voxels[i];
                let mut run = 1;
                while v.water == 0 && run < 256 && i + run < voxels.len() && voxels[i + run] == v {
                    run += 1;
                }
                let mut flag = v.material as u8;
                let partial = v.occupancy != 255 && v.material != TerrainMaterial::Air;
                if partial {
                    flag |= HAS_OCCUPANCY;
                }
                if run > 1 || v.water != 0 {
                    flag |= HAS_COUNT;
                }
                out.push(flag);
                if partial {
                    out.push(v.occupancy);
                }
                if v.water != 0 {
                    out.extend([0, v.water]);
                } else if run > 1 {
                    out.push((run - 1) as u8);
                }
                i += run;
            }
        }
        out
    }
}
