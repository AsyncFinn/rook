//! Closing where a ring meets a coarser one: both surfaces end on the
//! plane between them ([`super::clip`]), and the finer chunk fills, in that
//! plane, where their outlines disagree, so no crack shows whichever way
//! the surface faces (`docs/behavior/terrain/detail.md#how-rook-does-it`).

use glam::{IVec3, Vec3};
use rustc_hash::FxHashMap;
use spade::handles::{FixedDirectedEdgeHandle, FixedVertexHandle};
use spade::{ConstrainedDelaunayTriangulation, Point2, Triangulation};

use super::clip::{Plane, box_planes};
use super::nets::{Block, Keep, Mesher, Pass};
use super::{DIAGONALS, PAD, chunk_origin_at};
use crate::TerrainMaterial;
use crate::grid::{CHUNK_VOXELS, Grid};

/// Sides of a chunk across X and Z, as bits of [`Sides::DIRECTIONS`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash)]
pub struct Sides(u8);

impl Sides {
    pub const NONE: Sides = Sides(0);

    /// Each side's outward direction: -X, +X, -Z, +Z.
    pub const DIRECTIONS: [IVec3; 4] = [IVec3::NEG_X, IVec3::X, IVec3::NEG_Z, IVec3::Z];

    /// These sides and side `i`.
    pub const fn with(self, i: usize) -> Self {
        Sides(self.0 | 1 << i)
    }

    pub const fn has(self, i: usize) -> bool {
        self.0 & 1 << i != 0
    }

    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }
}

/// Which of the two rings an outline is of.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Ring {
    Finer = 0,
    Coarser = 1,
}

/// An edge where a ring's clipped surface ends on the plane, as its
/// triangle runs it, with its ends' shading.
#[derive(Debug, Clone, Copy)]
struct End {
    ends: [Corner; 2],
    /// The finer chunk's own vertices, for its edges.
    vertices: Option<[u32; 2]>,
}

/// An end of an outline's edge, in the finer chunk's frame.
#[derive(Debug, Clone, Copy)]
struct Corner {
    position: Vec3,
    normal: Vec3,
    material: TerrainMaterial,
}

/// The axes across a side facing `dir`, `u` then `v`, turning counter-
/// clockwise seen from that side (`u × v = dir`).
fn across(dir: IVec3) -> (usize, usize) {
    match (dir.x, dir.z) {
        (1, _) => (1, 2),
        (-1, _) => (2, 1),
        (_, 1) => (0, 1),
        _ => (1, 0),
    }
}

