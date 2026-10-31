//! The `CSGMDL` forms of a union's `MeshData`. Versions 2 and 4 are XORed
//! with a 31-byte key by absolute position (their magic too) and hold
//! 84-byte vertices; version 5 is plain, each vertex attribute in its own
//! list and the indices delta coded. Layouts:
//! `docs/behavior/meshes/unions.md`.

use rook_types::NormalId;

use super::super::reader::{Reader, floats, u32s};
use super::{Mesh, MeshError};

const KEY: [u8; 31] = [
    86, 46, 110, 88, 49, 32, 48, 4, 52, 105, 12, 119, 12, 1, 94, 0, 26, 96, 55, 105, 29, 82, 43, 7,
    79, 36, 89, 101, 83, 4, 122,
];

/// `CSGMDL` under the key.
pub(super) const MAGIC: [u8; 6] = [0x15, 0x7d, 0x29, 0x15, 0x75, 0x6c];

/// Undo the XOR, which runs from the first byte.
fn deobfuscate(bytes: &[u8]) -> Vec<u8> {
    let key = KEY.iter().cycle();
    bytes.iter().zip(key).map(|(b, k)| b ^ k).collect()
}

/// Versions 2 and 4 (4 adds a list after the indices that isn't read).
pub(super) fn csgmdl2(bytes: &[u8]) -> Result<Mesh, MeshError> {
    let plain = deobfuscate(bytes);
    let mut r = Reader::new(&plain);
    r.skip(10 + 32)?; // Magic and version, then a hash.
    let count = r.count()?;
    let stride = r.u32()? as usize;
    if stride < 40 {
        return Err(MeshError::Damaged(format!("{stride}-byte union vertices")));
    }
    let mut out = Mesh::default();
    for v in r.records(count, stride)? {
        let [px, py, pz, nx, ny, nz] = floats::<6>(v);
        out.positions.push([px, py, pz]);
        out.normals.push([nx, ny, nz]);
        out.colors.push(v[24..28].try_into().expect("4 bytes"));
        out.faces.push(normal_id(u32s::<1>(&v[28..])[0])?);
        out.uvs.push(floats::<2>(&v[32..]));
    }
    let indices = r.count()?;
    out.indices = r
        .records(indices / 3 * 3, 4)?
        .map(|i| u32s::<1>(i)[0])
        .collect();
    Ok(out)
}

pub(super) fn csgmdl5(bytes: &[u8]) -> Result<Mesh, MeshError> {
    let mut r = Reader::new(bytes);
    r.skip(10)?;
    let count = |r: &mut Reader| r.u16().map(usize::from);
    let positions = count(&mut r)?;
    let positions = r
        .records(positions, 12)?
        .map(floats::<3>)
        .collect::<Vec<_>>();
    let normals = count(&mut r)?;
    r.u32()?; // The normals' size in bytes.
    let normals = r.records(normals, 6)?.map(unit).collect::<Vec<_>>();
    let colors = count(&mut r)?;
    let colors = r.records(colors, 4)?;
    let colors = colors
        .map(|c| c.try_into().expect("4 bytes"))
        .collect::<Vec<_>>();
    let faces = count(&mut r)?;
    let faces = r.bytes(faces)?;
    let faces = faces
        .iter()
        .map(|&f| normal_id(u32::from(f)))
        .collect::<Result<Vec<_>, _>>()?;
    let uvs = count(&mut r)?;
    let uvs = r.records(uvs, 8)?.map(floats::<2>).collect::<Vec<_>>();
    let tangents = count(&mut r)?;
    r.u32()?;
    r.skip(tangents * 6)?;
    let n = positions.len();
    if [normals.len(), colors.len(), faces.len(), uvs.len()]
        .iter()
        .any(|&len| len != n)
    {
        return Err(MeshError::Damaged("vertex lists differ in length".into()));
    }
    Ok(Mesh {
        positions,
        normals,
        uvs,
        indices: indices5(&mut r)?,
        colors,
        faces,
        ..Mesh::default()
    })
}

/// A normal stored as three u16s offset by 32767.
fn unit(v: &[u8]) -> [f32; 3] {
    std::array::from_fn(|i| {
        let q = i16::from_le_bytes([v[i * 2], v[i * 2 + 1]]);
        f32::from(q.wrapping_sub(0x7fff)) / 32767.0
    })
}

/// Version 5's indices: a count, then each index as the difference from
/// the one before (one byte for -64 to 63, three for larger), then range
/// markers. The first two markers bound the drawn triangles; the ranges
/// after them aren't read (Inference: levels of detail).
fn indices5(r: &mut Reader) -> Result<Vec<u32>, MeshError> {
    let damaged = |what: &str| MeshError::Damaged(format!("union indices {what}"));
    let count = r.count()?;
    let len = r.u32()? as usize;
    let mut data = Reader::new(r.bytes(len)?);
    let mut out = Vec::with_capacity(count);
    let mut index = 0u32;
    for _ in 0..count {
        let b = data.u8()?;
        let step = match b {
            0..64 => i32::from(b),
            64..128 => i32::from(b) - 128,
            _ => {
                let [hi, lo] = data.array::<2>()?;
                i32::from_le_bytes([lo, hi, b - 128, 0])
            }
        };
        index = index.wrapping_add_signed(step);
        out.push(index & 0x7f_ffff);
    }
    if !data.is_empty() {
        return Err(damaged("have bytes left over"));
    }
    let markers = usize::from(r.u8()?);
    let markers: Vec<u32> = r.records(markers, 4)?.map(|m| u32s::<1>(m)[0]).collect();
    let [start, end, ..] = markers[..] else {
        return Err(damaged("have fewer than two markers"));
    };
    if !markers.is_sorted() || markers.last().is_some_and(|&m| m as usize > out.len()) {
        return Err(damaged("have markers out of order"));
    }
    out.truncate(end as usize);
    out.drain(..start as usize);
    Ok(out)
}

/// Unions number box faces from 1, in their own order.
fn normal_id(n: u32) -> Result<NormalId, MeshError> {
    Ok(match n {
        1 => NormalId::Right,
        2 => NormalId::Top,
        3 => NormalId::Back,
        4 => NormalId::Left,
        5 => NormalId::Bottom,
        6 => NormalId::Front,
        _ => return Err(MeshError::Damaged(format!("box face {n}"))),
    })
}
