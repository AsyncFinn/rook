//! The surface Roblox builds from the voxels, measured in Studio with
//! `Workspace:Raycast` (`docs/behavior/terrain.md`): surface nets.
//!
//! Corners of the dual grid are voxel centres. Each dual cell (the cube
//! between eight voxel centres) where solid meets air holds one vertex: the
//! mean of the points where its edges cross the surface. An edge from a
//! solid voxel to an air one crosses `4 × occupancy` studs from the solid
//! voxel's centre, so a full voxel reaches the next voxel's centre. Each
//! voxel edge that crosses gets a quad joining the vertices of the four
//! cells around it, made of the solid voxel's material and split into two
//! triangles along a fixed diagonal ([`DIAGONALS`]).
//!
//! Water is meshed the same way in a second pass, where solid voxels count
//! as inside too and only the faces of water voxels are kept, then moved
//! down half a voxel ([`WATER_DROP`]).
//!
//! [`ChunkMesh::surfaces`] split the triangles by their voxel's material,
//! for collisions; [`ChunkMesh::draws`] by the layers drawn, blending
//! where they meet ([`draws`]).

use glam::{IVec3, Vec3};

use crate::TerrainMaterial;
use crate::grid::{CHUNK_VOXELS, Grid, VOXEL_STUDS, Voxel};

mod deform;
pub mod draws;

use deform::deform;
pub use draws::{Corner, Draw, Layer, Seam};

/// One material's part of a chunk's surface, relative to the chunk's
/// corner ([`ChunkMesh::origin`]).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Surface {
    pub material: TerrainMaterial,
    /// Which of the material's textures its triangles take.
    pub facing: Facing,
    pub positions: Vec<[f32; 3]>,
    /// Smooth: the mean of the faces around each vertex, across chunks
    /// and materials; flat for the materials with hard edges.
    pub normals: Vec<[f32; 3]>,
    /// Three per triangle, counter-clockwise seen from outside.
    pub indices: Vec<u32>,
}

/// Which way a triangle faces, for its material's top, side and bottom
/// textures: up when its normal is closer to one of the five upward planes
/// Studio projects textures on (straight up and the four at 45 degrees)
/// than to a vertical one, that is more than 22.5 degrees up (observed: a
/// Grass ball shows grass down to about that far above its middle).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Facing {
    Top,
    #[default]
    Side,
    Bottom,
}

impl Facing {
    /// sin(22.5 degrees).
    const LIMIT: f32 = 0.382_683_43;

    pub fn of(normal: Vec3) -> Self {
        if normal.y > Self::LIMIT {
            Facing::Top
        } else if normal.y < -Self::LIMIT {
            Facing::Bottom
        } else {
            Facing::Side
        }
    }
}

/// The surface of one chunk: the quads of the voxel edges starting in it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ChunkMesh {
    pub chunk: IVec3,
    /// By material, solids first and water last.
    pub surfaces: Vec<Surface>,
    /// What's drawn, by layer, solids first and water last.
    pub draws: Vec<Draw>,
}

impl ChunkMesh {
    /// Where the chunk's corner is in the world.
    pub fn origin(&self) -> Vec3 {
        chunk_origin(self.chunk)
    }

    pub fn triangles(&self) -> usize {
        self.surfaces.iter().map(|s| s.indices.len() / 3).sum()
    }
}

pub fn chunk_origin(chunk: IVec3) -> Vec3 {
    (chunk * CHUNK_VOXELS).as_vec3() * VOXEL_STUDS
}

/// Which diagonal splits the quad of an edge along each axis (X, Y, Z):
/// true joins the cell before it on the first other axis and after it on
/// the second to the opposite one (the other axes in X, Y, Z order turned
/// to start after the edge's: Y and Z for X, Z and X for Y, X and Y for Z).
/// Measured: see `tests::diagonals`.
pub const DIAGONALS: [[bool; 2]; 3] = [[true; 2]; 3];

/// The chunks that can have a surface: every stored chunk, and the empty
/// ones just below stored ones on each axis (they own the edges into the
/// stored chunk). Chunks whose whole neighbourhood is one uniform voxel
/// have none and are left out.
pub fn chunks_to_mesh(grid: &Grid) -> Vec<IVec3> {
    let mut out: Vec<IVec3> = grid
        .chunks()
        .flat_map(|c| [c, c - IVec3::X, c - IVec3::Y, c - IVec3::Z])
        .collect();
    out.sort_by_key(|c| (c.x, c.y, c.z));
    out.dedup();
    out.retain(|c| !all_uniform(grid, *c));
    out
}

/// Whether the 27 chunks around `chunk` are all the same uniform voxel.
fn all_uniform(grid: &Grid, chunk: IVec3) -> bool {
    let Some(first) = grid.uniform(chunk) else {
        return false;
    };
    (-1..=1).all(|z| {
        (-1..=1).all(|y| (-1..=1).all(|x| grid.uniform(chunk + IVec3::new(x, y, z)) == Some(first)))
    })
}