impl Mesher<'_> {
    /// Close the chunk's `sides` against the surface of `coarser`, the
    /// next level's voxels. The chunk is `chunk` of level `level`, its
    /// triangles already clipped to its box.
    pub(super) fn add_seams(&mut self, coarser: &Grid, chunk: IVec3, level: u8, sides: Sides) {
        // Every side's outline before any closure: a closure's edges on the
        // corner between two sides aren't the surface ending there.
        let outlines: Vec<(IVec3, Vec<End>)> = Sides::DIRECTIONS
            .iter()
            .enumerate()
            .filter(|&(i, _)| sides.has(i))
            .map(|(_, &dir)| (dir, self.outline(dir)))
            .collect();
        for (dir, fine) in outlines {
            self.close(coarser, chunk, level, dir, &fine);
        }
    }

    /// Where the chunk's own surface ends on its side facing `dir`.
    fn outline(&self, dir: IVec3) -> Vec<End> {
        let axis = if dir.x != 0 { 0 } else { 2 };
        let at = if dir[axis] > 0 {
            CHUNK_VOXELS as f32 * self.scale
        } else {
            0.0
        };
        self.ends_on(axis, at)
            .into_iter()
            .map(|(a, b)| End {
                ends: [a, b].map(|i| Corner {
                    position: self.positions[i as usize],
                    normal: self.normals[i as usize].normalize_or_zero(),
                    material: self.materials[i as usize],
                }),
                vertices: Some([a, b]),
            })
            .collect()
    }

    fn close(&mut self, coarser: &Grid, chunk: IVec3, level: u8, dir: IVec3, fine: &[End]) {
        let axis = if dir.x != 0 { 0 } else { 2 };
        let side = CHUNK_VOXELS as f32 * self.scale;
        let at = if dir[axis] > 0 { side } else { 0.0 };
        let (u, v) = across(dir);
        let coarse = coarse_ends(coarser, chunk, level, dir, self.scale);
        let flat = |p: Vec3| Point2::new(f64::from(p[u]), f64::from(p[v]));
        let mut cdt: ConstrainedDelaunayTriangulation<Point2<f64>> =
            ConstrainedDelaunayTriangulation::new();
        let mut corners: FxHashMap<FixedVertexHandle, (Corner, Option<u32>)> = FxHashMap::default();
        let s = f64::from(side);
        for p in [[0.0, 0.0], [s, 0.0], [0.0, s], [s, s]] {
            // Finite corners always insert.
            let _ = cdt.insert(Point2::new(p[0], p[1]));
        }
        let mut lines: Vec<Line> = Vec::new();
        for (ring, ends) in [(Ring::Finer, fine), (Ring::Coarser, &coarse[..])] {
            for e in ends.iter() {
                let mut handles = [None; 2];
                for k in 0..2 {
                    let c = e.ends[k];
                    let p = flat(c.position);
                    let p = Point2::new(p.x.clamp(0.0, s), p.y.clamp(0.0, s));
                    let Ok(h) = cdt.insert(p) else { continue };
                    corners.entry(h).or_insert((c, e.vertices.map(|v| v[k])));
                    handles[k] = Some(h);
                }
                if let [Some(a), Some(b)] = handles
                    && a != b
                {
                    lines.push((a, b, ring, e.vertices));
                }
            }
        }
        for &(a, b, _, _) in &lines {
            cdt.add_constraint_and_split(a, b, |p| p);
        }
        let (along, splits) = trace(&cdt, &lines);
        let states = [Ring::Finer, Ring::Coarser].map(|ring| {
            fill(&cdt, &along, ring).unwrap_or_else(|| {
                let solid = self.uniform(coarser, chunk, dir, ring);
                vec![Some(solid); cdt.num_all_faces()]
            })
        });
        // Vertices for the closure: the chunk's own where it has them.
        let mut index: FxHashMap<FixedVertexHandle, u32> = FxHashMap::default();
        let attribute = |h: FixedVertexHandle| -> Corner {
            let p = cdt.vertex(h).position();
            if let Some((c, _)) = corners.get(&h) {
                return *c;
            }
            // A crossing of two outlines or a corner of the side: shaded as
            // the nearest end.
            let near = corners
                .values()
                .min_by(|x, y| {
                    let d = |c: &Corner| {
                        let q = flat(c.position);
                        (q.x - p.x).powi(2) + (q.y - p.y).powi(2)
                    };
                    d(&x.0).total_cmp(&d(&y.0))
                })
                .map(|(c, _)| *c);
            let mut position = Vec3::splat(at);
            position[u] = p.x as f32;
            position[v] = p.y as f32;
            Corner {
                position,
                normal: near.map_or(dir.as_vec3(), |c| c.normal),
                material: near.map_or(TerrainMaterial::Rock, |c| c.material),
            }
        };
        let mut vertex = |m: &mut Self, h: FixedVertexHandle| -> u32 {
            if let Some(&i) = index.get(&h) {
                return i;
            }
            let i = match corners.get(&h) {
                Some((_, Some(own))) => *own,
                _ => {
                    let c = attribute(h);
                    m.positions.push(c.position);
                    m.normals.push(c.normal);
                    m.materials.push(c.material);
                    (m.positions.len() - 1) as u32
                }
            };
            index.insert(h, i);
            i
        };
        // The chunk's own edges a crossing split: their triangles fanned
        // through the crossings, so the closure meets them vertex to vertex.
        for (ends, through) in splits {
            let Some(n) = self
                .triangles
                .iter()
                .position(|(t, _)| (0..3).any(|k| [t[k], t[(k + 1) % 3]] == ends))
            else {
                continue;
            };
            let (t, material) = self.triangles.swap_remove(n);
            let k = (0..3)
                .find(|&k| [t[k], t[(k + 1) % 3]] == ends)
                .expect("found");
            let apex = t[(k + 2) % 3];
            let mut chain = vec![ends[0]];
            chain.extend(through.iter().map(|&h| vertex(self, h)));
            chain.push(ends[1]);
            for w in chain.windows(2) {
                self.triangles.push(([w[0], w[1], apex], material));
            }
        }
        for face in cdt.inner_faces() {
            let n = face.fix().index();
            let (Some(finer), Some(coarser)) = (states[0][n], states[1][n]) else {
                continue;
            };
            if finer == coarser {
                continue;
            }
            let mut t = face.vertices().map(|h| vertex(self, h.fix()));
            // Counter-clockwise across the side faces out along `dir`:
            // toward the coarser ring, over what only the finer holds.
            if coarser {
                t.swap(1, 2);
            }
            let material = self.materials[t[0] as usize];
            self.triangles.push((t, material));
        }
    }

    /// Whether `ring` holds the whole side facing `dir` solid, for a side
    /// its outline never crosses: what the voxels on both sides of the
    /// plane agree on most, else what the chunk's own side holds.
    fn uniform(&self, coarser: &Grid, chunk: IVec3, dir: IVec3, ring: Ring) -> bool {
        let axis = if dir.x != 0 { 0 } else { 2 };
        let (u, v) = across(dir);
        let (grid, width, base) = match ring {
            Ring::Finer => (self.grid, CHUNK_VOXELS, chunk * CHUNK_VOXELS),
            Ring::Coarser => (coarser, CHUNK_VOXELS / 2, chunk * CHUNK_VOXELS / 2),
        };
        // The voxel inside the chunk at the plane, and the one past it.
        let mut inside = base;
        if dir[axis] > 0 {
            inside[axis] += width - 1;
        }
        let (mut agree, mut solid, mut own) = (0, 0, 0);
        for a in 0..width {
            for b in 0..width {
                let mut p = inside;
                p[u] += a;
                p[v] += b;
                let here = Pass::Solid.inside(grid.voxel(p));
                let past = Pass::Solid.inside(grid.voxel(p + dir));
                own += i32::from(here);
                if here == past {
                    agree += 1;
                    solid += i32::from(here);
                }
            }
        }
        if agree > 0 {
            2 * solid > agree
        } else {
            2 * own > width * width
        }
    }
}

