//! A union's or MeshPart's collision geometry, read from its `PhysicsData`
//! or `PhysicalConfigData`: a box, or convex hulls in its own space, with
//! their mass. The `CSGPHS` versions and where the blob is are in
//! `docs/behavior/physics/collision-shapes.md`.

use glam::Vec3;
use rbx_types::Variant;
use rook_types::{Hull, SavedCollision, SavedMass};

use super::super::reader::{Reader, floats, u32s};
use super::MeshError;
use super::edgebreaker::{Bits, Decoder};

/// The blob of a part that collides as its box. It is also the property's
/// default, so a file without `PhysicalConfigData` reads as it (and Studio,
/// too, makes such a part a box).
const BLOCK: &[u8] = b"CSGPHS\0\0\0\0BLOCK";

/// Whether `bytes` hold collision geometry or a block, rather than
/// nothing or a `CSGK` pointer.
pub fn has_geometry(bytes: &[u8]) -> bool {
    bytes.starts_with(b"CSGPHS")
}

pub fn is_block(bytes: &[u8]) -> bool {
    bytes == BLOCK
}

/// The enum value of `Enum.CollisionFidelity` Studio reads for a blob:
/// version 7 and later save it in a byte of their own (0 Box, 1 Hull, 2
/// Default, 3 PreciseConvexDecomposition, whatever hulls follow); before
/// that one hull reads Hull and several Default. Observed in Studio 0.746.
pub fn fidelity(bytes: &[u8]) -> Result<u32, MeshError> {
    const DEFAULT: u32 = 0;
    const BOX: u32 = 1;
    const HULL: u32 = 2;
    const PRECISE: u32 = 3;
    if bytes.get(6).is_some_and(|v| *v >= 7) {
        return match bytes.get(10) {
            Some(0) => Ok(BOX),
            Some(1) => Ok(HULL),
            Some(2) => Ok(DEFAULT),
            Some(3) => Ok(PRECISE),
            _ => Err(MeshError::Damaged("the fidelity byte is unknown".into())),
        };
    }
    Ok(match decode(bytes)? {
        SavedCollision::Block => BOX,
        SavedCollision::Hulls { hulls, .. } if hulls.len() == 1 => HULL,
        SavedCollision::Hulls { .. } => DEFAULT,
    })
}

/// The physics blob among an instance's properties, if one holds geometry.
pub fn physics_data(properties: &rbx_dom_weak::UstrMap<Variant>) -> Option<Vec<u8>> {
    ["PhysicalConfigData", "PhysicsData"]
        .iter()
        .find_map(|name| {
            let bytes: &[u8] = match properties.get(&(*name).into())? {
                Variant::BinaryString(b) => b.as_ref(),
                Variant::SharedString(s) => s.data(),
                _ => return None,
            };
            has_geometry(bytes).then(|| bytes.to_vec())
        })
}

/// The collision geometry saved in a union asset (or any union in a
/// model), if any (none of the assets checked so far save it).
pub fn decode_asset(bytes: &[u8]) -> Result<Option<SavedCollision>, MeshError> {
    let loaded = rook_formats::load(bytes).map_err(|e| MeshError::Damaged(e.to_string()))?;
    loaded
        .dom
        .descendants()
        .find_map(|inst| physics_data(&inst.properties))
        .map(|data| decode(&data))
        .transpose()
}

pub fn decode(bytes: &[u8]) -> Result<SavedCollision, MeshError> {
    let damaged = |what: &str| MeshError::Damaged(format!("the union's PhysicsData {what}"));
    if !has_geometry(bytes) {
        return Err(damaged("has no geometry"));
    }
    let version = bytes
        .get(6..10)
        .map_or(0, |v| u32::from_le_bytes(v.try_into().unwrap_or_default()));
    let (header_at, saved_mass) = match version {
        0 if is_block(bytes) => return Ok(SavedCollision::Block),
        3 | 5 => (10, false),
        6 => (10, true),
        7 => (11, true),
        8 => return decode_v8(bytes),
        v => return Err(MeshError::Unsupported(format!("CSGPHS{v}"))),
    };
    let mut r = Reader::new(bytes);
    r.skip(header_at)?;
    let saved = saved_mass.then(|| saved_mass_header(&mut r)).transpose()?;
    let mut hulls = Vec::new();
    while !r.is_empty() {
        hulls.push(hull(&mut r)?);
    }
    let mass = saved.unwrap_or_else(|| hulls_mass(&hulls));
    Ok(SavedCollision::Hulls { hulls, mass })
}

