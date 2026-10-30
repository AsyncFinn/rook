//! Edgebreaker connectivity (Rossignac's CLERS codes, decoded by Wrap and
//! Zip) for `CSGPHS` 8's convex hulls. Each hull starts from a triangle of
//! vertices 0, 1 and 2 and grows one triangle per code across its gate
//! edge: C brings a new vertex, L and R close the triangle against the
//! boundary on one side, S splits the boundary in two (the second half
//! continues once the first ends) and E closes both sides. Edges left
//! open by L, R and E are zipped to their neighbors, which also settles
//! which vertex each corner is. Every walk is bounded by the edge count,
//! so a damaged blob is an error, never a hang.

use super::MeshError;

/// A face's three edges are `3f`, `3f + 1` and `3f + 2`; edge `e` starts
/// at corner `e`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Twin {
    /// Not reached yet: the gate a later triangle attaches to.
    Open,
    /// On the hull's boundary until zipped.
    Border,
    /// Closed by L, R or E and waiting to be zipped.
    Zipping,
    Edge(u32),
}

fn next(e: u32) -> u32 {
    e - e % 3 + (e + 1) % 3
}

fn prev(e: u32) -> u32 {
    e - e % 3 + (e + 2) % 3
}

/// Roblox's bit order: 32-bit little-endian words read from their top
/// bit, only the low `bits` left in the stream counting in the last one.
pub(super) struct Bits<'a> {
    words: std::slice::Iter<'a, [u8; 4]>,
    left: u32,
    word: u32,
    have: u32,
}

impl<'a> Bits<'a> {
    pub(super) fn new(bytes: &'a [u8], bits: u32) -> Result<Self, MeshError> {
        if !bytes.len().is_multiple_of(4) || (bytes.len() as u64) * 8 < u64::from(bits) {
            return Err(MeshError::Damaged("hull codes are cut short".into()));
        }
        Ok(Self {
            words: bytes.as_chunks().0.iter(),
            left: bits,
            word: 0,
            have: 0,
        })
    }

    fn read(&mut self, mut n: u32) -> Result<u32, MeshError> {
        let mut out = 0u32;
        while n > 0 {
            if self.have == 0 {
                let word = self.words.next().filter(|_| self.left > 0);
                let word = word.ok_or_else(|| MeshError::Damaged("hull codes run out".into()))?;
                self.word = u32::from_le_bytes(*word);
                self.have = self.left.min(32);
                self.left -= self.have;
            }
            let k = n.min(self.have);
            let bits = (u64::from(self.word) >> (self.have - k)) & ((1u64 << k) - 1);
            out = ((u64::from(out) << k) | bits) as u32;
            self.have -= k;
            n -= k;
        }
        Ok(out)
    }

    /// The next code: 0 is C, then two bits for S, L, R or E.
    fn code(&mut self) -> Result<Code, MeshError> {
        if self.read(1)? == 0 {
            return Ok(Code::C);
        }
        Ok(match self.read(2)? {
            0 => Code::S,
            1 => Code::L,
            2 => Code::R,
            _ => Code::E,
        })
    }
}

enum Code {
    C,
    L,
    E,
    R,
    S,
}

/// Decodes hulls one after another into one shared triangle list.
pub(super) struct Decoder<'a> {
    bits: Bits<'a>,
    twin: Vec<Twin>,
    /// Each corner's vertex, numbered within its hull.
    corner: Vec<u32>,
    faces: u32,
}

impl<'a> Decoder<'a> {
    /// `faces` is the triangle count of every hull together.
    pub(super) fn new(bits: Bits<'a>, faces: usize) -> Self {
        Self {
            bits,
            twin: vec![Twin::Open; faces * 3],
            corner: vec![0; faces * 3],
            faces: 0,
        }
    }

    /// Every hull's corners so far, three per triangle.
    pub(super) fn into_corners(mut self) -> Vec<u32> {
        self.corner.truncate(self.faces as usize * 3);
        self.corner
    }

    pub(super) fn faces(&self) -> u32 {
        self.faces
    }

    fn face(&mut self) -> Result<u32, MeshError> {
        if (self.faces as usize + 1) * 3 > self.twin.len() {
            return Err(MeshError::Damaged(
                "hulls have more triangles than counted".into(),
            ));
        }
        self.faces += 1;
        Ok(3 * (self.faces - 1))
    }

    /// One hull; returns its vertex count.
    pub(super) fn hull(&mut self) -> Result<u32, MeshError> {
        let first = self.face()?;
        let f = first as usize;
        self.twin[f..f + 3].copy_from_slice(&[Twin::Border, Twin::Open, Twin::Border]);
        self.corner[f..f + 3].copy_from_slice(&[0, 1, 2]);
        let mut vertices = 3;
        // Gates to come back to after each S's first branch ends.
        let mut splits = Vec::new();
        let mut gate = first + 1;
        loop {
            let e = self.face()?;
            self.twin[e as usize] = Twin::Edge(gate);
            self.twin[gate as usize] = Twin::Edge(e);
            self.corner[e as usize + 1] = self.corner[prev(gate) as usize];
            self.corner[e as usize + 2] = self.corner[next(gate) as usize];
            gate = e + 1;
            match self.bits.code()? {
                Code::C => {
                    self.corner[e as usize] = vertices;
                    self.twin[next(gate) as usize] = Twin::Border;
                    vertices += 1;
                }
                Code::S => splits.push(gate),
                Code::L => {
                    self.twin[gate as usize] = Twin::Zipping;
                    gate = next(gate);
                }
                Code::R => {
                    self.twin[next(gate) as usize] = Twin::Zipping;
                    self.zip(next(gate))?;
                }
                Code::E => {
                    self.twin[gate as usize] = Twin::Zipping;
                    self.twin[next(gate) as usize] = Twin::Zipping;
                    self.zip(next(gate))?;
                    match splits.pop() {
                        Some(split) => gate = next(split),
                        None => return Ok(vertices),
                    }
                }
            }
        }
    }

