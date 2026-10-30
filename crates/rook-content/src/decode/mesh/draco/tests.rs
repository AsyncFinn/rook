use super::*;

/// Writes Draco streams as its encoder does, for the decoder to read back.
#[derive(Default)]
struct Writer(Vec<u8>);

impl Writer {
    fn u8(&mut self, v: u8) -> &mut Self {
        self.0.push(v);
        self
    }

    fn bytes(&mut self, b: &[u8]) -> &mut Self {
        self.0.extend_from_slice(b);
        self
    }

    fn varint(&mut self, mut v: u64) -> &mut Self {
        loop {
            let b = (v & 0x7f) as u8;
            v >>= 7;
            if v == 0 {
                return self.u8(b);
            }
            self.u8(b | 0x80);
        }
    }

    /// A rANS stream of `symbols` over `probs`, which add up to the
    /// precision `length` gives.
    fn rans(&mut self, length: u8, probs: &[u32], symbols: &[u32]) -> &mut Self {
        let precision = 1u32 << (3 * u32::from(length) / 2).clamp(12, 20);
        assert_eq!(probs.iter().sum::<u32>(), precision);
        self.varint(probs.len() as u64);
        for &p in probs {
            match p {
                0..64 => self.u8((p << 2) as u8),
                64..16384 => self.u8(((p & 63) << 2) as u8 | 1).u8((p >> 6) as u8),
                _ => panic!("test probabilities stay under 16384"),
            };
        }
        let starts: Vec<u32> = probs
            .iter()
            .scan(0, |s, &p| Some(std::mem::replace(s, *s + p)))
            .collect();
        let low = 4 * precision;
        let mut state = low;
        let mut data = Vec::new();
        for &s in symbols.iter().rev() {
            let p = probs[s as usize];
            while state >= low / precision * 256 * p {
                data.push(state as u8);
                state >>= 8;
            }
            state = state / p * precision + state % p + starts[s as usize];
        }
        let state = state - low;
        let tail: Vec<u8> = match state {
            0..64 => vec![state as u8],
            64..16384 => (1 << 14 | state).to_le_bytes()[..2].to_vec(),
            _ => (2 << 22 | state).to_le_bytes()[..3].to_vec(),
        };
        data.extend(tail);
        self.varint(data.len() as u64).bytes(&data)
    }
}

fn decode_symbols(bytes: &[u8], n: usize, components: usize) -> Result<Vec<u32>, MeshError> {
    rans::symbols(&mut Reader::new(bytes), n, components)
}

#[test]
fn reads_raw_symbols() {
    let symbols: Vec<u32> = (0..200).map(|i| [0, 1, 1, 2, 1][i % 5]).collect();
    let mut w = Writer::default();
    w.u8(1).u8(2).rans(2, &[1024, 2048, 1024], &symbols);
    assert_eq!(decode_symbols(&w.0, symbols.len(), 1).unwrap(), symbols);
}

/// Tagged: rANS codes each group's bit length, the bits follow plainly.
#[test]
fn reads_tagged_symbols() {
    let lengths = [3u32, 0, 5];
    let mut w = Writer::default();
    w.u8(0).rans(5, &[1366, 0, 0, 1365, 0, 1365], &lengths);
    // Groups of two: 5 and 6 in 3 bits, then 0 and 0, then 17 and 30.
    let values = [5u32, 6, 0, 0, 17, 30];
    let bits = [(5u32, 3), (6, 3), (17, 5), (30, 5)];
    let mut packed = 0u64;
    let mut at = 0;
    for (v, n) in bits {
        packed |= u64::from(v) << at;
        at += n;
    }
    w.bytes(&packed.to_le_bytes()[..2]).u8(0xee);
    let mut r = Reader::new(&w.0);
    assert_eq!(rans::symbols(&mut r, 6, 2).unwrap(), values);
    assert_eq!(r.remaining(), [0xee], "only the bits' whole bytes are used");
}

/// One triangle laid out as Roblox stores it: raw positions and UVs,
/// quantized normals with difference prediction, then an integer
/// attribute that's stepped over.
pub(in crate::decode::mesh) fn triangle() -> Vec<u8> {
    let mut w = Writer::default();
    w.bytes(b"DRACO").u8(2).u8(2).u8(1).u8(0).bytes(&[0, 0]);
    w.varint(1).varint(3).u8(1).bytes(&[0, 2, 1]);
    w.u8(1).varint(4);
    for (kind, data_type, components) in [(0, 9, 3), (4, 9, 3), (3, 9, 2), (2, 2, 4)] {
        w.u8(kind).u8(data_type).u8(components).u8(0).varint(0);
    }
    w.bytes(&[0, 2, 0, 1]);
    for f in [0.0f32, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0] {
        w.bytes(&f.to_le_bytes());
    }
    // Normals: quantized 0..=2 per component (2 bits), stored as
    // differences from the point before, wrapped into 0..=3.
    let normals = [[1, 1, 2], [1, 1, 2], [3, 1, 1]];
    let mut before = [0i32; 3];
    let mut corrections = Vec::new();
    for n in normals {
        for c in 0..3 {
            let d = n[c] - before[c];
            corrections.push(if d < 0 { (-d - 1) * 2 + 1 } else { d * 2 } as u32);
        }
        before = n;
    }
    w.u8(0)
        .u8(1)
        .u8(1)
        .u8(1)
        .u8(2)
        .rans(2, &[820, 819, 819, 819, 819], &corrections);
    w.bytes(&0i32.to_le_bytes()).bytes(&3i32.to_le_bytes());
    for f in [0.0f32, 0.0, 1.0, 0.0, 0.0, 1.0] {
        w.bytes(&f.to_le_bytes());
    }
    // The colors: no prediction, not entropy coded, one byte each.
    w.bytes(&[0xfe, 0, 1]).bytes(&[0; 12]);
    w.bytes(&[-1.0f32, -1.0, -1.0, 2.0].map(f32::to_le_bytes).concat())
        .u8(2);
    w.0
}

#[test]
fn reads_a_mesh() {
    let g = decode(&triangle()).unwrap();
    assert_eq!(g.positions, [[0.0; 3], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]]);
    assert_eq!(g.faces, [[0, 2, 1]]);
    // Dequantized: q * 2 / 3 - 1.
    let n = |q: f32| q * (2.0 / 3.0) - 1.0;
    assert_eq!(g.normals[0], [n(1.0), n(1.0), n(2.0)]);
    assert_eq!(g.normals[2], [n(3.0), n(1.0), n(1.0)]);
    assert_eq!(g.uvs, [[0.0, 0.0], [1.0, 0.0], [0.0, 1.0]]);
}

#[test]
fn damage_is_an_error() {
    let full = triangle();
    for end in 0..full.len() {
        assert!(decode(&full[..end]).is_err(), "cut at {end}");
    }
    let mut old = full;
    old[6] = 1;
    assert!(matches!(decode(&old), Err(MeshError::Unsupported(_))));
}
