//! Unit primitive meshes (extents -0.5..0.5) with flat or smooth normals,
//! and Rook's own head mesh. Each is scaled by the instance's size in the
//! vertex shader. UVs matter for the block (the empty-`MeshId` box draws a
//! mesh texture on every face) and the head (its face decal); the other
//! shapes only ever get images projected onto them.

use bytemuck::{Pod, Zeroable};
use glam::Vec3;

use crate::scene::{DrawShape, NormalId, PartShape};

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub(crate) struct Vertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
    pub uv: [f32; 2],
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
                position: p.to_array(),
                normal: normal.to_array(),
                uv: (at / extent).to_array(),
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

const SEGMENTS: u32 = 32;

/// Along X, radius 0.5.
fn cylinder() -> MeshData {
    let mut m = MeshData::default();
    let ring = |i: u32| {
        let a = i as f32 / SEGMENTS as f32 * std::f32::consts::TAU;
        (a.cos() * 0.5, a.sin() * 0.5)
    };
    // Side, smooth normals.
    for i in 0..=SEGMENTS {
        let (y, z) = ring(i);
        let n = Vec3::new(0.0, y, z).normalize();
        for x in [-0.5, 0.5] {
            m.vertices.push(Vertex {
                position: [x, y, z],
                normal: n.to_array(),
                uv: [0.0; 2],
            });
        }
    }
    for i in 0..SEGMENTS {
        let a = i * 2;
        m.indices
            .extend_from_slice(&[a, a + 2, a + 1, a + 1, a + 2, a + 3]);
    }
    // Caps.
    let caps: Vec<Vec3> = (0..SEGMENTS)
        .map(|i| {
            let (y, z) = ring(i);
            Vec3::new(0.5, y, z)
        })
        .collect();
    m.polygon(&caps);
    let back: Vec<Vec3> = caps
        .iter()
        .rev()
        .map(|p| Vec3::new(-0.5, p.y, p.z))
        .collect();
    m.polygon(&back);
    m
}

fn ball() -> MeshData {
    let mut m = MeshData::default();
    let (rings, segs) = (16u32, SEGMENTS);
    for r in 0..=rings {
        let theta = r as f32 / rings as f32 * std::f32::consts::PI;
        for s in 0..=segs {
            let phi = s as f32 / segs as f32 * std::f32::consts::TAU;
            let n = Vec3::new(
                theta.sin() * phi.cos(),
                theta.cos(),
                theta.sin() * phi.sin(),
            );
            m.vertices.push(Vertex {
                position: (n * 0.5).to_array(),
                normal: n.to_array(),
                uv: [0.0; 2],
            });
        }
    }
    for r in 0..rings {
        for s in 0..segs {
            let a = r * (segs + 1) + s;
            let b = a + segs + 1;
            m.indices.extend_from_slice(&[a, a + 1, b, a + 1, b + 1, b]);
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
        Vertex {
            position,
            normal: [a.cos() * cos, a.sin(), a.cos() * sin],
            uv: head_uv(front, position),
        }
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
            m.vertices.push(Vertex {
                position,
                normal: [0.0, ny, 0.0],
                uv: head_uv(front, position),
            });
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

/// A mesh read from a file, as drawn.
pub(crate) fn from_file(m: &rook_content::mesh::Mesh) -> MeshData {
    MeshData {
        vertices: m
            .positions
            .iter()
            .zip(&m.normals)
            .zip(&m.uvs)
            .map(|((&position, &normal), &uv)| Vertex {
                position,
                normal,
                uv,
            })
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