/// Where the coarser ring's surface ends on the side of chunk `chunk` of
/// level `level` facing `dir`, within that side, in the chunk's frame: its
/// chunk's surface there, cut as that chunk cuts it.
fn coarse_ends(coarser: &Grid, chunk: IVec3, level: u8, dir: IVec3, scale: f32) -> Vec<End> {
    let axis = if dir.x != 0 { 0 } else { 2 };
    let (u, v) = across(dir);
    let owner = (chunk + dir).div_euclid(IVec3::splat(2));
    let offset = chunk_origin_at(owner, level + 1) - chunk_origin_at(chunk, level);
    let first = owner * CHUNK_VOXELS - IVec3::splat(PAD);
    let block = Block::read(coarser, first);
    let coarse = scale * 2.0;
    let mut mesher = Mesher::new(coarser, &block, Pass::Solid, coarse, first);
    // The plane and the finer chunk's side, in the coarser chunk's frame.
    let side = CHUNK_VOXELS as f32 * scale;
    let at = if dir[axis] > 0 { side } else { 0.0 } - offset[axis];
    let lo = -offset;
    let hi = lo + Vec3::splat(side);
    // The quads that can reach that part of the plane.
    let voxel = |s: f32| (s / coarse).round() as i32 + PAD;
    let mut from = IVec3::ZERO;
    let mut to = IVec3::ZERO;
    from[axis] = voxel(at) - 2;
    to[axis] = voxel(at) + 1;
    for t in [u, v] {
        from[t] = voxel(lo[t]) - 2;
        to[t] = voxel(hi[t]) + 1;
    }
    mesher.run(DIAGONALS, Keep::Within(from, to));
    let mut planes = vec![Plane {
        axis,
        at,
        above: dir[axis] > 0,
    }];
    planes.extend(box_planes(
        lo,
        hi,
        glam::BVec3::new(axis != 0, true, axis != 2),
    ));
    mesher.clip(&planes);
    mesher
        .ends_on(axis, at)
        .into_iter()
        .map(|(a, b)| End {
            ends: [a, b].map(|i| Corner {
                position: mesher.positions[i as usize] + offset,
                normal: mesher.normals[i as usize].normalize_or_zero(),
                material: mesher.materials[i as usize],
            }),
            vertices: None,
        })
        .collect()
}

/// An outline's edge between two of the triangulation's vertices, of
/// `ring`, and the finer chunk's own vertices at its ends if it has them.
type Line = (FixedVertexHandle, FixedVertexHandle, Ring, Option<[u32; 2]>);

/// Each constraint edge's outlines' sums, by ring ([`trace`]).
type Sums = FxHashMap<FixedDirectedEdgeHandle, [i8; 2]>;

/// One of the finer chunk's edges, by its two vertices, and the vertices
/// crossings split it at, in order.
type Split = ([u32; 2], Vec<FixedVertexHandle>);

/// Each constraint edge's outlines, by the sum of their directions along
/// its first directed edge, and the chunk's own edges that crossings split,
/// with the vertices they were split at.
fn trace(
    cdt: &ConstrainedDelaunayTriangulation<Point2<f64>>,
    lines: &[Line],
) -> (Sums, Vec<Split>) {
    let mut along = Sums::default();
    let mut splits = Vec::new();
    for &(a, b, ring, vertices) in lines {
        let path = walk(cdt, a, b);
        for w in path.windows(2) {
            let Some(e) = cdt.get_edge_from_neighbors(w[0], w[1]) else {
                continue;
            };
            let first = e.as_undirected().as_directed();
            let sign = if first.fix() == e.fix() { 1 } else { -1 };
            along.entry(first.fix()).or_default()[ring as usize] += sign;
        }
        if let Some(v) = vertices
            && path.len() > 2
        {
            splits.push((v, path[1..path.len() - 1].to_vec()));
        }
    }
    (along, splits)
}

