//! Unit primitive meshes (extents -0.5..0.5) with flat or smooth normals,
//! and Rook's own head mesh. Each is scaled by the instance's size in the
//! vertex shader.
//!
//! Images and surface patterns on a primitive are mapped from each
//! vertex's point on the shape's bounding box and the box face it belongs
//! to (`Vertex::box_point`, `Vertex::face`). For flat faces that's the
//! vertex itself. The round shapes are built as Roblox builds them, from a
//! box whose faces are split into a grid and pushed out onto the round
//! surface, each vertex keeping the grid point it came from (measured from
//! Studio's OBJ export of a Ball and a Cylinder part, 2026-10-13: see
//! [`ball`] and [`cylinder`]). So a decal on a ball or cylinder covers its
//! face's whole patch of the surface, as in Roblox.

use bytemuck::{Pod, Zeroable};
use glam::Vec3;

use rook_content::decode::mesh::TerrainMapping;

use rook_types::{NormalId, PartShape, TrussShape};

use crate::scene::DrawShape;

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub(crate) struct Vertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
    /// The point on the shape's bounding box this vertex maps images and
    /// surfaces from: the vertex itself on flat faces and mesh files.
    pub box_point: [f32; 3],
    /// The `NormalId` index of the box face it belongs to, or [`NO_FACE`]
    /// (mesh files: the face is found from the normal), in the low byte;
    /// a union's vertex colour (sRGB) in the three bytes above, drawn
    /// instead of the part's colour when the instance asks for it
    /// (`PartInstance::vertex_colors`). wgpu allows 16 vertex attributes
    /// and the vertex and instance layouts use them all, hence the packing.
    pub face: u32,
}

/// No box face of its own: the shader finds one from the normal.
pub(crate) const NO_FACE: u32 = 255;

/// Added to a union vertex's face: Textures and material textures tile
/// from the face's centre, as they follow the union's UVs in Studio.
pub(crate) const UNION_FACE: u32 = 64;

/// A terrain vertex's face: its material textures are projected onto the
/// plane its surface faces most, in world studs; plus 1 for the axes'
/// planes only, 2 for water and 4 for grass blades
/// (`rook_content::decode::mesh::TerrainMapping`).
pub(crate) const TERRAIN_FACE: u32 = 128;

/// Where a terrain seam vertex's face holds which corners' layer its mesh
/// draws (`TerrainCorner::drawn`, 3 bits). The vertex's UV is its
/// barycentric weights' first two, and its box point its triangle's
/// corners' layers.
pub(crate) const TERRAIN_DRAWN_SHIFT: u32 = 9;

impl Vertex {
    /// A vertex mapped from where it is, its face found from its normal.
    fn plain(position: [f32; 3], normal: [f32; 3], uv: [f32; 2]) -> Self {
        Self {
            position,
            normal,
            uv,
            box_point: position,
            face: NO_FACE,
        }
    }
}

#[derive(Debug, Default)]
pub(crate) struct MeshData {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
}

impl MeshData {
    /// A flat polygon (convex, counter-clockwise seen from outside). Its UVs
    /// put the whole image on the face its normal belongs to, oriented as
    /// Studio orients images on that face (`rook_types::NormalId`).
    fn polygon(&mut self, points: &[Vec3]) {
        let normal = (points[1] - points[0])
            .cross(points[2] - points[0])
            .normalize();
        let face = NormalId::of_normal(normal);
        let base = self.vertices.len() as u32;
        for p in points {
            let (at, extent) = face.image_position(*p, Vec3::ONE);
            self.vertices.push(Vertex {
                face: face.index() as u32,
                ..Vertex::plain(p.to_array(), normal.to_array(), (at / extent).to_array())
            });
        }
        for i in 1..points.len() as u32 - 1 {
            self.indices
                .extend_from_slice(&[base, base + i, base + i + 1]);
        }
    }
}

