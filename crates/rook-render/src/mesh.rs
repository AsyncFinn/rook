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

use crate::scene::{DrawShape, NormalId, PartShape, TrussShape};

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
    /// (mesh files: the face is found from the normal).
    pub face: u32,
}

/// No box face of its own: the shader finds one from the normal.
pub(crate) const NO_FACE: u32 = 255;

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
/// edges. The flat ends are the Left and Right faces.
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
                let box_point = Vec3::new(x, g.y, g.z) * 0.5;
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
/// user's Roblox install (the Head MeshType draws that file; ROADMAP M4).
/// The facts measured, and nothing else from the file, are these: a solid
/// of revolution about Y with 36 segments starting 5 degrees off +X; its
/// profile is a cylinder of radius R = 0.6012 whose straight side runs
/// from -R/2 to R/2, with each rim rounded by a quarter circle of radius R/2
/// in 4 steps, and flat round caps. Normals are smooth around and along the
/// whole profile, so the rim blends into the caps. Its UVs (measured in
/// M4 step 4): the vertices of the front quarter, facing within 45 degrees
/// of -Z, have UV (0.5 - x / 2R, 0.5 - y / 2R), a straight projection onto
/// the front; the other 417 of its 517 vertices have (0, 0), and the
/// quarter's edges are seams with both. Rook keeps those rules and
/// triangulates the caps as fans (a grid in the file).
///
/// From asset fetching on (M4 steps 2 and 3), the user's own install can
/// supply the real file: load it with the mesh reader and use it in place
/// of this for [`DrawShape::Head`].
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

