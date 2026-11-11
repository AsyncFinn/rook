//! Skirts: where a chunk's surface ends at the chunk's side, a strip
//! hanging straight down from the open edge into the solid. Chunks of
//! different detail meet with their surfaces a little apart (a coarse
//! surface lies where the fine one does, give or take the shapes too small
//! for its voxels, half a voxel of it), and the strip of whichever side
//! lies higher closes the crack. Inside a ring, neighbors' surfaces meet
//! exactly and the strips stay hidden in the solid. The usual fix for
//! cracks between levels of a chunked surface, hung vertically as there
//! (Ulrich, "Rendering massive terrains using chunked level of detail
//! control", 2002); only from surfaces facing up, as one hung from an
//! underside would rise through a slab thinner than it and stand above
//! the ground. A strip stops at the solid's lowest voxel under it, so a
//! deep one never hangs out under thin ground. Studio's rings showed no
//! cracks at their steps (`docs/behavior/terrain/detail.md`).

use glam::{IVec3, Vec3};
use rustc_hash::FxHashSet;

use super::nets::Mesher;

/// How far a skirt hangs at most, in the chunk's voxels. On a slope the
/// two sides' edges lie apart across the seam as well as up or down,
/// which the slope turns into height. One voxel let the sky through on
/// steep hills at the seam of the 16- and 32-stud rings; three close it,
/// and four stood out of the coarser side's ground at some seams.
const DEPTH: i32 = 3;

impl Mesher<'_> {
    /// Hang a skirt from each edge of the chunk's upward-facing triangles
    /// that no other of them shares: a quad straight down, turned as the
    /// triangle is, of the vertices' own materials and normals so it draws
    /// in the surface's layers.
    pub(super) fn add_skirts(&mut self) {
        let edges: FxHashSet<(u32, u32)> = self
            .triangles
            .iter()
            .flat_map(|(t, _)| [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])])
            .collect();
        let positions = &self.positions;
        let up = |t: &[u32; 3]| {
            let p = t.map(|i| positions[i as usize]);
            (p[1] - p[0]).cross(p[2] - p[0]).y > 0.0
        };
        let open: Vec<((u32, u32), _)> = self
            .triangles
            .iter()
            .filter(|(t, _)| up(t))
            .flat_map(|(t, m)| [((t[0], t[1]), *m), ((t[1], t[2]), *m), ((t[2], t[0]), *m)])
            .filter(|((a, b), _)| !edges.contains(&(*b, *a)))
            .collect();
        let mut hung = rustc_hash::FxHashMap::default();
        let mut below = |m: &mut Self, i: u32| -> u32 {
            *hung.entry(i).or_insert_with(|| {
                let p = m.positions[i as usize];
                let n = m.normals[i as usize];
                m.positions.push(p - Vec3::Y * m.hang(p));
                m.normals.push(n);
                m.materials.push(m.materials[i as usize]);
                (m.positions.len() - 1) as u32
            })
        };
        for ((a, b), material) in open {
            let (a2, b2) = (below(self, a), below(self, b));
            // The neighbor the edge lacks would run b to a.
            self.triangles.push(([b, a, a2], material));
            self.triangles.push(([b, a2, b2], material));
        }
    }

    /// How far below `p`, a vertex of the surface on top of the solid, a
    /// skirt hangs: [`DEPTH`] voxels, or to the center of the solid's
    /// lowest voxel under it if that comes first.
    fn hang(&self, p: Vec3) -> f32 {
        let corner = self.origin + IVec3::splat(super::PAD);
        // Skirts hang from solids only.
        let solid = |v: IVec3| self.grid.voxel(v).is_solid();
        let mut v = corner + (p / self.scale).floor().as_ivec3();
        // The vertex lies in the voxel it tops or the one above it.
        if !solid(v) {
            v.y -= 1;
            if !solid(v) {
                return 0.0;
            }
        }
        let mut lowest = v;
        while v.y - lowest.y < DEPTH && solid(lowest - IVec3::Y) {
            lowest.y -= 1;
        }
        let bottom = ((lowest.y - corner.y) as f32 + 0.5) * self.scale;
        (p.y - bottom).clamp(0.0, DEPTH as f32 * self.scale)
    }
}