/// A flat-sided shape from its faces (shared with ray tests and physics).
fn polyhedron(shape: PartShape) -> MeshData {
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
fn cylinder() -> MeshData {
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
fn ball() -> MeshData {
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

/// The head's radius and half height (about 1.2 studs across and tall).
const HEAD_RADIUS: f32 = 0.6012;
/// Around the Y axis: the first at 5 degrees from +X towards +Z, then every
/// 10 degrees.
const HEAD_SEGMENTS: u32 = 36;
/// Steps in each rounded edge, 22.5 degrees apart.
const HEAD_EDGE_STEPS: u32 = 4;

/// The head's UVs: segments facing within 45 degrees of -Z (the front
/// quarter, from 225 to 315 degrees) map the image straight on, across
/// the head's 2R x 2R front; every other vertex has UV (0, 0), so a face
/// decal's top-left pixel covers the back and sides.
fn head_uv(front: bool, position: [f32; 3]) -> [f32; 2] {
    if !front {
        return [0.0; 2];
    }
    let span = 2.0 * HEAD_RADIUS;
    [0.5 - position[0] / span, 0.5 - position[1] / span]
}

/// Rook's own head, built from measurements of the `head.mesh` in the
/// user's Roblox install (the Head MeshType draws that file): a solid of
/// revolution with rounded rims and its front quarter's UVs projected
/// straight on. The measured facts are in `docs/behavior/meshes.md`; the
/// user's own install can supply the real file for [`DrawShape::Head`].
fn head() -> MeshData {
    use std::f32::consts::{FRAC_PI_2, TAU};
    let mut m = MeshData::default();
    let r = HEAD_RADIUS;
    // The profile from the bottom rim to the top rim: (radius, y, normal
    // angle above the horizontal).
    let mut profile = Vec::new();
    for (centre_y, from) in [(-r / 2.0, -FRAC_PI_2), (r / 2.0, 0.0)] {
        for i in 0..=HEAD_EDGE_STEPS {
            let a = from + i as f32 / HEAD_EDGE_STEPS as f32 * FRAC_PI_2;
            profile.push((r / 2.0 + r / 2.0 * a.cos(), centre_y + r / 2.0 * a.sin(), a));
        }
    }
    let around = |k: u32| (5.0f32.to_radians() + k as f32 / HEAD_SEGMENTS as f32 * TAU).sin_cos();
    // Segments 22 to 30 run from 225 to 315 degrees: the front quarter.
    let is_front = |k: u32| (22..31).contains(&k);
    let vertex = |&(radius, y, a): &(f32, f32, f32), k: u32, front: bool| {
        let (sin, cos) = around(k);
        let position = [radius * cos, y, radius * sin];
        Vertex::plain(
            position,
            [a.cos() * cos, a.sin(), a.cos() * sin],
            head_uv(front, position),
        )
    };
    // Each segment has its own vertices, so the front quarter's edges are
    // seams; the normals stay smooth.
    for k in 0..HEAD_SEGMENTS {
        let front = is_front(k);
        let base = m.vertices.len() as u32;
        for ring in &profile {
            m.vertices.push(vertex(ring, k, front));
            m.vertices.push(vertex(ring, k + 1, front));
        }
        for ring in 0..profile.len() as u32 - 1 {
            let a = base + ring * 2;
            let b = a + 2;
            m.indices.extend_from_slice(&[a, b, a + 1, a + 1, b, b + 1]);
        }
        // Caps: a fan triangle from the centre, sharing the rims' normals
        // (straight up and down there).
        for (ring, ny) in [(0u32, -1.0f32), (profile.len() as u32 - 1, 1.0)] {
            let centre = m.vertices.len() as u32;
            let position = [0.0, ny * r, 0.0];
            m.vertices.push(Vertex::plain(
                position,
                [0.0, ny, 0.0],
                head_uv(front, position),
            ));
            let (a, b) = (base + ring * 2, base + ring * 2 + 1);
            let tri = if ny < 0.0 {
                [centre, a, b]
            } else {
                [centre, b, a]
            };
            m.indices.extend_from_slice(&tri);
        }
    }
    m
}

/// One face of a box centred at `centre` whose edges run along `axes` (a
/// right-handed frame) with half extents `half`: the face on the `sign`
/// side of axis `k`, flat, wound counter-clockwise seen from outside.
fn box_face(m: &mut MeshData, centre: Vec3, axes: [Vec3; 3], half: Vec3, k: usize, sign: f32) {
    let normal = axes[k] * sign;
    let (mut a, mut b) = (k + 1, k + 2);
    if sign < 0.0 {
        std::mem::swap(&mut a, &mut b);
    }
    let a = axes[a % 3] * half[a % 3];
    let b = axes[b % 3] * half[b % 3];
    let c = centre + normal * half[k];
    let base = m.vertices.len() as u32;
    for p in [c - a - b, c + a - b, c + a + b, c - a + b] {
        m.vertices
            .push(Vertex::plain(p.to_array(), normal.to_array(), [0.0; 2]));
    }
    m.indices
        .extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
}

/// A TrussPart's bars, along Y, in studs, centred on the origin, built from
/// the facts `rook_types::TrussShape` documents (measured from Studio's
/// OBJ export of a 2 × 2 × 2 and a 2 × 4 × 2 truss, 2026-10-13): four
/// 0.25-stud rails at the corners, a 0.25-stud rung across each side at
/// the truss's ends and a 0.5-stud one where two segments meet, and each
/// segment's braces. Faces hidden inside the bars where they meet are left
/// out (the rungs' and braces' ends); normals are flat. The export maps its
/// UVs into the surface-pattern strip, which Studio doesn't draw on
/// trusses, so the bars have none.
fn truss(shape: TrussShape) -> MeshData {
    let mut m = MeshData::default();
    let bar = TrussShape::BAR;
    let half_length = shape.length() / 2.0;
    // From the axis to the centre of a rail, a rung or a brace.
    let side_at = TrussShape::SEGMENT / 2.0 - bar / 2.0;
    let frame = [Vec3::X, Vec3::Y, Vec3::Z];
    for (x, z) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
        let centre = Vec3::new(x, 0.0, z) * side_at;
        let half = Vec3::new(bar / 2.0, half_length, bar / 2.0);
        for k in 0..3 {
            for sign in [-1.0, 1.0] {
                box_face(&mut m, centre, frame, half, k, sign);
            }
        }
    }
    // The rungs' bands along the truss: (centre, half thickness).
    let mut bands = vec![
        (-half_length + bar / 2.0, bar / 2.0),
        (half_length - bar / 2.0, bar / 2.0),
    ];
    for joint in 1..shape.segments {
        bands.push((-half_length + f32::from(joint) * TrussShape::SEGMENT, bar));
    }
    for side in TrussShape::SIDES {
        let normal = side.normal();
        let across = TrussShape::across(side);
        for &(y, thickness) in &bands {
            let centre = normal * side_at + Vec3::Y * y;
            let half = across * (TrussShape::SEGMENT / 2.0 - bar)
                + Vec3::Y * thickness
                + normal.abs() * (bar / 2.0);
            for k in (0..3).filter(|&k| frame[k] != across) {
                for sign in [-1.0, 1.0] {
                    box_face(&mut m, centre, frame, half, k, sign);
                }
            }
        }
        for segment in 0..shape.segments {
            let Some(lean) = shape.brace(segment, side) else {
                continue;
            };
            let middle = -half_length + (f32::from(segment) + 0.5) * TrussShape::SEGMENT;
            let along = (across * lean.sign() + Vec3::Y).normalize();
            let axes = [along, normal.cross(along), normal];
            let half = Vec3::new(
                side_at * std::f32::consts::SQRT_2,
                TrussShape::BRACE_WIDTH / 2.0,
                TrussShape::BRACE_DEPTH / 2.0,
            );
            let centre = normal * side_at + Vec3::Y * middle;
            for k in 1..3 {
                for sign in [-1.0, 1.0] {
                    box_face(&mut m, centre, axes, half, k, sign);
                }
            }
        }
    }
    m
}

/// A mesh file's or a union's vertices as they are. A vertex whose normal
/// is zero (a file that left them out) takes the normal of the first
/// triangle using it, so it's lit instead of drawing black (Studio lights
/// such meshes; observed with a test mesh written without normals). A
/// union's vertices keep the box face they were cut from and their colour.
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
    }
}

impl MeshData {
    /// The smallest box holding every vertex: (min, max).
    pub fn bounds(&self) -> (Vec3, Vec3) {
        if self.vertices.is_empty() {
            return (Vec3::ZERO, Vec3::ZERO);
        }
        let mut lo = Vec3::splat(f32::MAX);
        let mut hi = Vec3::splat(f32::MIN);
        for v in &self.vertices {
            lo = lo.min(Vec3::from(v.position));
            hi = hi.max(Vec3::from(v.position));
        }
        (lo, hi)
    }
}

pub(crate) fn build(shape: DrawShape) -> MeshData {
    match shape {
        DrawShape::Part(shape @ (PartShape::Block | PartShape::Wedge | PartShape::CornerWedge)) => {
            polyhedron(shape)
        }
        DrawShape::Part(PartShape::Cylinder) => cylinder(),
        DrawShape::Part(PartShape::Ball) => ball(),
        DrawShape::Head => head(),
        // Mesh files come with the scene (`Renderer::has_mesh`).
        DrawShape::File(_) => MeshData::default(),
        DrawShape::Truss(shape) => truss(shape),
    }
}

#[cfg(test)]
mod tests;
