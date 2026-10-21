//! A TrussPart's bars.

use super::*;

/// One face of a box centered at `center` whose edges run along `axes` (a
/// right-handed frame) with half extents `half`: the face on the `sign`
/// side of axis `k`, flat, wound counter-clockwise seen from outside.
fn box_face(m: &mut MeshData, center: Vec3, axes: [Vec3; 3], half: Vec3, k: usize, sign: f32) {
    let normal = axes[k] * sign;
    let (mut a, mut b) = (k + 1, k + 2);
    if sign < 0.0 {
        std::mem::swap(&mut a, &mut b);
    }
    let a = axes[a % 3] * half[a % 3];
    let b = axes[b % 3] * half[b % 3];
    let c = center + normal * half[k];
    let base = m.vertices.len() as u32;
    for p in [c - a - b, c + a - b, c + a + b, c - a + b] {
        m.vertices
            .push(Vertex::plain(p.to_array(), normal.to_array(), [0.0; 2]));
    }
    m.indices
        .extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
}

/// A TrussPart's bars, along Y, in studs, centered on the origin, built from
/// the facts `rook_types::TrussShape` documents (measured from Studio's
/// OBJ export of a 2 × 2 × 2 and a 2 × 4 × 2 truss, 2026-10-13): four
/// 0.25-stud rails at the corners, a 0.25-stud rung across each side at
/// the truss's ends and a 0.5-stud one where two segments meet, and each
/// segment's braces. Faces hidden inside the bars where they meet are left
/// out (the rungs' and braces' ends); normals are flat. The export maps its
/// UVs into the surface-pattern strip, which Studio doesn't draw on
/// trusses, so the bars have none.
pub(super) fn truss(shape: TrussShape) -> MeshData {
    let mut m = MeshData::default();
    let bar = TrussShape::BAR;
    let half_length = shape.length() / 2.0;
    // From the axis to the center of a rail, a rung or a brace.
    let side_at = TrussShape::SEGMENT / 2.0 - bar / 2.0;
    let frame = [Vec3::X, Vec3::Y, Vec3::Z];
    for (x, z) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
        let center = Vec3::new(x, 0.0, z) * side_at;
        let half = Vec3::new(bar / 2.0, half_length, bar / 2.0);
        for k in 0..3 {
            for sign in [-1.0, 1.0] {
                box_face(&mut m, center, frame, half, k, sign);
            }
        }
    }
    // The rungs' bands along the truss: (center, half thickness).
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
            let center = normal * side_at + Vec3::Y * y;
            let half = across * (TrussShape::SEGMENT / 2.0 - bar)
                + Vec3::Y * thickness
                + normal.abs() * (bar / 2.0);
            for k in (0..3).filter(|&k| frame[k] != across) {
                for sign in [-1.0, 1.0] {
                    box_face(&mut m, center, frame, half, k, sign);
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
            let center = normal * side_at + Vec3::Y * middle;
            for k in 1..3 {
                for sign in [-1.0, 1.0] {
                    box_face(&mut m, center, axes, half, k, sign);
                }
            }
        }
    }
    m
}
