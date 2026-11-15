//! Workspace's Terrain: its `SmoothGrid` voxels meshed by `rook_terrain`,
//! each chunk's surface split by texture layer and drawn with that
//! layer's textures, tint and blending ([`materials`]), water in its own
//! look, and Grass's blades near the camera ([`grass`]). Far terrain is
//! drawn coarser in rings around the camera, kept up as it moves
//! ([`rings`], [`background`]); edits re-mesh only what they reached
//! ([`meshing`]). The voxels also go to the renderer's light voxels, as the
//! chunks that changed ([`SceneCache::take_light_voxels`]). See
//! `docs/behavior/terrain/`.

mod background;
mod chunks;
mod grass;
mod looks;
mod materials;
mod meshing;
mod rings;

use glam::IVec3;
use rook_content::MaterialMap;
use rook_content::decode::mesh::{Mesh, TerrainCorner, TerrainMapping};
use rook_render::{Detile, TerrainColor, TerrainLook, TerrainVoxels};
use rook_terrain::grass::{GrassTops, add_grass_tops, blade_color};
use rook_terrain::grid::edit::Bounds;
use rook_terrain::lod::Rings;
use rook_terrain::{ChunkMesh, Coloring, Facing, Grid, Layer, Seam, TerrainMaterial};
use rook_types::MaterialColors;

use super::*;
use background::Background;
use grass::{GRASS_REACH, Grass};

/// One chunk's surface of one layer, as a mesh: the triangles wholly of
/// the layer, or its part of the seams.
#[derive(Debug, Clone, Copy)]
struct TerrainSurface {
    mesh: MeshHash,
    origin: Vec3,
    layer: Layer,
    seam: Option<Seam>,
}

/// A chunk drawn at some level: its surfaces and the scene entries drawing
/// them (none until the scene places them).
#[derive(Debug, Default)]
struct MeshedChunk {
    /// The sides closed against the next ring.
    seams: rook_terrain::Sides,
    surfaces: Vec<TerrainSurface>,
    entries: Vec<usize>,
}

/// A chunk of some level: the level and the chunk, in that level's chunks.
type LevelChunk = (u8, IVec3);

/// The order chunks are drawn and their Grass tops gathered in, so a
/// scene comes out the same however its chunks arrived.
fn chunk_order(&(level, c): &LevelChunk) -> (u8, i32, i32, i32) {
    (level, c.x, c.y, c.z)
}

/// The terrain's surfaces near and far, kept until its voxels change or
/// the camera moves to other rings, and the scene entries drawing them.
#[derive(Debug, Default)]
pub(super) struct TerrainCache {
    /// The Terrain whose voxels `levels` hold.
    meshed: Option<InstanceId>,
    /// Its voxels were written whole since they were meshed.
    whole: bool,
    /// The regions of them edited since they were meshed.
    edits: Vec<Bounds>,
    /// How many voxels of each material level 0 holds, kept up to date by
    /// each edit's chunks.
    counts: [u64; 23],
    /// The voxels at every level of detail (`rook_terrain::lod::pyramid`),
    /// shared with the background mesher while it works.
    levels: Arc<Vec<Grid>>,
    /// The materials the voxels hold, whose looks are read.
    materials: Vec<TerrainMaterial>,
    /// The lowest and highest level-0 chunk row holding voxels.
    rows: (i32, i32),
    /// Where the rings are for the chunks drawn, and where the camera has
    /// taken them since.
    shown: Option<Rings>,
    target: Option<Rings>,
    chunks: FxHashMap<LevelChunk, MeshedChunk>,
    /// The Grass tops of the chunks drawn at levels blades can reach
    /// (`rings::grows_grass`): blades stand on the ground that's drawn.
    tops: FxHashMap<LevelChunk, GrassTops>,
    /// The chunks in `tops` holding each tile's tops, in drawing order
    /// (`chunk_order`), so a tile's tops are gathered from its chunks
    /// alone.
    tile_chunks: FxHashMap<glam::IVec2, Vec<LevelChunk>>,
    /// Tiles whose tops went with a dropped chunk, to grow again.
    stale_tiles: FxHashSet<glam::IVec2>,
    /// Counts the meshes made for chunks and blades ([`TerrainCache::new_mesh`]).
    serial: u64,
    /// Counts the voxels' changes, so a background job begun on older
    /// voxels is thrown away.
    epoch: u64,
    looks: Looks,
    /// The Terrain instance, which waits for the textures.
    instance: Option<InstanceId>,
    grass: Grass,
    /// Meshes of chunks no longer drawn, for the renderer to free.
    dropped: Vec<MeshHash>,
    background: Background,
    /// What the renderer's light voxels haven't taken of the voxels.
    light: LightChange,
}

