//! The primitive shapes: Block, Wedge and CornerWedge from their faces,
//! and Ball and Cylinder as Roblox builds them.
//!
//! Images and surface patterns on a primitive are mapped from each
//! vertex's point on the shape's bounding box and the box face it belongs
//! to (`Vertex::box_point`, `Vertex::face`). For flat faces that's the
//! vertex itself. The round shapes are built from a box whose faces are
//! split into a grid and pushed out onto the round surface, each vertex
//! keeping the grid point it came from (measured from Studio's OBJ export
//! of a Ball and a Cylinder part, 2026-10-13: see [`ball`] and
//! [`cylinder`]). So a decal on a ball or cylinder covers its face's whole
//! patch of the surface, as in Roblox.

use super::*;

/// A flat-sided shape from its faces (shared with ray tests and physics).
pub(super) fn polyhedron(shape: PartShape) -> MeshData {
    let mut m = MeshData::default();
    for face in shape.faces().unwrap_or_default() {
        m.polygon(face);
    }
    m
}

/// Grid squares along each edge of a round shape's box faces (measured: 6
/// for both the Ball and the Cylinder).
const GRID: u32 = 6;

/// A point of a box face's grid: `s` and `t` from -1 to 1 along the face's
/// image right and up, on the unit cube (extents -1..1).
fn grid_point(face: NormalId, s: f32, t: f32) -> Vec3 {
    let (right, up) = face.image_axes();
    face.normal() + right * s + up * t
}

/// The grid coordinate of step `i` of [`GRID`], from -1 to 1.
fn step(i: u32) -> f32 {
    i as f32 / GRID as f32 * 2.0 - 1.0
}

/// A Cylinder part's shape, along X, radius 0.5, as Roblox builds it
/// (measured from Studio's OBJ export: 114 vertices, 96 triangles). Its
/// round side is a square split into [`GRID`] segments per side (24 in all,
/// so the corners sit at atan(1/3), atan(2/3), 45 degrees, ...) with each
/// point pushed out onto the circle; normals are smooth. Each quarter maps
/// images from the Top, Front, Bottom or Back face of the box, linearly
/// along the square's side: the quarter's 45-degree seams are the face's
/// edges, and the Bottom quarter's image is turned half a turn from the
/// box face's (observed in Studio 0.742, 2026-10-19: one image on a block
/// and a cylinder, upright on the cylinder's Bottom and upside down on the
/// block's; Top, Back and Front matched; CylinderMesh and the SpecialMesh
/// Cylinder are Inference). The flat ends are the Left and Right faces.
pub(super) fn cylinder() -> MeshData {
    let mut m = MeshData::default();
    for face in [
        NormalId::Top,
        NormalId::Back,
        NormalId::Bottom,
        NormalId::Front,
    ] {
        let n = face.normal();
        // Along the side, so that (X, w) turns towards the outside.
        let w = n.cross(Vec3::X);
        let base = m.vertices.len() as u32;
        for i in 0..=GRID {
            let g = n + w * step(i);
            let round = Vec3::new(0.0, g.y, g.z).normalize();
            for x in [-1.0f32, 1.0] {
                let mut box_point = Vec3::new(x, g.y, g.z) * 0.5;
                if face == NormalId::Bottom {
                    // Turned half a turn: the Bottom quarter shows its
                    // image upside down from a block's Bottom face.
                    box_point *= Vec3::new(-1.0, 1.0, -1.0);
                }
                m.vertices.push(Vertex {
                    position: [x * 0.5, round.y * 0.5, round.z * 0.5],
                    normal: round.to_array(),
                    uv: [0.0; 2],
                    box_point: box_point.to_array(),
                    face: face.index() as u32,
                });
            }
        }
        for i in 0..GRID {
            let a = base + i * 2;
            // (x-, s), (x+, s), (x+, s'), (x-, s').
            m.indices
                .extend_from_slice(&[a, a + 1, a + 3, a, a + 3, a + 2]);
        }
    }
    // The ends: the round rim, flat.
    let mut rim = Vec::new();
    for face in [
        NormalId::Top,
        NormalId::Back,
        NormalId::Bottom,
        NormalId::Front,
    ] {
        let n = face.normal();
        let w = n.cross(Vec3::X);
        for i in 0..GRID {
            let g = n + w * step(i);
            rim.push(Vec3::new(0.0, g.y, g.z).normalize() * 0.5);
        }
    }
    // In order around the axis.
    rim.sort_by(|a, b| a.z.atan2(a.y).total_cmp(&b.z.atan2(b.y)));
    for x in [0.5f32, -0.5] {
        let mut cap: Vec<Vec3> = rim.iter().map(|p| Vec3::new(x, p.y, p.z)).collect();
        let normal = (cap[1] - cap[0]).cross(cap[2] - cap[0]);
        if normal.x * x < 0.0 {
            cap.reverse();
        }
        m.polygon(&cap);
    }
    m
}

/// A Ball part's shape, radius 0.5, as Roblox builds it (measured from
/// Studio's OBJ export: 294 vertices, 432 triangles): a cube whose six
/// faces are each split into a [`GRID`] × [`GRID`] grid, every point pushed
/// out onto the sphere, each face with its own vertices; normals are
/// smooth. Each face's patch maps images from that face of the box,
/// linearly along its grid.
pub(super) fn ball() -> MeshData {
    let mut m = MeshData::default();
    for face in NormalId::ALL {
        let base = m.vertices.len() as u32;
        for j in 0..=GRID {
            for i in 0..=GRID {
                let g = grid_point(face, step(i), step(j));
                let round = g.normalize();
                m.vertices.push(Vertex {
                    position: (round * 0.5).to_array(),
                    normal: round.to_array(),
                    uv: [i as f32 / GRID as f32, 1.0 - j as f32 / GRID as f32],
                    box_point: (g * 0.5).to_array(),
                    face: face.index() as u32,
                });
            }
        }
        let row = GRID + 1;
        for j in 0..GRID {
            for i in 0..GRID {
                let a = base + j * row + i;
                // Right then up is counter-clockwise seen from outside.
                m.indices
                    .extend_from_slice(&[a, a + 1, a + row + 1, a, a + row + 1, a + row]);
            }
        }
    }
    m
}