    /// Zips the boundary closed from `e` while its edges wait to be.
    fn zip(&mut self, mut e: u32) -> Result<(), MeshError> {
        let budget = self.twin.len() + 1;
        let looped = || MeshError::Damaged("hull edges loop".into());
        for _ in 0..budget {
            if self.twin[e as usize] != Twin::Zipping {
                return Ok(());
            }
            // The boundary edge across the vertex, around the fan from the
            // edge after `e`.
            let mut other = next(e);
            let mut steps = 0..budget;
            while let Twin::Edge(t) = self.twin[other as usize] {
                steps.next().ok_or_else(looped)?;
                other = next(t);
            }
            if self.twin[other as usize] != Twin::Border {
                return Ok(());
            }
            self.twin[e as usize] = Twin::Edge(other);
            self.twin[other as usize] = Twin::Edge(e);
            e = prev(e);
            // The zipped-away vertex takes the surviving one's number all
            // around its fan.
            let survivor = self.corner[prev(other) as usize];
            self.corner[prev(e) as usize] = survivor;
            let mut fan = e;
            let mut steps = 0..budget;
            while let Twin::Edge(t) = self.twin[fan as usize]
                && fan != other
            {
                steps.next().ok_or_else(looped)?;
                fan = prev(t);
                self.corner[prev(fan) as usize] = survivor;
            }
            // On to the next edge still open along the boundary.
            let mut steps = 0..budget;
            while let Twin::Edge(t) = self.twin[e as usize]
                && e != other
            {
                steps.next().ok_or_else(looped)?;
                e = prev(t);
            }
        }
        Err(looped())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Codes packed as Roblox packs them: from each word's top bit, the
    /// last word's in its low bits.
    fn packed(codes: &str) -> (Vec<u8>, u32) {
        let bits: String = codes
            .chars()
            .map(|c| match c {
                'C' => "0",
                'S' => "100",
                'L' => "101",
                'R' => "110",
                'E' => "111",
                _ => unreachable!(),
            })
            .collect();
        let mut bytes = Vec::new();
        for chunk in bits.as_bytes().chunks(32) {
            let mut word = 0u32;
            for (i, b) in chunk.iter().enumerate() {
                word |= u32::from(b - b'0') << (chunk.len() - 1 - i);
            }
            bytes.extend_from_slice(&word.to_le_bytes());
        }
        (bytes, bits.len() as u32)
    }

    type Hulls = Vec<(Vec<[u32; 3]>, u32)>;

    /// Each hull's triangles and vertex count.
    fn decode(codes: &str, hulls: usize, faces: usize) -> Result<Hulls, MeshError> {
        let (bytes, bits) = packed(codes);
        let mut d = Decoder::new(Bits::new(&bytes, bits)?, faces);
        let mut ends = Vec::new();
        for _ in 0..hulls {
            let vertices = d.hull()?;
            ends.push((d.faces() as usize, vertices));
        }
        let corners = d.into_corners();
        let mut start = 0;
        Ok(ends
            .into_iter()
            .map(|(end, vertices)| {
                let tris = corners[start * 3..end * 3].as_chunks().0.to_vec();
                start = end;
                (tris, vertices)
            })
            .collect())
    }

    /// Every edge of a closed hull is shared by exactly two triangles,
    /// once each way.
    fn closed(tris: &[[u32; 3]], vertices: u32) -> bool {
        let mut edges = std::collections::HashMap::new();
        for t in tris {
            if t[0] == t[1] || t[1] == t[2] || t[0] == t[2] || t.iter().any(|&v| v >= vertices) {
                return false;
            }
            for i in 0..3 {
                *edges.entry((t[i], t[(i + 1) % 3])).or_insert(0) += 1;
            }
        }
        edges
            .iter()
            .all(|(&(a, b), &n)| n == 1 && edges.get(&(b, a)) == Some(&1))
    }

    #[test]
    fn decodes_closed_hulls() {
        // A tetrahedron, then an octahedron through a split; each hull
        // numbers its vertices from 0.
        let hulls = decode("CLECCCLSEE", 2, 12).unwrap();
        assert_eq!((hulls[0].1, hulls[1].1), (4, 6));
        assert_eq!((hulls[0].0.len(), hulls[1].0.len()), (4, 8));
        assert!(hulls.iter().all(|(tris, n)| closed(tris, *n)), "{hulls:?}");
        // R right after C folds back onto the triangle before.
        let folded = decode("CRE", 1, 4).unwrap();
        assert!(!closed(&folded[0].0, folded[0].1));
    }

    #[test]
    fn damage_is_an_error() {
        assert!(decode("CL", 1, 4).is_err(), "codes run out");
        assert!(decode("CLE", 1, 3).is_err(), "more triangles than counted");
        assert!(Bits::new(&[0; 3], 8).is_err());
        assert!(Bits::new(&[0; 4], 33).is_err());
    }
}