/// How the voxels changed since the renderer's light voxels took them.
#[derive(Debug, Default)]
enum LightChange {
    #[default]
    None,
    /// All of them may have.
    Whole,
    /// Those in these boxes.
    Edits(Vec<Bounds>),
}

impl LightChange {
    fn add(&mut self, edits: &[Bounds]) {
        match self {
            LightChange::Whole => {}
            LightChange::Edits(list) => list.extend_from_slice(edits),
            LightChange::None => *self = LightChange::Edits(edits.to_vec()),
        }
    }
}

#[derive(Debug)]
struct Looks {
    set: MaterialSet,
    layers: FxHashMap<Layer, (MaterialMaps, [f32; 3])>,
    water: [f32; 3],
    water_transparency: f32,
    water_reflectance: f32,
}

impl Default for Looks {
    fn default() -> Self {
        Looks {
            set: MaterialSet::Legacy,
            layers: FxHashMap::default(),
            water: [0.0; 3],
            water_transparency: 0.0,
            water_reflectance: 0.0,
        }
    }
}

impl TerrainCache {
    /// A key for a new mesh of `kind`, never one used before: a chunk or
    /// tile built again gets a new key, so the renderer can't place it on
    /// the old mesh just before freeing that one.
    fn new_mesh(&mut self, kind: &[u8]) -> MeshHash {
        self.serial += 1;
        let mut key = blake3::Hasher::new();
        key.update(kind);
        key.update(&self.serial.to_le_bytes());
        MeshHash(*key.finalize().as_bytes())
    }

    #[cfg(test)]
    fn surfaces(&self) -> impl Iterator<Item = &TerrainSurface> {
        self.chunks.values().flat_map(|c| &c.surfaces)
    }
}

impl SceneCache {
    pub fn terrain_meshes(&self) -> usize {
        self.terrain.chunks.values().map(|c| c.entries.len()).sum()
    }

    /// All of the terrain's voxels, for a renderer to take whole.
    pub(crate) fn light_voxels(&mut self) -> TerrainVoxels {
        self.terrain.light = LightChange::None;
        TerrainVoxels::Whole(self.terrain.levels.first().cloned().unwrap_or_default())
    }

    /// What changed of the terrain's voxels since the renderer took them:
    /// the chunks holding the edits (sharing their voxels), or all of them.
    pub(crate) fn take_light_voxels(&mut self) -> Option<TerrainVoxels> {
        let empty = Grid::new();
        let voxels = self.terrain.levels.first().unwrap_or(&empty);
        match std::mem::take(&mut self.terrain.light) {
            LightChange::None => None,
            LightChange::Whole => Some(TerrainVoxels::Whole(voxels.clone())),
            LightChange::Edits(bounds) => {
                let mut chunks = Grid::new();
                for b in &bounds {
                    chunks.copy_chunks(voxels, b.chunks(0));
                }
                Some(TerrainVoxels::Edits { chunks, bounds })
            }
        }
    }

    pub(super) fn is_terrain(&self, id: InstanceId) -> bool {
        self.terrain.instance == Some(id)
    }
}

fn workspace_terrain(dom: &Dom) -> Option<InstanceId> {
    dom.find_service("Workspace")
        .and_then(|ws| dom.find_first_child_of_class(ws, "Terrain"))
}

#[cfg(test)]
mod tests;