/// Versions 6 and 7: volume, center of mass and the inertia's upper
/// triangle at unit density, about that center.
fn saved_mass_header(r: &mut Reader) -> Result<SavedMass, MeshError> {
    let f = floats::<10>(r.bytes(40)?);
    Ok(SavedMass {
        volume: f[0],
        center: Vec3::new(f[1], f[2], f[3]),
        inertia: [f[4], f[5], f[6], f[7], f[8], f[9]],
    })
}

/// What `hulls` add up to, as Studio works out for the versions that save
/// no mass (measured on a half-box: volume, center and inertia). Each hull
/// is a fan of tetrahedra from the origin, whose moments have a closed form.
fn hulls_mass(hulls: &[Hull]) -> SavedMass {
    let (mut volume, mut first) = (0.0_f64, [0.0_f64; 3]);
    let mut second = [[0.0_f64; 3]; 3];
    for h in hulls {
        let (mut v, mut f) = (0.0_f64, [0.0_f64; 3]);
        let mut s = [[0.0_f64; 3]; 3];
        for t in h.indices.as_chunks::<3>().0 {
            let corner = |i: usize| h.positions.get(t[i] as usize).map(|p| p.map(f64::from));
            let (Some(a), Some(b), Some(c)) = (corner(0), corner(1), corner(2)) else {
                continue;
            };
            let det = a[0] * (b[1] * c[2] - b[2] * c[1]) - a[1] * (b[0] * c[2] - b[2] * c[0])
                + a[2] * (b[0] * c[1] - b[1] * c[0]);
            v += det / 6.0;
            for i in 0..3 {
                f[i] += det * (a[i] + b[i] + c[i]) / 24.0;
                for j in 0..3 {
                    let pairs = a[i] * a[j] + b[i] * b[j] + c[i] * c[j];
                    let sum = (a[i] + b[i] + c[i]) * (a[j] + b[j] + c[j]);
                    s[i][j] += det * (pairs + sum) / 120.0;
                }
            }
        }
        // A hull's winding is its own; each adds its size.
        let sign = v.signum();
        volume += v * sign;
        for i in 0..3 {
            first[i] += f[i] * sign;
            for j in 0..3 {
                second[i][j] += s[i][j] * sign;
            }
        }
    }
    if volume <= 0.0 {
        return SavedMass::default();
    }
    let center = first.map(|c| c / volume);
    let about = |i: usize, j: usize| second[i][j] - volume * center[i] * center[j];
    let (xx, yy, zz) = (about(0, 0), about(1, 1), about(2, 2));
    SavedMass {
        volume: volume as f32,
        center: Vec3::new(center[0] as f32, center[1] as f32, center[2] as f32),
        inertia: [
            (yy + zz) as f32,
            -about(0, 1) as f32,
            -about(0, 2) as f32,
            (xx + zz) as f32,
            -about(1, 2) as f32,
            (xx + yy) as f32,
        ],
    }
}

/// Version 8: after the header, a type byte and a zero, everything is
/// zstd packed: counts, a bounding box, any hulls stored plainly, then the
/// rest as [Edgebreaker](super::edgebreaker) codes and their positions.
fn decode_v8(bytes: &[u8]) -> Result<SavedCollision, MeshError> {
    let packed = bytes
        .get(12..)
        .ok_or_else(|| MeshError::Damaged("CSGPHS8 is cut short".into()))?;
    let plain =
        zstd::decode_all(packed).map_err(|e| MeshError::Damaged(format!("CSGPHS8: {e}")))?;
    let mut r = Reader::new(&plain);
    let coded = r.count()?;
    let positions = r.count()?;
    let faces = r.count()?;
    r.skip(8)?; // The first hull's counts.
    let plain_size = r.u32()?;
    let bits = r.u32()?;
    let codes = r.count()?;
    r.skip(4 + 24)?; // The positions' size; the bounding box.
    // Only blobs with plainly stored hulls hold their list.
    let mut hulls = if plain_size == 0 {
        Vec::new()
    } else {
        plain_hulls(&mut r)?
    };
    let mut decoder = Decoder::new(Bits::new(r.bytes(codes)?, bits)?, faces);
    let points: Vec<[f32; 3]> = r.records(positions, 12)?.map(floats::<3>).collect();
    let mut ends = Vec::with_capacity(coded);
    for _ in 0..coded {
        let vertices = decoder.hull()?;
        ends.push((decoder.faces() as usize * 3, vertices as usize));
    }
    let corners = decoder.into_corners();
    let (mut face, mut point) = (0, 0);
    for (face_end, vertices) in ends {
        let positions = points.get(point..point + vertices);
        let positions =
            positions.ok_or_else(|| MeshError::Damaged("hulls need more positions".into()))?;
        hulls.push(Hull {
            positions: positions.to_vec(),
            indices: corners[face..face_end].to_vec(),
        });
        (face, point) = (face_end, point + vertices);
    }
    let mass = hulls_mass(&hulls);
    Ok(SavedCollision::Hulls { hulls, mass })
}