/// A mesh read from a file, as drawn.
pub(crate) fn from_file(m: &rook_content::mesh::Mesh) -> MeshData {
    MeshData {
        vertices: m
            .positions
            .iter()
            .zip(&m.normals)
            .zip(&m.uvs)
            .map(|((&position, &normal), &uv)| Vertex::plain(position, normal, uv))
            .collect(),
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
mod tests {
    use super::*;

    /// Every face's normal points away from the centre.
    #[test]
    fn normals_point_outward() {
        for shape in DrawShape::ALL {
            let m = build(shape);
            for tri in m.indices.chunks(3) {
                let [a, b, c] =
                    [tri[0], tri[1], tri[2]].map(|i| Vec3::from(m.vertices[i as usize].position));
                let face_normal = (b - a).cross(c - a);
                let centroid = (a + b + c) / 3.0;
                assert!(
                    face_normal.dot(centroid) > -1e-6,
                    "{shape:?}: inward-facing triangle"
                );
            }
        }
    }

    /// The user's own head mesh, when an install is found, faces outward
    /// like Rook's (same winding) and is centred like it.
    #[test]
    fn installed_head_matches_rooks_frame() {
        let Some(install) = rook_content::install::newest() else {
            eprintln!("skipped: no Roblox install found");
            return;
        };
        let Ok(bytes) = std::fs::read(install.content[0].join("avatar/heads/head.mesh")) else {
            eprintln!("skipped: no head.mesh in the install");
            return;
        };
        let m = from_file(&rook_content::mesh::Mesh::decode(&bytes).unwrap());
        let outward = m
            .indices
            .chunks(3)
            .filter(|tri| {
                let [a, b, c] =
                    [tri[0], tri[1], tri[2]].map(|i| Vec3::from(m.vertices[i as usize].position));
                (b - a).cross(c - a).dot((a + b + c) / 3.0) > -1e-6
            })
            .count();
        assert_eq!(outward, m.indices.len() / 3);
        let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
        for v in &m.vertices {
            lo = lo.min(Vec3::from(v.position));
            hi = hi.max(Vec3::from(v.position));
        }
        assert!((lo + hi).abs().max_element() < 1e-3, "{lo} {hi}");
    }

    /// The round shapes keep each vertex's grid point on the box face it
    /// belongs to, with the measured vertex and triangle counts.
    #[test]
    fn round_shapes_map_from_their_box_faces() {
        for (shape, triangles) in [(PartShape::Ball, 432), (PartShape::Cylinder, 96)] {
            let m = build(DrawShape::Part(shape));
            let side_triangles = m.indices.len() / 3;
            if shape == PartShape::Ball {
                assert_eq!((m.vertices.len(), side_triangles), (294, triangles));
            } else {
                // 24 side quads; the flat ends as 22-triangle fans (the
                // export's are 24-triangle fans around a centre).
                assert_eq!(side_triangles, 48 + 2 * 22);
            }
            for v in &m.vertices {
                let face = NormalId::ALL[v.face as usize];
                let b = Vec3::from(v.box_point);
                assert!((b.dot(face.normal()) - 0.5).abs() < 1e-5, "{shape:?} {b}");
                assert!(b.abs().max_element() <= 0.5 + 1e-5);
                assert!(
                    (Vec3::from(v.position).length() - 0.5).abs() < 1e-5
                        || shape == PartShape::Cylinder
                );
            }
        }
        // The cylinder's side corners: atan(1/3) from the axis of its face.
        let m = build(DrawShape::Part(PartShape::Cylinder));
        let angle = (1.0f32 / 3.0).atan();
        assert!(m.vertices.iter().any(|v| {
            let p = Vec3::from(v.position);
            (p.z.atan2(p.y) - angle).abs() < 1e-5 && p.x > 0.0
        }));
    }

    /// The head's UVs: the front quarter projected straight on, the rest
    /// at (0, 0).
    #[test]
    fn head_uvs_project_the_front_quarter() {
        let m = build(DrawShape::Head);
        let mut front = 0;
        for v in &m.vertices {
            if v.uv == [0.0; 2] {
                continue;
            }
            front += 1;
            let [x, y, z] = v.position;
            // In the front quarter, or a cap's centre.
            assert!(x.abs() <= -z + 1e-4, "{:?}", v.position);
            let span = 2.0 * HEAD_RADIUS;
            assert!((v.uv[0] - (0.5 - x / span)).abs() < 1e-6);
            assert!((v.uv[1] - (0.5 - y / span)).abs() < 1e-6);
        }
        assert!(front > 0);
        // The middle of the front maps to the middle of the image.
        assert!(m.vertices.iter().any(|v| v.uv[0] > 0.45 && v.uv[0] < 0.55));
    }

    /// The block's front face shows the image upright, unmirrored from the
    /// front: its top-left at +X, +Y.
    #[test]
    fn block_uvs_follow_studios_faces() {
        let m = build(DrawShape::Part(PartShape::Block));
        let front = m
            .vertices
            .iter()
            .find(|v| v.normal == [0.0, 0.0, -1.0] && v.position == [0.5, 0.5, -0.5])
            .unwrap();
        assert_eq!(front.uv, [0.0, 0.0]);
    }

    /// A truss's faces each face the way their normal says, and its bars sit
    /// where the export of a 2 × 4 × 2 truss has them: rails out to the
    /// 2-stud box over the whole length, and each brace's outline reaching
    /// 0.9634 studs from its segment's middle both across and along.
    #[test]
    fn truss_matches_the_export() {
        use rook_types::TrussStyle;
        let shape = TrussShape::new(TrussStyle::AlternatingSupports, 2);
        let m = build(DrawShape::Truss(shape));
        for tri in m.indices.chunks(3) {
            let v = [tri[0], tri[1], tri[2]].map(|i| m.vertices[i as usize]);
            let [a, b, c] = v.map(|v| Vec3::from(v.position));
            let n = (b - a).cross(c - a).normalize();
            assert!(n.dot(Vec3::from(v[0].normal)) > 0.999);
        }
        let (lo, hi) = m.bounds();
        assert_eq!(
            (lo, hi),
            (Vec3::new(-1.0, -2.0, -1.0), Vec3::new(1.0, 2.0, 1.0))
        );
        // The braces are the only faces with diagonal normals: on the
        // Front side of the lower segment they span x and y within
        // ±0.9634 of (0, -1).
        let brace: Vec<Vec3> = m
            .vertices
            .iter()
            .filter(|v| v.normal[0].abs() > 0.1 && v.normal[1].abs() > 0.1)
            .map(|v| Vec3::from(v.position))
            .filter(|p| p.y < 0.0 && p.z < 0.0)
            .collect();
        let reach = brace
            .iter()
            .map(|p| p.x.abs().max((p.y + 1.0).abs()))
            .fold(0.0, f32::max);
        assert!((reach - 0.9634).abs() < 1e-4, "{reach}");
        assert!(brace.iter().all(|p| (-0.9375..=-0.8125).contains(&p.z)));
        // The joint's rung is 0.5 thick: a top face at y = 0.25.
        assert!(m.vertices.iter().any(|v| v.normal == [0.0, 1.0, 0.0]
            && (v.position[1] - 0.25).abs() < 1e-6
            && v.position[2] == -1.0));
        // One segment: no braces.
        let one = build(DrawShape::Truss(TrussShape::new(
            TrussStyle::AlternatingSupports,
            1,
        )));
        assert!(
            one.vertices
                .iter()
                .all(|v| v.normal.iter().filter(|c| **c != 0.0).count() == 1)
        );
    }

    /// The head's measured outline: 1.2024 studs tall, and as wide as its
    /// 36 segments allow (R cos 5 degrees seen from the front or side).
    #[test]
    fn head_matches_the_measured_bounds() {
        let m = build(DrawShape::Head);
        let (mut lo, mut hi) = (Vec3::splat(f32::MAX), Vec3::splat(f32::MIN));
        for v in &m.vertices {
            lo = lo.min(Vec3::from(v.position));
            hi = hi.max(Vec3::from(v.position));
        }
        let wide = HEAD_RADIUS * 5.0f32.to_radians().cos();
        assert!(
            (hi - Vec3::new(wide, HEAD_RADIUS, wide))
                .abs()
                .max_element()
                < 1e-5
        );
        assert!((lo + hi).abs().max_element() < 1e-5);
        for v in &m.vertices {
            assert!((Vec3::from(v.normal).length() - 1.0).abs() < 1e-5);
        }
    }
}