/// The vertices along the constraint from `a` to `b`, which crossings
/// with other constraints may have split.
fn walk(
    cdt: &ConstrainedDelaunayTriangulation<Point2<f64>>,
    a: FixedVertexHandle,
    b: FixedVertexHandle,
) -> Vec<FixedVertexHandle> {
    let target = cdt.vertex(b).position();
    let mut path = vec![a];
    let mut at = a;
    while at != b && path.len() <= cdt.num_vertices() {
        let here = cdt.vertex(at).position();
        let (dx, dy) = (target.x - here.x, target.y - here.y);
        let next = cdt
            .vertex(at)
            .out_edges()
            .filter(|e| e.is_constraint_edge())
            .map(|e| e.to())
            .max_by(|x, y| {
                let score = |h: &spade::handles::VertexHandle<'_, Point2<f64>, _, _, _>| {
                    let p = h.position();
                    let (ex, ey) = (p.x - here.x, p.y - here.y);
                    (ex * dx + ey * dy) / (ex.hypot(ey) * dx.hypot(dy)).max(f64::MIN_POSITIVE)
                };
                score(x).total_cmp(&score(y))
            });
        let Some(next) = next else { break };
        at = next.fix();
        path.push(at);
    }
    path
}

/// Which faces `ring` holds solid, by the winding number of its outline
/// around each face (the nonzero rule of polygon fills): its solid lies
/// right of the finer ring's edges and left of the coarser ring's, seen
/// from the finer side's outside, so crossing one of its edges steps the
/// count up or down. `None` when its outline doesn't cross the side.
///
/// Counting rather than marking each edge's two sides solid and air keeps
/// an outline that crosses itself to its own faces: a surface folded
/// over (Rock's shifted vertices on a thin spike) cuts the plane in a
/// figure eight whose small loop runs backwards, and marking that loop's
/// outside solid would carry solid across the whole empty side.
fn fill(
    cdt: &ConstrainedDelaunayTriangulation<Point2<f64>>,
    along: &Sums,
    ring: Ring,
) -> Option<Vec<Option<bool>>> {
    let r = ring as usize;
    // Crossing an edge from its left to its right changes the count by
    // this times the ring's sum along it.
    let step = match ring {
        Ring::Coarser => -1,
        Ring::Finer => 1,
    };
    // The ring's sum along `e`, walked as `e` runs.
    let sum = |e: FixedDirectedEdgeHandle| -> i32 {
        let edge = cdt.directed_edge(e);
        let first = edge.as_undirected().as_directed();
        let s = along.get(&first.fix()).map_or(0, |s| i32::from(s[r]));
        if first.fix() == e { s } else { -s }
    };
    if !along.values().any(|s| s[r] != 0) {
        return None;
    }
    // The count around each face, from the first: the side is one square,
    // so every inner face is reached.
    let mut winding: Vec<Option<i32>> = vec![None; cdt.num_all_faces()];
    let start = cdt.inner_faces().next()?.fix();
    winding[start.index()] = Some(0);
    let mut queue = vec![start];
    while let Some(f) = queue.pop() {
        let w = winding[f.index()]?;
        for e in cdt.face(f).adjacent_edges() {
            let Some(next) = e.rev().face().as_inner() else {
                continue;
            };
            let slot = &mut winding[next.fix().index()];
            if slot.is_none() {
                *slot = Some(w + step * sum(e.fix()));
                queue.push(next.fix());
            }
        }
    }
    // The count outside the ring's solid: what most of its outline, by
    // length, has on its empty side. An outline that never crosses itself
    // agrees all along; a backwards loop is outvoted.
    let mut votes: FxHashMap<i32, f64> = FxHashMap::default();
    for (&e, sums) in along {
        if sums[r] == 0 {
            continue;
        }
        let edge = cdt.directed_edge(e);
        let (left, right) = (edge.face(), edge.rev().face());
        let counts = match (left.is_outer(), right.is_outer()) {
            (false, _) => winding[left.fix().index()].map(|w| (w, w + step * sum(e))),
            (true, false) => winding[right.fix().index()].map(|w| (w - step * sum(e), w)),
            (true, true) => None,
        };
        if let Some((a, b)) = counts {
            *votes.entry(a.min(b)).or_default() += edge.length_2().sqrt();
        }
    }
    let outside = votes
        .into_iter()
        .max_by(|a, b| a.1.total_cmp(&b.1).then(b.0.cmp(&a.0)))?
        .0;
    Some(
        winding
            .into_iter()
            .map(|w| w.map(|w| w > outside))
            .collect(),
    )
}

#[cfg(test)]
mod tests;
