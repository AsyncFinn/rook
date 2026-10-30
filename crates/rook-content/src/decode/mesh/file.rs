//! Roblox mesh files: version 1's text, the binary versions 2.00 to 5.00,
//! and the chunked 6.00 and 7.00 ([`super::chunks`]). Layouts are in
//! `docs/behavior/meshes/file-meshes.md`.

use super::reader::{Reader, checked_count, floats, u32s};
use super::*;

pub(super) fn decode(bytes: &[u8]) -> Result<Mesh, MeshError> {
    let (version, body) = version(bytes).ok_or(MeshError::NotAMesh)?;
    let mut r = Reader::new(body);
    match version {
        "1.00" | "1.01" => text(body, version == "1.00"),
        "2.00" => {
            let mut head = header(&mut r, 12)?;
            let (vertex, face) = (head.u8()?, head.u8()?);
            let (vertices, faces) = (head.count()?, head.count()?);
            binary(&mut r, vertices, vertex, faces, face, None, 1)
        }
        // Version 3 doesn't say which levels are high quality: all are
        // taken to be (Inference; none was seen in Studio).
        "3.00" | "3.01" => {
            let mut head = header(&mut r, 16)?;
            let (vertex, face) = (head.u8()?, head.u8()?);
            let (lod, lods) = (head.u16()?, head.u16()?);
            let (vertices, faces) = (head.count()?, head.count()?);
            let lods = Some((usize::from(lods), lod));
            binary(&mut r, vertices, vertex, faces, face, lods, u8::MAX)
        }
        "4.00" | "4.01" | "5.00" => {
            let mut head = header(&mut r, 24)?;
            head.u16()?; // The LOD type: how the levels were made.
            let (vertices, faces) = (head.count()?, head.count()?);
            let (lods, bones) = (head.u16()?, head.u16()?);
            head.skip(6)?; // Bone names' length, subset count.
            let high_quality = head.u8()?;
            // Each vertex's bone weights follow the vertices when the
            // file has bones; they aren't drawn yet.
            let skin = if bones > 0 { 8 } else { 0 };
            let mut out = Mesh::default();
            read_vertices(&mut r, vertices, 40, &mut out)?;
            r.skip(vertices * skin)?;
            let faces = read_faces(&mut r, faces, 12)?;
            let lods = read_lods(&mut r, usize::from(lods), 4)?;
            set_levels(&mut out, &faces, &lods, high_quality);
            Ok(out)
        }
        "6.00" | "7.00" => chunks::decode(&mut r),
        other => Err(MeshError::Unsupported(other.to_owned())),
    }
}

/// The version in a mesh file's first line (`version 2.00`) and the bytes
/// after that line.
fn version(bytes: &[u8]) -> Option<(&str, &[u8])> {
    let end = bytes.iter().take(32).position(|&b| b == b'\n')?;
    let line = std::str::from_utf8(&bytes[..end]).ok()?;
    let version = line.trim_end_matches('\r').strip_prefix("version ")?;
    Some((version, &bytes[end + 1..]))
}

/// A binary header: it starts with its own size, which is at least
/// `least` and is where the data starts. Returns the rest of the header.
fn header<'a>(r: &mut Reader<'a>, least: u16) -> Result<Reader<'a>, MeshError> {
    let size = r.u16()?;
    if size < least {
        return Err(MeshError::Damaged(format!("a {size}-byte header")));
    }
    Ok(Reader::new(r.bytes(usize::from(size) - 2)?))
}

/// Versions 2 and 3: vertex and face sizes from the header, and in
/// version 3 a LOD table of `lods.0` entries `lods.1` bytes each.
fn binary(
    r: &mut Reader,
    vertices: usize,
    vertex: u8,
    faces: usize,
    face: u8,
    lods: Option<(usize, u16)>,
    high_quality: u8,
) -> Result<Mesh, MeshError> {
    let mut out = Mesh::default();
    read_vertices(r, vertices, usize::from(vertex), &mut out)?;
    let faces = read_faces(r, faces, usize::from(face))?;
    let lods = match lods {
        Some((count, size)) => read_lods(r, count, usize::from(size))?,
        None => Vec::new(),
    };
    set_levels(&mut out, &faces, &lods, high_quality);
    Ok(out)
}

