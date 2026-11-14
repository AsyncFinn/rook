//! Cutting a drawn chunk's surface to its box (Sutherland-Hodgman), so
//! neighbors meet on the planes between them. The box is half open: a face
//! lying on a shared plane belongs to one chunk only.

use glam::{BVec3, Vec3};
use rustc_hash::FxHashMap;

use super::nets::Mesher;

/// A plane of the box: an axis, where on it, and whether the box lies
/// above it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct Plane {
    pub(super) axis: usize,
    pub(super) at: f32,
    pub(super) above: bool,
}

impl Plane {
    fn keeps(self, p: Vec3) -> bool {
        if self.above {
            p[self.axis] >= self.at
        } else {
            p[self.axis] < self.at
        }
    }
}

/// The planes of the box from `lo` to `hi`, on the axes in `axes`.
pub(super) fn box_planes(lo: Vec3, hi: Vec3, axes: BVec3) -> Vec<Plane> {
    (0..3)
        .filter(|&a| axes.test(a))
        .flat_map(|axis| {
            [
                Plane {
                    axis,
                    at: lo[axis],
                    above: true,
                },
                Plane {
                    axis,
                    at: hi[axis],
                    above: false,
                },
            ]
        })
        .collect()
}

impl Mesher<'_> {
    /// Cut the triangles to `planes`, dropping what lies outside them.
    pub(super) fn clip(&mut self, planes: &[Plane]) {
        let mut cuts: FxHashMap<(u32, u32, usize), u32> = FxHashMap::default();
        let triangles = std::mem::take(&mut self.triangles);
        let mut polygon = Vec::new();
        let mut next = Vec::new();
        for (t, material) in triangles {
            polygon.clear();
            polygon.extend(t);
            for (n, &plane) in planes.iter().enumerate() {
                next.clear();
                for k in 0..polygon.len() {
                    let (a, b) = (polygon[k], polygon[(k + 1) % polygon.len()]);
                    let (pa, pb) = (self.positions[a as usize], self.positions[b as usize]);
                    let (ka, kb) = (plane.keeps(pa), plane.keeps(pb));
                    if ka {
                        next.push(a);
                    }
                    if ka != kb {
                        next.push(self.cut(a, b, plane, n, &mut cuts));
                    }
                }
                next.dedup();
                if next.len() > 1 && next.first() == next.last() {
                    next.pop();
                }
                std::mem::swap(&mut polygon, &mut next);
                if polygon.len() < 3 {
                    break;
                }
            }
            for k in 1..polygon.len().saturating_sub(1) {
                self.triangles
                    .push(([polygon[0], polygon[k], polygon[k + 1]], material));
            }
        }
    }

    /// The vertex where the edge from vertex `a` to `b` crosses `plane`
    /// (the `n`th): an end lying on it, or a new vertex between them.
    fn cut(
        &mut self,
        a: u32,
        b: u32,
        plane: Plane,
        n: usize,
        cuts: &mut FxHashMap<(u32, u32, usize), u32>,
    ) -> u32 {
        for v in [a, b] {
            if self.positions[v as usize][plane.axis] == plane.at {
                return v;
            }
        }
        let key = (a.min(b), a.max(b), n);
        if let Some(&v) = cuts.get(&key) {
            return v;
        }
        let (pa, pb) = (self.positions[a as usize], self.positions[b as usize]);
        let (a, b) = if pa.to_array() <= pb.to_array() {
            (a, b)
        } else {
            (b, a)
        };
        let (pa, pb) = (self.positions[a as usize], self.positions[b as usize]);
        let t = (plane.at - pa[plane.axis]) / (pb[plane.axis] - pa[plane.axis]);
        let mut p = pa + (pb - pa) * t;
        p[plane.axis] = plane.at;
        let normal = self.normals[a as usize]
            .normalize_or_zero()
            .lerp(self.normals[b as usize].normalize_or_zero(), t);
        let material = self.materials[if t < 0.5 { a } else { b } as usize];
        let v = self.positions.len() as u32;
        self.positions.push(p);
        self.normals.push(normal);
        self.materials.push(material);
        cuts.insert(key, v);
        v
    }

    /// The edges of the triangles lying on the plane `at` across `axis`
    /// that no other triangle shares, each as its triangle runs it: where
    /// the clipped surface ends on that side of the box.
    pub(super) fn ends_on(&self, axis: usize, at: f32) -> Vec<(u32, u32)> {
        let on = |v: u32| self.positions[v as usize][axis] == at;
        let edges: rustc_hash::FxHashSet<(u32, u32)> = self
            .triangles
            .iter()
            .flat_map(|(t, _)| [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])])
            .collect();
        let mut out: Vec<(u32, u32)> = edges
            .iter()
            .filter(|&&(a, b)| on(a) && on(b) && !edges.contains(&(b, a)))
            .copied()
            .collect();
        out.sort_unstable();
        out
    }
}
