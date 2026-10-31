//! Draco's entropy coding of unsigned symbols: rANS over a probability
//! table, either for every symbol (raw) or for each value's bit length,
//! the bits following plainly (tagged).

use super::super::MeshError;
use super::super::reader::{Reader, checked_count};

/// `n` symbols, in groups of `components` sharing one bit length when
/// tagged.
pub(super) fn symbols(r: &mut Reader, n: usize, components: usize) -> Result<Vec<u32>, MeshError> {
    if n == 0 {
        return Ok(Vec::new());
    }
    let mut out = Vec::with_capacity(n);
    match r.u8()? {
        0 => {
            let mut tags = Decoder::new(r, 5)?;
            let mut bits = Bits::new(r.remaining());
            while out.len() < n {
                let length = tags.read();
                for _ in 0..components.min(n - out.len()) {
                    out.push(bits.read(length)?);
                }
            }
            r.skip(bits.bytes_read())?;
        }
        1 => {
            let length = r.u8()?;
            if !(1..=18).contains(&length) {
                return Err(MeshError::Damaged(format!("{length}-bit Draco symbols")));
            }
            let mut decoder = Decoder::new(r, length)?;
            for _ in 0..n {
                out.push(decoder.read());
            }
        }
        s => return Err(MeshError::Damaged(format!("Draco symbol scheme {s}"))),
    }
    Ok(out)
}

/// A rANS decoder over its symbols' probability table.
struct Decoder<'a> {
    precision: u32,
    /// Each symbol's probability and where its range starts.
    table: Vec<(u32, u32)>,
    /// The symbol owning each slot of the precision.
    lookup: Vec<u32>,
    data: &'a [u8],
    at: usize,
    state: u32,
}

impl<'a> Decoder<'a> {
    /// The table, then the coded bytes, which are read from their end.
    /// `length` is the bit length of the largest symbol, which sets the
    /// precision.
    fn new(r: &mut Reader<'a>, length: u8) -> Result<Self, MeshError> {
        let damaged = |what: &str| MeshError::Damaged(format!("Draco rANS {what}"));
        let precision_bits = (3 * u32::from(length) / 2).clamp(12, 20);
        let precision = 1u32 << precision_bits;
        let count = checked_count(r.varint()?)?;
        if count / 64 > r.remaining().len() {
            return Err(damaged("table is cut short"));
        }
        let mut table = Vec::with_capacity(count);
        let mut lookup = Vec::with_capacity(precision as usize);
        while table.len() < count {
            let b = r.u8()?;
            let token = b & 3;
            if token == 3 {
                let zeros = usize::from(b >> 2) + 1;
                if table.len() + zeros > count {
                    return Err(damaged("table runs past its count"));
                }
                table.extend(std::iter::repeat_n((0, lookup.len() as u32), zeros));
                continue;
            }
            let mut p = u32::from(b >> 2);
            for i in 0..u32::from(token) {
                p |= u32::from(r.u8()?) << (8 * (i + 1) - 2);
            }
            let start = lookup.len() as u32;
            if start + p > precision {
                return Err(damaged("probabilities pass the precision"));
            }
            lookup.extend(std::iter::repeat_n(table.len() as u32, p as usize));
            table.push((p, start));
        }
        if lookup.len() as u32 != precision {
            return Err(damaged("probabilities don't add up"));
        }
        let size = usize::try_from(r.varint()?).map_err(|_| damaged("size is out of range"))?;
        let data = r.bytes(size)?;
        let last = *data.last().ok_or_else(|| damaged("data is empty"))?;
        let width = usize::from(last >> 6) + 1;
        let at = size
            .checked_sub(width)
            .ok_or_else(|| damaged("data is cut short"))?;
        let head = data[at..]
            .iter()
            .rev()
            .fold(0u32, |acc, &b| acc << 8 | u32::from(b));
        let state = (head & ((1 << (8 * width - 2)) - 1)) + 4 * precision;
        if state >= 4 * precision * 256 {
            return Err(damaged("state is out of range"));
        }
        Ok(Self {
            precision,
            table,
            lookup,
            data,
            at,
            state,
        })
    }

    fn read(&mut self) -> u32 {
        let low = 4 * self.precision;
        while self.state < low && self.at > 0 {
            self.at -= 1;
            self.state = self.state << 8 | u32::from(self.data[self.at]);
        }
        let (quotient, slot) = (self.state / self.precision, self.state % self.precision);
        let symbol = self.lookup[slot as usize];
        let (p, start) = self.table[symbol as usize];
        self.state = quotient * p + slot - start;
        symbol
    }
}

/// Plain bits, least significant first within each byte.
struct Bits<'a> {
    data: &'a [u8],
    bit: usize,
}

impl<'a> Bits<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, bit: 0 }
    }

    fn read(&mut self, length: u32) -> Result<u32, MeshError> {
        if length > 32 || self.bit + length as usize > self.data.len() * 8 {
            return Err(MeshError::Damaged("Draco bits are cut short".into()));
        }
        let mut out = 0u32;
        for i in 0..length {
            let b = self.data[self.bit >> 3] >> (self.bit & 7) & 1;
            out |= u32::from(b) << i;
            self.bit += 1;
        }
        Ok(out)
    }

    fn bytes_read(&self) -> usize {
        self.bit.div_ceil(8)
    }
}