/// How far water's surface sits below where solid terrain's would: half a
/// voxel (Studio's terrain material table gives Water `"water": 0.5`;
/// measured: a lone full water voxel spans 3.333 to 0.667 studs below its
/// centre, a full one-voxel slab 6 below to 2 above, its edge sloping as a
/// solid slab's does, 2 studs lower).
pub const WATER_DROP: f32 = VOXEL_STUDS / 2.0;

/// Voxels read before a chunk's first one: edges start one voxel before
/// it (for the normals of the faces just outside), and their cells one
/// more.
const PAD: i32 = 2;
/// How far outside a chunk, in voxels, its surface reads: changing a
/// voxel changes the surfaces of the chunks within this many voxels of it.
pub const MESH_REACH: i32 = PAD;
/// Voxels read along each axis.
const SPAN: i32 = CHUNK_VOXELS + 2 * PAD;
/// Dual cells along each axis.
const CELLS: i32 = SPAN - 1;
const NONE: u32 = u32::MAX;

/// Mesh one chunk.
pub fn mesh_chunk(grid: &Grid, chunk: IVec3) -> ChunkMesh {
    mesh_chunk_with(grid, chunk, DIAGONALS)
}

pub(crate) fn mesh_chunk_with(grid: &Grid, chunk: IVec3, diagonals: [[bool; 2]; 3]) -> ChunkMesh {
    let first = chunk * CHUNK_VOXELS - IVec3::splat(PAD);
    let mut voxels = Vec::new();
    grid.read_block(first, IVec3::splat(SPAN), &mut voxels);
    let block = Block { voxels };
    let mut surfaces = Vec::new();
    let mut draws = Vec::new();
    for pass in [Pass::Solid, Pass::Water] {
        let mut mesher = Mesher {
            block: &block,
            pass,
            origin: chunk * CHUNK_VOXELS - IVec3::splat(PAD),
            cells: vec![NONE; (CELLS * CELLS * CELLS) as usize],
            positions: Vec::new(),
            normals: Vec::new(),
            materials: Vec::new(),
            triangles: Vec::new(),
        };
        mesher.run(diagonals);
        mesher.draws(&mut draws);
        mesher.split(&mut surfaces);
    }
    ChunkMesh {
        chunk,
        surfaces,
        draws,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Pass {
    /// Solid voxels against air and water.
    Solid,
    /// Water voxels against air (solids count as inside).
    Water,
}

impl Pass {
    fn inside(self, v: Voxel) -> bool {
        match self {
            Pass::Solid => v.is_solid(),
            Pass::Water => v.material != TerrainMaterial::Air,
        }
    }

    /// Whether faces of this inside voxel are kept.
    fn keeps(self, v: Voxel) -> bool {
        match self {
            Pass::Solid => true,
            Pass::Water => v.material == TerrainMaterial::Water,
        }
    }
}

struct Block {
    voxels: Vec<Voxel>,
}

impl Block {
    /// Voxel `p` of the block (0 to `SPAN - 1` on each axis).
    fn at(&self, p: IVec3) -> Voxel {
        self.voxels[(p.x + SPAN * (p.y + SPAN * p.z)) as usize]
    }
}

/// Where voxel `p` of the block has its centre, from the chunk's corner.
fn centre(p: IVec3) -> Vec3 {
    ((p - IVec3::splat(PAD)).as_vec3() + 0.5) * VOXEL_STUDS
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

struct Mesher<'a> {
    block: &'a Block,
    pass: Pass,
    /// The block's first voxel in the grid (seeds the shift).
    origin: IVec3,
    /// Each cell's vertex, by cell, once made.
    cells: Vec<u32>,
    positions: Vec<Vec3>,
    normals: Vec<Vec3>,
    /// Each vertex's material: its cell's ([`Mesher::dominant`]), or Water.
    materials: Vec<TerrainMaterial>,
    /// The chunk's own triangles and their material.
    triangles: Vec<([u32; 3], TerrainMaterial)>,
}

impl Mesher<'_> {
    fn inside(&self, p: IVec3) -> bool {
        self.pass.inside(self.block.at(p))
    }

    /// The vertex of the cell whose first corner is voxel `c`.
    fn vertex(&mut self, c: IVec3) -> u32 {
        let slot = (c.x + CELLS * (c.y + CELLS * c.z)) as usize;
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
            let fill = self.block.at(from).fill();
            sum += centre(from) + (centre(to) - centre(from)) * fill;
            n += 1;
        }
        let mut position = sum / n.max(1) as f32;
        let material = match self.pass {
            Pass::Solid => {
                let material = self.dominant(c);
                position = deform(position, centre(c), material.shape(), self.origin + c);
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
        if !self.pass.keeps(solid) {
            return;
        }
        let u = IVec3::AXES[(axis + 1) % 3];
        let w = IVec3::AXES[(axis + 2) % 3];
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
                self.triangles.push((t, solid.material));
            }
        }
    }

    /// Add the triangles to `surfaces`, one per material.
    fn split(self, surfaces: &mut Vec<Surface>) {
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
            let flat = material.shape().edges.flat();
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
