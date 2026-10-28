//! Surface nets over one chunk's voxels: a vertex in each dual cell where
//! solid meets air, a quad across each crossing edge (see [`super`]).

use super::*;

/// Voxels read along each axis.
pub(super) const SPAN: i32 = CHUNK_VOXELS + 2 * PAD;
/// Dual cells along each axis.
const CELLS: i32 = SPAN - 1;
pub(super) const NONE: u32 = u32::MAX;

pub(crate) fn mesh_chunk_with(
    grid: &Grid,
    chunk: IVec3,
    detail: Detail,
    diagonals: [[bool; 2]; 3],
) -> ChunkMesh {
    let first = chunk * CHUNK_VOXELS - IVec3::splat(PAD);
    let scale = detail.voxel_studs();
    let mut voxels = Vec::new();
    grid.read_block(first, IVec3::splat(SPAN), &mut voxels);
    let block = Block { voxels };
    let mut surfaces = Vec::new();
    let mut draws = Vec::new();
    for pass in [Pass::Solid, Pass::Water] {
        if pass == Pass::Water && !block.voxels.iter().any(|v| v.is_wet()) {
            continue;
        }
        let mut mesher = Mesher {
            block: &block,
            pass,
            scale,
            origin: chunk * CHUNK_VOXELS - IVec3::splat(PAD),
            cells: vec![NONE; (CELLS * CELLS * CELLS) as usize],
            wet: match pass {
                Pass::Solid => Vec::new(),
                Pass::Water => wet_cells(&block),
            },
            positions: Vec::new(),
            normals: Vec::new(),
            materials: Vec::new(),
            triangles: Vec::new(),
        };
        mesher.run(diagonals);
        mesher.split(&mut surfaces);
        if detail.skirts && pass == Pass::Solid {
            mesher.add_skirts();
        }
        mesher.draws(&mut draws);
    }
    ChunkMesh {
        chunk,
        level: detail.level,
        surfaces,
        draws,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Pass {
    /// Solid voxels against air and water.
    Solid,
    /// Water against air: water voxels and the water solid voxels hold,
    /// with solids counted as inside, so water meets solid with no face.
    /// Only cells with water in them have a vertex ([`Mesher::wet`]).
    Water,
}

impl Pass {
    fn inside(self, v: Voxel) -> bool {
        match self {
            Pass::Solid => v.is_solid(),
            Pass::Water => v.material != TerrainMaterial::Air,
        }
    }

    /// How far toward its outside neighbor this inside voxel's surface
    /// crosses: for water, by the water it holds, whatever its solid
    /// occupancy (observed: Rock at 0, 0.25 and 0.5 occupancy holding the
    /// same water draws the same water box); a dry solid by its own
    /// occupancy (Inference: it lies under that solid's surface).
    fn fill(self, v: Voxel) -> f32 {
        match self {
            Pass::Water if v.is_solid() && v.water > 0 => v.water_fill(),
            _ => v.fill(),
        }
    }
}

pub(super) struct Block {
    voxels: Vec<Voxel>,
}

impl Block {
    /// Voxel `p` of the block (0 to `SPAN - 1` on each axis).
    fn at(&self, p: IVec3) -> Voxel {
        self.voxels[(p.x + SPAN * (p.y + SPAN * p.z)) as usize]
    }
}

/// The slot of the cell whose first corner is voxel `c` of the block.
fn cell(c: IVec3) -> usize {
    (c.x + CELLS * (c.y + CELLS * c.z)) as usize
}

/// Which of the block's cells have a corner holding water.
fn wet_cells(block: &Block) -> Vec<bool> {
    let mut wet = vec![false; (CELLS * CELLS * CELLS) as usize];
    for z in 0..CELLS {
        for y in 0..CELLS {
            for x in 0..CELLS {
                let c = IVec3::new(x, y, z);
                wet[cell(c)] =
                    (0..8).any(|i| block.at(c + IVec3::new(i & 1, i >> 1 & 1, i >> 2)).is_wet());
            }
        }
    }
    wet
}

/// The twelve edges of a cell, as pairs of corner offsets.
const CELL_EDGES: [(IVec3, IVec3); 12] = {
    const fn v(x: i32, y: i32, z: i32) -> IVec3 {
        IVec3::new(x, y, z)
    }
    [
        (v(0, 0, 0), v(1, 0, 0)),
        (v(0, 1, 0), v(1, 1, 0)),
        (v(0, 0, 1), v(1, 0, 1)),
        (v(0, 1, 1), v(1, 1, 1)),
        (v(0, 0, 0), v(0, 1, 0)),
        (v(1, 0, 0), v(1, 1, 0)),
        (v(0, 0, 1), v(0, 1, 1)),
        (v(1, 0, 1), v(1, 1, 1)),
        (v(0, 0, 0), v(0, 0, 1)),
        (v(1, 0, 0), v(1, 0, 1)),
        (v(0, 1, 0), v(0, 1, 1)),
        (v(1, 1, 0), v(1, 1, 1)),
    ]
};

pub(super) struct Mesher<'a> {
    pub(super) block: &'a Block,
    pub(super) pass: Pass,
    /// A voxel's edge in studs, at the level meshed.
    pub(super) scale: f32,
    /// The block's first voxel in the grid (seeds the shift).
    pub(super) origin: IVec3,
    /// Each cell's vertex, by cell, once made.
    pub(super) cells: Vec<u32>,
    /// Water only: which cells have water at a corner. A water quad is
    /// kept when its four cells do, so solids touching water carry the
    /// surface across them and dry terrain has none.
    pub(super) wet: Vec<bool>,
    pub(super) positions: Vec<Vec3>,
    pub(super) normals: Vec<Vec3>,
    /// Each vertex's material: its cell's ([`Mesher::dominant`]), or Water.
    pub(super) materials: Vec<TerrainMaterial>,
    /// The chunk's own triangles and their material.
    pub(super) triangles: Vec<([u32; 3], TerrainMaterial)>,
}

impl Mesher<'_> {
    /// Where voxel `p` of the block has its center, from the chunk's
    /// corner.
    fn center(&self, p: IVec3) -> Vec3 {
        ((p - IVec3::splat(PAD)).as_vec3() + 0.5) * self.scale
    }

    fn inside(&self, p: IVec3) -> bool {
        self.pass.inside(self.block.at(p))
    }

    /// The vertex of the cell whose first corner is voxel `c`.
    fn vertex(&mut self, c: IVec3) -> u32 {
        let slot = cell(c);
        if self.cells[slot] != NONE {
            return self.cells[slot];
        }
        let mut sum = Vec3::ZERO;
        let mut n = 0;
        for (a, b) in CELL_EDGES {
            let (a, b) = (c + a, c + b);
            let (ia, ib) = (self.inside(a), self.inside(b));
            if ia == ib {
                continue;
            }
            let (from, to) = if ia { (a, b) } else { (b, a) };
            let t = crossing(self.pass.fill(self.block.at(from)), self.scale);
            sum += self.center(from) + (self.center(to) - self.center(from)) * t;
            n += 1;
        }
        let mut position = sum / n.max(1) as f32;
        let material = match self.pass {
            Pass::Solid => {
                let material = self.dominant(c);
                let first = self.center(c);
                position = deform(
                    position,
                    first,
                    self.scale,
                    material.look(),
                    self.origin + c,
                );
                material
            }
            Pass::Water => {
                position.y -= WATER_DROP;
                TerrainMaterial::Water
            }
        };
        let index = self.positions.len() as u32;
        self.positions.push(position);
        self.materials.push(material);
        self.normals.push(Vec3::ZERO);
        self.cells[slot] = index;
        index
    }

    /// The material of the cell's solid corners with the most occupancy
    /// in all (the first by index on a tie). Only materials the corners
    /// hold count: corners whose occupancy byte is 0 still hold a little
    /// ([`Voxel::fill`]), so a cell of only those is theirs, not Air's.
    fn dominant(&self, c: IVec3) -> TerrainMaterial {
        let mut total = [None::<u32>; 23];
        for i in 0..8 {
            let v = self.block.at(c + IVec3::new(i & 1, i >> 1 & 1, i >> 2));
            if v.is_solid() {
                *total[v.material as usize].get_or_insert(0) += u32::from(v.occupancy);
            }
        }
        let best = (0..23).rev().max_by_key(|&i| total[i]).unwrap_or(0);
        TerrainMaterial::ALL[best]
    }

    fn run(&mut self, diagonals: [[bool; 2]; 3]) {
        let own = PAD..PAD + CHUNK_VOXELS;
        for z in 1..SPAN - 1 {
            for y in 1..SPAN - 1 {
                for x in 1..SPAN - 1 {
                    let a = IVec3::new(x, y, z);
                    let mine = own.contains(&x) && own.contains(&y) && own.contains(&z);
                    for (axis, diagonal) in diagonals.into_iter().enumerate() {
                        self.edge(a, axis, diagonal, mine);
                    }
                }
            }
        }
    }

    /// The quad of the edge from voxel `a` along `axis`, if it crosses;
    /// its triangles are kept when `mine`, and only shade otherwise.
    fn edge(&mut self, a: IVec3, axis: usize, diagonal: [bool; 2], mine: bool) {
        let d = IVec3::AXES[axis];
        let b = a + d;
        let (ia, ib) = (self.inside(a), self.inside(b));
        if ia == ib {
            return;
        }
        let solid = self.block.at(if ia { a } else { b });
        let u = IVec3::AXES[(axis + 1) % 3];
        let w = IVec3::AXES[(axis + 2) % 3];
        if self.pass == Pass::Water
            && ![a - u - w, a - w, a, a - u]
                .iter()
                .all(|&c| self.wet[cell(c)])
        {
            return;
        }
        let material = match self.pass {
            Pass::Solid => solid.material,
            Pass::Water => TerrainMaterial::Water,
        };
        let q00 = self.vertex(a - u - w);
        let q10 = self.vertex(a - w);
        let q11 = self.vertex(a);
        let q01 = self.vertex(a - u);
        let mut tris = if diagonal[usize::from(!ia)] {
            [[q00, q10, q01], [q10, q11, q01]]
        } else {
            [[q00, q10, q11], [q00, q11, q01]]
        };
        // Counter-clockwise around +axis; turn it when the solid is at b.
        if !ia {
            for t in &mut tris {
                t.swap(1, 2);
            }
        }
        for t in tris {
            let [p0, p1, p2] = t.map(|i| self.positions[i as usize]);
            let face = (p1 - p0).cross(p2 - p0);
            for i in t {
                self.normals[i as usize] += face;
            }
            if mine {
                self.triangles.push((t, material));
            }
        }
    }

    /// Add the triangles to `surfaces`, one per material.
    fn split(&self, surfaces: &mut Vec<Surface>) {
        let positions = &self.positions;
        // Split by facing only where the material's textures differ by it,
        // so each draw covers as much as it can.
        let facing = |t: &[u32; 3], m: TerrainMaterial| {
            if !m.faces_differ() {
                return Facing::Side;
            }
            let p = t.map(|i| positions[i as usize]);
            Facing::of((p[1] - p[0]).cross(p[2] - p[0]).normalize_or(Vec3::Y))
        };
        let mut keys: Vec<(TerrainMaterial, Facing)> = self
            .triangles
            .iter()
            .map(|(t, m)| (*m, facing(t, *m)))
            .collect();
        keys.sort();
        keys.dedup();
        let mut remap = vec![NONE; self.positions.len()];
        for (material, side) in keys {
            let mut s = Surface {
                material,
                facing: side,
                ..Surface::default()
            };
            remap.fill(NONE);
            let flat = material.look().edges.flat();
            for (t, m) in &self.triangles {
                if *m != material || facing(t, *m) != side {
                    continue;
                }
                if flat {
                    let p = t.map(|i| self.positions[i as usize]);
                    let n = (p[1] - p[0]).cross(p[2] - p[0]).normalize_or(Vec3::Y);
                    for p in p {
                        s.indices.push(s.positions.len() as u32);
                        s.positions.push(p.to_array());
                        s.normals.push(n.to_array());
                    }
                    continue;
                }
                for &i in t {
                    let r = &mut remap[i as usize];
                    if *r == NONE {
                        *r = s.positions.len() as u32;
                        s.positions.push(self.positions[i as usize].to_array());
                        let n = self.normals[i as usize].normalize_or(Vec3::Y);
                        s.normals.push(n.to_array());
                    }
                    s.indices.push(*r);
                }
            }
            surfaces.push(s);
        }
    }
}
