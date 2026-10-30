//! A bounds-checked little-endian reader over a mesh blob, shared by mesh
//! files, union geometry and collision hulls: running past the end is a
//! [`MeshError::Damaged`], never a panic.

use super::MeshError;

/// Counts read from a file are checked against this before anything is
/// allocated for them, so a damaged count can't ask for gigabytes.
pub(super) const MAX_ELEMENTS: usize = 1 << 24;

#[derive(Debug, Clone)]
pub(super) struct Reader<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl<'a> Reader<'a> {
    pub(super) fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, at: 0 }
    }

    pub(super) fn is_empty(&self) -> bool {
        self.at >= self.bytes.len()
    }

    pub(super) fn remaining(&self) -> &'a [u8] {
        &self.bytes[self.at.min(self.bytes.len())..]
    }

    pub(super) fn bytes(&mut self, n: usize) -> Result<&'a [u8], MeshError> {
        let end = self
            .at
            .checked_add(n)
            .filter(|&end| end <= self.bytes.len())
            .ok_or_else(|| MeshError::Damaged(format!("cut short at byte {}", self.at)))?;
        let out = &self.bytes[self.at..end];
        self.at = end;
        Ok(out)
    }

    pub(super) fn skip(&mut self, n: usize) -> Result<(), MeshError> {
        self.bytes(n).map(drop)
    }

    pub(super) fn array<const N: usize>(&mut self) -> Result<[u8; N], MeshError> {
        Ok(self.bytes(N)?.try_into().expect("N bytes"))
    }

    pub(super) fn u8(&mut self) -> Result<u8, MeshError> {
        Ok(self.array::<1>()?[0])
    }

    pub(super) fn u16(&mut self) -> Result<u16, MeshError> {
        self.array().map(u16::from_le_bytes)
    }

    pub(super) fn u32(&mut self) -> Result<u32, MeshError> {
        self.array().map(u32::from_le_bytes)
    }

    pub(super) fn i32(&mut self) -> Result<i32, MeshError> {
        self.array().map(i32::from_le_bytes)
    }

    pub(super) fn f32(&mut self) -> Result<f32, MeshError> {
        self.array().map(f32::from_le_bytes)
    }

    /// A count, checked against [`MAX_ELEMENTS`].
    pub(super) fn count(&mut self) -> Result<usize, MeshError> {
        checked_count(self.u32()? as u64)
    }

    /// An unsigned LEB128 varint, as Draco writes counts.
    pub(super) fn varint(&mut self) -> Result<u64, MeshError> {
        let mut out = 0u64;
        for shift in (0..64).step_by(7) {
            let b = self.u8()?;
            out |= u64::from(b & 0x7f) << shift;
            if b < 0x80 {
                return Ok(out);
            }
        }
        Err(MeshError::Damaged("a varint runs past 64 bits".into()))
    }

    /// `n` records of `stride` bytes each.
    pub(super) fn records(
        &mut self,
        n: usize,
        stride: usize,
    ) -> Result<std::slice::ChunksExact<'a, u8>, MeshError> {
        let len = n
            .checked_mul(stride)
            .ok_or_else(|| MeshError::Damaged("a list's size overflows".into()))?;
        Ok(self.bytes(len)?.chunks_exact(stride.max(1)))
    }
}

pub(super) fn checked_count(n: u64) -> Result<usize, MeshError> {
    usize::try_from(n)
        .ok()
        .filter(|&n| n <= MAX_ELEMENTS)
        .ok_or_else(|| MeshError::Damaged(format!("a count of {n} is out of range")))
}

/// Little-endian floats at the start of a record.
pub(super) fn floats<const N: usize>(record: &[u8]) -> [f32; N] {
    std::array::from_fn(|i| f32::from_le_bytes(record[i * 4..][..4].try_into().expect("4 bytes")))
}

/// Little-endian u32s at the start of a record.
pub(super) fn u32s<const N: usize>(record: &[u8]) -> [u32; N] {
    std::array::from_fn(|i| u32::from_le_bytes(record[i * 4..][..4].try_into().expect("4 bytes")))
}
