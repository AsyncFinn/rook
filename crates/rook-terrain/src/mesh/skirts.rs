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
//! the ground. Studio's rings showed no cracks at their steps
//! (`docs/behavior/terrain/detail.md`).

use rustc_hash::FxHashSet;

use super::nets::Mesher;

/// How far a skirt hangs, in the chunk's voxels: to where the next
/// coarser level's surface can lie (half its voxel, one of these).
const DEPTH: f32 = 1.0;

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
        let depth = DEPTH * self.scale;
        let mut hung = rustc_hash::FxHashMap::default();
        let mut below = |m: &mut Self, i: u32| -> u32 {
            *hung.entry(i).or_insert_with(|| {
                let n = m.normals[i as usize];
                m.positions
                    .push(m.positions[i as usize] - glam::Vec3::Y * depth);
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
}
