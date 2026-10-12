//! Unit primitive meshes (extents -0.5..0.5) with flat or smooth normals.
//! Each is scaled by the part's size in the vertex shader.

use bytemuck::{Pod, Zeroable};
use glam::Vec3;

use crate::scene::PartShape;

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub(crate) struct Vertex {
    pub position: [f32; 3],
    pub normal: [f32; 3],
}

#[derive(Debug, Default)]
pub(crate) struct MeshData {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
}

impl MeshData {
    /// A flat polygon (convex, counter-clockwise seen from outside).
    fn polygon(&mut self, points: &[Vec3]) {
        let normal = (points[1] - points[0])
            .cross(points[2] - points[0])
            .normalize();
        let base = self.vertices.len() as u32;
        for p in points {
            self.vertices.push(Vertex {
                position: p.to_array(),
                normal: normal.to_array(),
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

pub(crate) fn build(shape: PartShape) -> MeshData {
    match shape {
        PartShape::Block | PartShape::Wedge | PartShape::CornerWedge => polyhedron(shape),
        PartShape::Cylinder => cylinder(),
        PartShape::Ball => ball(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every face's normal points away from the centre.
    #[test]
    fn normals_point_outward() {
        for shape in PartShape::ALL {
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
}