/// Vertices of `stride` bytes, each starting with its position, normal
/// and UV (the tangent and color after them aren't drawn).
pub(super) fn read_vertices(
    r: &mut Reader,
    n: usize,
    stride: usize,
    out: &mut Mesh,
) -> Result<(), MeshError> {
    if stride < 32 {
        return Err(MeshError::Damaged(format!("{stride}-byte vertices")));
    }
    out.positions.reserve(n);
    out.normals.reserve(n);
    out.uvs.reserve(n);
    for v in r.records(n, stride)? {
        let [px, py, pz, nx, ny, nz, u, w] = floats::<8>(v);
        out.positions.push([px, py, pz]);
        out.normals.push([nx, ny, nz]);
        out.uvs.push([u, w]);
    }
    Ok(())
}

/// Faces of `stride` bytes, each starting with three u32 vertex indices.
pub(super) fn read_faces(
    r: &mut Reader,
    n: usize,
    stride: usize,
) -> Result<Vec<[u32; 3]>, MeshError> {
    if stride < 12 {
        return Err(MeshError::Damaged(format!("{stride}-byte faces")));
    }
    Ok(r.records(n, stride)?.map(u32s::<3>).collect())
}

/// A LOD table: entries of `stride` bytes, each a u32 face index.
pub(super) fn read_lods(r: &mut Reader, n: usize, stride: usize) -> Result<Vec<u32>, MeshError> {
    if n > 0 && stride < 4 {
        return Err(MeshError::Damaged(format!("{stride}-byte LOD entries")));
    }
    Ok(r.records(n, stride)?.map(|e| u32s::<1>(e)[0]).collect())
}

/// Every level of detail's faces into `out.indices`: `lods` gives each
/// level's first face and, last, where the last ends (none means one
/// level of every face). The table is read while it runs forward within
/// the faces; empty levels are left out. `high_quality` is the file's
/// count of high quality levels.
pub(super) fn set_levels(out: &mut Mesh, faces: &[[u32; 3]], lods: &[u32], high_quality: u8) {
    let mut ranges: Vec<_> = lods
        .windows(2)
        .map(|w| w[0] as usize..w[1] as usize)
        .take_while(|r| r.start <= r.end && r.end <= faces.len())
        .filter(|r| !r.is_empty())
        .collect();
    if ranges.is_empty() {
        ranges.push(0..faces.len());
    }
    for (i, range) in ranges.into_iter().enumerate() {
        if i > 0 {
            out.levels.starts.push(out.indices.len() as u32);
        }
        out.indices.extend(faces[range].iter().flatten());
    }
    out.levels.high_quality = high_quality.max(1);
}

/// Version 1: a face count line, then each triangle's three corners in
/// turn as `[position][normal][uv]` triples. 1.00 is at twice the size,
/// and V runs the other way in both. Studio makes version 1's normals unit
/// length, some files storing them up to 3.9 long, but keeps later
/// versions' as stored (Observed through EditableMesh); a zero one stays
/// zero.
fn text(body: &[u8], doubled: bool) -> Result<Mesh, MeshError> {
    let damaged = |what: &str| MeshError::Damaged(format!("version 1 {what}"));
    let text = std::str::from_utf8(body).map_err(|_| damaged("isn't text"))?;
    let (count, mut rest) = text
        .split_once('\n')
        .ok_or_else(|| damaged("has no faces"))?;
    let faces = count
        .trim()
        .parse::<u64>()
        .map_err(|_| damaged("has no face count"))?;
    let corners = checked_count(faces.saturating_mul(3))?;
    let mut triple = || -> Option<[f32; 3]> {
        let (_, after) = rest.split_once('[')?;
        let (inside, after) = after.split_once(']')?;
        rest = after;
        let mut parts = inside.split(',').map(|s| s.trim().parse::<f32>().ok());
        Some([parts.next()??, parts.next()??, parts.next()??])
    };
    let scale = if doubled { 0.5 } else { 1.0 };
    let mut out = Mesh::default();
    for _ in 0..corners {
        let (pos, norm, uv) = (|| Some((triple()?, triple()?, triple()?)))()
            .ok_or_else(|| damaged("has fewer corners than its count"))?;
        out.positions.push(pos.map(|p| p * scale));
        let length = (norm[0] * norm[0] + norm[1] * norm[1] + norm[2] * norm[2]).sqrt();
        out.normals.push(if length > 0.0 {
            norm.map(|c| c / length)
        } else {
            norm
        });
        out.uvs.push([uv[0], 1.0 - uv[1]]);
    }
    out.indices = (0..corners as u32).collect();
    out.levels.high_quality = 1;
    Ok(out)
}
