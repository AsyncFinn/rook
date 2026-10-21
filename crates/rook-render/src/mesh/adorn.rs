//! The shapes only adornments draw ([`AdornShape`]): cones, tubes, flat
//! rings and squares, unit-sized and around Z. Only the square carries an
//! image (through its UVs); the others' vertices map from themselves.

use std::f32::consts::TAU;

use glam::Vec3;

use super::{MeshData, Vertex};
use crate::scene::AdornShape;

/// Segments around a circle (Studio's SelectionSphere ring has about 24).
const SEGMENTS: u32 = 32;

pub(super) fn adorn(shape: AdornShape) -> MeshData {
    match shape {
        AdornShape::Cone => cone(),
        AdornShape::Tube { hole } => tube(f32::from(hole) / 255.0),
        AdornShape::Ring { hole } => {
            let mut m = MeshData::default();
            ring(&mut m, f32::from(hole) / 255.0, 0.0, 1.0);
            m
        }
        AdornShape::Quad => quad(),
    }
}

fn quad() -> MeshData {
    let mut m = MeshData::default();
    let n = -Vec3::Z;
    // Seen from -Z, +X is on the left: the image's left edge (u = 0) is
    // there, its top (v = 0) at +Y.
    let corner = |x: f32, y: f32| Vertex::plain([x, y, 0.0], n.to_array(), [0.5 - x, 0.5 - y]);
    m.vertices.extend([
        corner(0.5, 0.5),
        corner(-0.5, 0.5),
        corner(-0.5, -0.5),
        corner(0.5, -0.5),
    ]);
    // Counter-clockwise seen from -Z.
    m.indices.extend_from_slice(&[0, 3, 2, 0, 2, 1]);
    m
}

/// Point `i` of [`SEGMENTS`] around a circle of radius 0.5 in XY.
fn around(i: u32) -> Vec3 {
    let a = i as f32 / SEGMENTS as f32 * TAU;
    Vec3::new(a.cos(), a.sin(), 0.0) * 0.5
}

fn push(m: &mut MeshData, p: Vec3, n: Vec3) -> u32 {
    m.vertices.push(Vertex::plain(
        p.to_array(),
        n.normalize().to_array(),
        [0.0; 2],
    ));
    m.vertices.len() as u32 - 1
}

/// A flat ring at `z` facing `facing` (+1 or -1 along Z), its hole `hole`
/// of its diameter.
fn ring(m: &mut MeshData, hole: f32, z: f32, facing: f32) {
    let n = Vec3::Z * facing;
    for i in 0..SEGMENTS {
        let (a, b) = (around(i), around(i + 1));
        let z = Vec3::Z * z;
        let quad = [
            push(m, a * hole + z, n),
            push(m, a + z, n),
            push(m, b + z, n),
            push(m, b * hole + z, n),
        ];
        // Counter-clockwise seen from the side it faces; a disc's inner
        // corners meet, leaving one triangle.
        let [p, q, r, s] = if facing > 0.0 {
            quad
        } else {
            [quad[3], quad[2], quad[1], quad[0]]
        };
        m.indices.extend_from_slice(&[p, q, r]);
        if hole > 0.0 {
            m.indices.extend_from_slice(&[p, r, s]);
        }
    }
}

/// The round side of a cylinder along Z of diameter `scale`, facing out
/// (`inward` false) or in.
fn side(m: &mut MeshData, scale: f32, inward: bool) {
    for i in 0..SEGMENTS {
        let (a, b) = (around(i) * scale, around(i + 1) * scale);
        let (na, nb) = if inward { (-a, -b) } else { (a, b) };
        let quad = [
            push(m, a - Vec3::Z * 0.5, na),
            push(m, b - Vec3::Z * 0.5, nb),
            push(m, b + Vec3::Z * 0.5, nb),
            push(m, a + Vec3::Z * 0.5, na),
        ];
        let [p, q, r, s] = if inward {
            [quad[3], quad[2], quad[1], quad[0]]
        } else {
            quad
        };
        m.indices.extend_from_slice(&[p, q, r, p, r, s]);
    }
}

fn tube(hole: f32) -> MeshData {
    let mut m = MeshData::default();
    side(&mut m, 1.0, false);
    if hole > 0.0 {
        side(&mut m, hole, true);
    }
    ring(&mut m, hole, 0.5, 1.0);
    ring(&mut m, hole, -0.5, -1.0);
    m
}

fn cone() -> MeshData {
    let mut m = MeshData::default();
    let tip = -Vec3::Z * 0.5;
    for i in 0..SEGMENTS {
        let (a, b) = (around(i) + Vec3::Z * 0.5, around(i + 1) + Vec3::Z * 0.5);
        // The side's normal leans towards the tip: radius 0.5 over height 1.
        let n = |p: Vec3| Vec3::new(p.x, p.y, -0.25);
        let (pa, pb) = (push(&mut m, a, n(a)), push(&mut m, b, n(b)));
        let mid = (around(i) + around(i + 1)) * 0.5;
        let pt = push(&mut m, tip, n(mid));
        m.indices.extend_from_slice(&[pt, pb, pa]);
    }
    ring(&mut m, 0.0, 0.5, 1.0);
    m
}