/// Version 8's plainly stored hulls: where each one's indices start (and,
/// last, end), the indices, then the same for position floats.
fn plain_hulls(r: &mut Reader) -> Result<Vec<Hull>, MeshError> {
    let ranges = |r: &mut Reader| -> Result<Vec<usize>, MeshError> {
        let n = r.count()?;
        let ranges: Vec<usize> = r.records(n, 4)?.map(|v| u32s::<1>(v)[0] as usize).collect();
        if !ranges.is_sorted() {
            return Err(MeshError::Damaged("hull ranges run backwards".into()));
        }
        Ok(ranges)
    };
    let face_ranges = ranges(r)?;
    let indices: Vec<u32> = (r.records(face_ranges.last().copied().unwrap_or(0), 4)?)
        .map(|v| u32s::<1>(v)[0])
        .collect();
    let pos_ranges = ranges(r)?;
    let floats_: Vec<f32> = (r.records(pos_ranges.last().copied().unwrap_or(0), 4)?)
        .map(|v| floats::<1>(v)[0])
        .collect();
    Ok(face_ranges
        .windows(2)
        .zip(pos_ranges.windows(2))
        .map(|(f, p)| Hull {
            positions: floats_[p[0]..p[1]].as_chunks().0.to_vec(),
            indices: indices[f[0]..f[1]].to_vec(),
        })
        .collect())
}

/// Write hulls as a version 5 blob (for tests and tools; Rook never saves
/// one).
pub fn encode(hulls: &[Hull]) -> Vec<u8> {
    let mut b = b"CSGPHS\x05\0\0\0".to_vec();
    for h in hulls {
        // The header `Reader::hull` skips, as Roblox writes it.
        b.extend_from_slice(&16u32.to_le_bytes());
        b.extend_from_slice(&[0; 16]);
        b.extend_from_slice(&16u32.to_le_bytes());
        b.extend_from_slice(&[0; 12]);
        b.extend_from_slice(&1f32.to_le_bytes());
        b.extend_from_slice(&(h.positions.len() as u32 * 3).to_le_bytes());
        b.extend_from_slice(&4u32.to_le_bytes());
        for f in h.positions.iter().flatten() {
            b.extend_from_slice(&f.to_le_bytes());
        }
        b.extend_from_slice(&(h.indices.len() as u32).to_le_bytes());
        for i in &h.indices {
            b.extend_from_slice(&i.to_le_bytes());
        }
    }
    b
}

