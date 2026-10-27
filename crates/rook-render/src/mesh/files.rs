//! Mesh files, unions and terrain as draw meshes.

use super::*;

/// A mesh file's or a union's vertices as they are. A vertex whose normal
/// is zero (a file that left them out) takes the normal of the first
/// triangle using it, so it's lit instead of drawing black (Studio lights
/// such meshes; observed with a test mesh written without normals). A
/// union's vertices keep the box face they were cut from and their color.
pub(crate) fn from_file(m: &rook_content::decode::mesh::Mesh) -> MeshData {
    let mut vertices: Vec<Vertex> = m
        .positions
        .iter()
        .zip(&m.normals)
        .zip(&m.uvs)
        .map(|((&position, &normal), &uv)| Vertex::plain(position, normal, uv))
        .collect();
    for ((v, face), [r, g, b, _]) in vertices.iter_mut().zip(&m.faces).zip(&m.colors) {
        v.face = face.index() as u32 | UNION_FACE | u32::from_le_bytes([0, *r, *g, *b]);
    }
    if let Some(mapping) = m.terrain {
        let face = TERRAIN_FACE
            + match mapping {
                TerrainMapping::Planes => 0,
                TerrainMapping::Cube => 1,
                TerrainMapping::Water => 2,
                TerrainMapping::Blades => 4,
            };
        for v in &mut vertices {
            v.face = face;
        }
        for (v, c) in vertices.iter_mut().zip(&m.terrain_corners) {
            v.uv = [
                f32::from(u8::from(c.corner == 0)),
                f32::from(u8::from(c.corner == 1)),
            ];
            v.box_point = c.layers.map(f32::from);
            v.face |= u32::from(c.drawn) << TERRAIN_DRAWN_SHIFT;
        }
    }
    for tri in m.indices.as_chunks::<3>().0 {
        let [a, b, c] = tri.map(|i| i as usize);
        if [a, b, c].iter().any(|&i| i >= vertices.len()) {
            continue;
        }
        let p = |i: usize| Vec3::from(vertices[i].position);
        let face = (p(b) - p(a)).cross(p(c) - p(a)).normalize_or_zero();
        for i in [a, b, c] {
            if Vec3::from(vertices[i].normal).length_squared() < 1e-12 {
                vertices[i].normal = face.to_array();
            }
        }
    }
    MeshData {
        vertices,
        indices: m.indices.clone(),
        levels: m.levels.clone(),
    }
}
