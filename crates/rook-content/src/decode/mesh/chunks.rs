//! Mesh versions 6.00 and 7.00: named chunks, each an 8-byte name, a
//! version and a size. The geometry is in `COREMESH` (version 1 as
//! version 5's vertices and faces, version 2 [Draco](super::draco)
//! compressed) and the levels of detail in `LODS`; the chunks for skinning,
//! facial animation and hidden surfaces are skipped by their size until
//! something draws them. Layouts: `docs/behavior/meshes/file-meshes.md`.

use super::file::{read_faces, read_lods, read_vertices, set_levels};
use super::reader::Reader;
use super::*;

pub(super) fn decode(r: &mut Reader) -> Result<Mesh, MeshError> {
    let mut mesh = None;
    let mut faces = Vec::new();
    let mut lods = Vec::new();
    let mut high_quality = 1;
    while !r.is_empty() {
        let name = r.array::<8>()?;
        let version = r.u32()?;
        let size = r.u32()? as usize;
        let mut body = Reader::new(r.bytes(size)?);
        match (&name, version) {
            (b"COREMESH", 1) => {
                let mut out = Mesh::default();
                let vertices = body.count()?;
                read_vertices(&mut body, vertices, 40, &mut out)?;
                let count = body.count()?;
                faces = read_faces(&mut body, count, 12)?;
                mesh = Some(out);
            }
            (b"COREMESH", 2) => {
                let len = body.u32()? as usize;
                let geometry = draco::decode(body.bytes(len)?)?;
                faces = geometry.faces;
                mesh = Some(Mesh {
                    positions: geometry.positions,
                    normals: geometry.normals,
                    uvs: geometry.uvs,
                    ..Mesh::default()
                });
            }
            (b"COREMESH", v) => {
                return Err(MeshError::Unsupported(format!("COREMESH {v}")));
            }
            (b"LODS\0\0\0\0", 1) => {
                body.u16()?; // The LOD type, as in version 4.
                high_quality = body.u8()?;
                let count = body.count()?;
                lods = read_lods(&mut body, count, 4)?;
            }
            _ => {}
        }
    }
    let mut out = mesh.ok_or_else(|| MeshError::Damaged("no COREMESH chunk".into()))?;
    set_levels(&mut out, &faces, &lods, high_quality);
    Ok(out)
}