/// One hull of versions 3 to 7: a fixed 40-byte header, the count of
/// position floats, a 4 (Inference: a vertex's stride in floats, though
/// positions are 3), the positions, the count of indices and the indices.
fn hull(r: &mut Reader) -> Result<Hull, MeshError> {
    r.skip(40)?;
    let count = r.count()?;
    r.u32()?;
    let positions = r.records(count / 3, 12)?.map(floats::<3>).collect();
    r.skip(count % 3 * 4)?;
    let count = r.count()?;
    let indices = r.records(count, 4)?.map(|i| u32s::<1>(i)[0]).collect();
    Ok(Hull { positions, indices })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tri(offset: f32, indices: [u32; 3]) -> Hull {
        Hull {
            positions: vec![
                [offset, offset, offset],
                [offset + 1.0, offset, offset],
                [offset, offset + 1.0, offset],
            ],
            indices: indices.to_vec(),
        }
    }

    /// Every hull of version 6 is read, not just the first.
    #[test]
    fn reads_every_hull() {
        let hulls = [tri(0.0, [0, 1, 2]), tri(2.0, [0, 2, 1])];
        let mut b = b"CSGPHS\x06\0\0\0".to_vec();
        b.extend_from_slice(&[0; 40]);
        b.extend_from_slice(&encode(&hulls)[10..]);
        let read = |b: &[u8]| match decode(b) {
            Ok(SavedCollision::Hulls { hulls, .. }) => hulls,
            other => panic!("{other:?}"),
        };
        assert_eq!(read(&b), hulls.to_vec());
        assert_eq!(read(&encode(&hulls)), hulls.to_vec());
    }

    #[test]
    fn version_7_has_a_type_byte() {
        let mut b = b"CSGPHS\x07\0\0\0\x02".to_vec();
        b.extend_from_slice(&[0; 40]);
        b.extend_from_slice(&encode(&[tri(0.0, [0, 1, 2])])[10..]);
        assert!(matches!(decode(&b), Ok(SavedCollision::Hulls { hulls, .. }) if hulls.len() == 1));
    }

    #[test]
    fn blocks_and_damage() {
        assert_eq!(decode(BLOCK), Ok(SavedCollision::Block));
        assert!(decode(b"CSGKf8a5e7d8f564986f0ae7e4bc02ef2ac4").is_err());
        let mut b = encode(&[tri(0.0, [0, 1, 2])]);
        b.truncate(b.len() - 2);
        assert!(matches!(decode(&b), Err(MeshError::Damaged(_))));
    }

    /// A box's corners, x from -1 to 0 and y and z from -1 to 1.
    fn half_box() -> Hull {
        let corner = |i: u32| {
            [
                [-1.0, 0.0][(i >> 2) as usize & 1],
                [-1.0, 1.0][(i >> 1) as usize & 1],
                [-1.0, 1.0][i as usize & 1],
            ]
        };
        Hull {
            positions: (0..8).map(corner).collect(),
            indices: vec![
                0, 1, 3, 0, 3, 2, 4, 6, 7, 4, 7, 5, 0, 4, 5, 0, 5, 1, 2, 3, 7, 2, 7, 6, 0, 2, 6, 0,
                6, 4, 1, 5, 7, 1, 7, 3,
            ],
        }
    }

    /// Numbers from Studio: at Plastic's 0.7 a half-box of a 2-stud cube
    /// weighs 2.8, its center of mass 0.5 off, spinning as a 1 x 2 x 2 box.
    #[test]
    fn versions_without_mass_add_up_their_hulls() {
        let Ok(SavedCollision::Hulls { mass, .. }) = decode(&encode(&[half_box()])) else {
            panic!("hulls");
        };
        assert!((mass.volume - 4.0).abs() < 1e-5);
        assert!((mass.center - Vec3::new(-0.5, 0.0, 0.0)).length() < 1e-5);
        let box_inertia = [8.0 / 12.0 * 4.0, 5.0 / 12.0 * 4.0, 5.0 / 12.0 * 4.0];
        assert!((mass.inertia[0] - box_inertia[0]).abs() < 1e-4, "{mass:?}");
        assert!((mass.inertia[3] - box_inertia[1]).abs() < 1e-4, "{mass:?}");
        assert!((mass.inertia[5] - box_inertia[2]).abs() < 1e-4, "{mass:?}");
    }

    #[test]
    fn saved_mass_is_read_from_the_header() {
        let mut b = b"CSGPHS\x06\0\0\0".to_vec();
        for f in [1.5_f32, -0.25, 0.5, 0.0, 3.0, 0.0, 0.0, 5.0, 0.0, 7.0] {
            b.extend_from_slice(&f.to_le_bytes());
        }
        b.extend_from_slice(&encode(&[half_box()])[10..]);
        let Ok(SavedCollision::Hulls { mass, .. }) = decode(&b) else {
            panic!("hulls");
        };
        assert_eq!(mass.volume, 1.5);
        assert_eq!(mass.center, Vec3::new(-0.25, 0.5, 0.0));
        assert_eq!(mass.inertia, [3.0, 0.0, 0.0, 5.0, 0.0, 7.0]);
    }

    /// `Enum.CollisionFidelity`'s values, as Studio read these blobs.
    #[test]
    fn fidelity_comes_from_the_blob() {
        let one = encode(&[half_box()]);
        let two = encode(&[half_box(), half_box()]);
        assert_eq!(fidelity(BLOCK), Ok(1));
        assert_eq!(fidelity(&one), Ok(2));
        assert_eq!(fidelity(&two), Ok(0));
        let v7 = |byte: u8| {
            let mut b = b"CSGPHS\x07\0\0\0".to_vec();
            b.push(byte);
            b.extend_from_slice(&[0; 40]);
            b.extend_from_slice(&one[10..]);
            b
        };
        for (byte, value) in [(0, 1), (1, 2), (2, 0), (3, 3)] {
            assert_eq!(fidelity(&v7(byte)), Ok(value));
        }
    }
}
