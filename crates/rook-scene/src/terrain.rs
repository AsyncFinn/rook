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
mod grass;
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

    /// Add the terrain's entries to the (rebuilt) scene, meshing it first
    /// if its voxels aren't the ones meshed last.
    pub(super) fn add_terrain(&mut self, dom: &Dom) {
        profiling::scope!("SceneCache::add_terrain");
        let terrain = workspace_terrain(dom);
        self.terrain.instance = terrain;
        for mesh in self.terrain.grass.reset() {
            self.scene.meshes.remove(&mesh);
        }
        // The scene's entries are gone; the meshes stay.
        for chunk in self.terrain.chunks.values_mut() {
            chunk.entries.clear();
        }
        self.read_terrain_look(dom);
        // The rebuilt scene places every chunk.
        self.take_voxels(dom, false);
        self.read_looks(dom);
        let mut keys: Vec<LevelChunk> = self.terrain.chunks.keys().copied().collect();
        keys.sort_unstable_by_key(chunk_order);
        self.place_chunks(&keys);
    }

    /// Note the frame's changes to the Terrain's voxels, which the next
    /// [`SceneCache::take_voxels`] meshes (the scene may be rebuilt first).
    pub(super) fn note_voxels(&mut self, changes: &RenderChanges) {
        let t = &mut self.terrain;
        t.whole |= changes.terrain_voxels;
        let meshed = t.meshed;
        t.edits.extend(
            changes
                .terrain_edits
                .iter()
                .filter(|(id, _)| Some(*id) == meshed)
                .map(|(_, bounds)| *bounds),
        );
    }

    /// Mesh what changed of the Terrain's voxels since they were meshed:
    /// only the chunks the edits reached, or what differs from a whole
    /// new grid. With `place`, those chunks are swapped in the scene and
    /// the entries that changed returned.
    fn take_voxels(&mut self, dom: &Dom, place: bool) -> Vec<usize> {
        let t = &mut self.terrain;
        let whole = std::mem::take(&mut t.whole) || t.meshed != t.instance;
        let edits = std::mem::take(&mut t.edits);
        if !whole && edits.is_empty() {
            return Vec::new();
        }
        t.meshed = t.instance;
        let empty = Grid::new();
        let voxels = t.instance.and_then(|i| dom.voxels(i)).unwrap_or(&empty);
        let materials = t.materials.clone();
        let edits = (!whole).then_some(&edits[..]);
        let mut changed = self.mesh_terrain(voxels, edits, place);
        // A material new to the terrain needs its look read.
        if place && self.terrain.materials != materials {
            changed.extend(self.refresh_terrain(dom));
        }
        changed
    }

    /// Workspace's Terrain is another instance: stop drawing the old one's
    /// chunks and blades and mesh the new one's voxels, leaving the rest
    /// of the scene as it is. Returns the entries that changed.
    fn swap_terrain(&mut self, dom: &Dom) -> Vec<usize> {
        self.terrain.background.wait();
        let mut changed = Vec::new();
        let chunks: Vec<LevelChunk> = self.terrain.chunks.keys().copied().collect();
        for key in chunks {
            changed.extend(self.drop_chunk(key));
        }
        for i in self.terrain.grass.entries() {
            self.release(i);
            changed.push(i);
        }
        for mesh in self.terrain.grass.reset() {
            self.scene.meshes.remove(&mesh);
        }
        let t = &mut self.terrain;
        t.instance = workspace_terrain(dom);
        // The new one's voxels are taken whole.
        t.edits.clear();
        t.levels = Arc::default();
        t.shown = None;
        t.epoch += 1;
        changed.extend(self.take_voxels(dom, true));
        changed
    }

    /// Scripts changed the Terrain (its voxels or other properties),
    /// `Workspace.GlobalWind`, or which Terrain Workspace holds: remesh
    /// what the voxel edits reached and, with `look` or for another
    /// Terrain, read the waves, wind, colors and grass again and redraw
    /// the terrain's entries and blades. Returns the entries that changed.
    pub(super) fn terrain_changed(&mut self, dom: &Dom, look: bool) -> Vec<usize> {
        let same = workspace_terrain(dom) == self.terrain.instance;
        let mut changed = if same {
            self.take_voxels(dom, true)
        } else {
            self.swap_terrain(dom)
        };
        // An edit alone changes no look.
        if same && !look {
            return changed;
        }
        self.read_terrain_look(dom);
        changed.extend(self.refresh_terrain(dom));
        let color = self.terrain.grass.color;
        for i in self.terrain.grass.entries() {
            self.scene.parts[i].color = color;
            changed.push(i);
        }
        changed
    }

    /// The Terrain's waves and wind, and whether and in what color Grass
    /// grows blades.
    fn read_terrain_look(&mut self, dom: &Dom) {
        let terrain = self.terrain.instance;
        self.scene.terrain = terrain_look(dom, terrain);
        let colors = terrain.and_then(|t| dom.material_colors_property(t, "MaterialColors"));
        let grass = &mut self.terrain.grass;
        grass.on = self.scene.terrain.grass_reach > 0.0;
        let set = look::materials::material_set(dom);
        grass.color = TerrainMaterial::Grass
            .color(colors.as_ref(), set)
            .map(|c| blade_color(c, set))
            .unwrap_or_default();
    }

    /// Redraw the terrain (its textures arrived, or its colors changed):
    /// the entries that changed.
    pub(super) fn refresh_terrain(&mut self, dom: &Dom) -> Vec<usize> {
        self.read_looks(dom);
        let mut changed = Vec::new();
        for chunk in self.terrain.chunks.values() {
            for (s, &i) in chunk.surfaces.iter().zip(&chunk.entries) {
                self.scene.parts[i] = self.terrain.looks.entry(s);
                changed.push(i);
            }
        }
        changed
    }

    pub(super) fn is_terrain(&self, id: InstanceId) -> bool {
        self.terrain.instance == Some(id)
    }

    /// Give the chunks at `keys` entries in the scene. Returns them.
    fn place_chunks(&mut self, keys: &[LevelChunk]) -> Vec<usize> {
        let mut placed = Vec::new();
        for key in keys {
            let Some(chunk) = self.terrain.chunks.get(key) else {
                continue;
            };
            let entries: Vec<PartInstance> = chunk
                .surfaces
                .iter()
                .map(|s| self.terrain.looks.entry(s))
                .collect();
            let indices: Vec<usize> = entries
                .into_iter()
                .map(|e| self.take_index(e, Pose::default()))
                .collect();
            placed.extend(&indices);
            self.terrain.chunks.get_mut(key).expect("placed").entries = indices;
        }
        placed
    }

    /// Stop drawing the chunk at `key`, freeing its meshes. Returns its
    /// entries.
    fn drop_chunk(&mut self, key: LevelChunk) -> Vec<usize> {
        let Some(chunk) = self.terrain.chunks.remove(&key) else {
            return Vec::new();
        };
        for s in &chunk.surfaces {
            self.scene.meshes.remove(&s.mesh);
            self.terrain.dropped.push(s.mesh);
        }
        for &i in &chunk.entries {
            self.release(i);
        }
        if let Some(tops) = self.terrain.tops.remove(&key) {
            for tile in tops.keys() {
                if let Some(chunks) = self.terrain.tile_chunks.get_mut(tile) {
                    chunks.retain(|c| *c != key);
                    if chunks.is_empty() {
                        self.terrain.tile_chunks.remove(tile);
                    }
                }
            }
            self.terrain.stale_tiles.extend(tops.keys());
        }
        chunk.entries
    }

    /// Read what each layer the voxels hold is drawn with: its textures as
    /// far as they've loaded (the Terrain waits for the rest), color and
    /// blending, and the water's look. Terrain draws the legacy set's
    /// textures in every place (`TerrainMaterial::look`), with the place's
    /// set's default colors where it saved none.
    fn read_looks(&mut self, dom: &Dom) {
        let Some(terrain) = self.terrain.instance else {
            return;
        };
        let set = look::materials::material_set(dom);
        let colors = dom.material_colors_property(terrain, "MaterialColors");
        let mut looks = Looks {
            set,
            water: dom
                .color3_property(terrain, "WaterColor")
                .map(Color3::to_array)
                .unwrap_or([12.0 / 255.0, 84.0 / 255.0, 92.0 / 255.0]),
            water_transparency: dom
                .float_property(terrain, "WaterTransparency")
                .unwrap_or(0.3),
            // The shader mixes towards the skybox by `WaterReflectance`.
            water_reflectance: dom
                .float_property(terrain, "WaterReflectance")
                .unwrap_or(1.0),
            layers: FxHashMap::default(),
        };
        for material in self.terrain.materials.clone() {
            if material == TerrainMaterial::Water {
                continue;
            }
            let facings: &[Facing] = if material.faces_differ() {
                &[Facing::Top, Facing::Side, Facing::Bottom]
            } else {
                &[Facing::Side]
            };
            for &facing in facings {
                let layer = Layer { material, facing };
                let (maps, color) = self.terrain_look(dom, terrain, layer, set, colors.as_ref());
                looks.layers.insert(layer, (maps, color));
            }
        }
        self.terrain.looks = looks;
    }
}

impl Looks {
    fn entry(&self, s: &TerrainSurface) -> PartInstance {
        let material = s.layer.material;
        let mut part =
            PartInstance::plain(DrawShape::File(s.mesh), s.origin, Mat3::IDENTITY, Vec3::ONE);
        part.material = material.material();
        part.material_set = self.set;
        if material == TerrainMaterial::Water {
            part.color = self.water;
            // Studio still draws water at WaterTransparency 1, letting
            // through about half of what's behind it (the shader's
            // `WATER_PASSES`), where a part would be gone.
            part.transparency = self.water_transparency.clamp(0.0, 0.999);
            part.reflectance = self.water_reflectance.clamp(0.0, 1.0);
            part.cast_shadow = false;
            return part;
        }
        if let Some((maps, color)) = self.layers.get(&s.layer) {
            part.maps = *maps;
            part.color = *color;
        }
        part.over = s.seam == Some(Seam::Over);
        part.cast_shadow = !part.over;
        part
    }
}

/// How the terrain's water waves and its grass moves: `WaterWaveSize`,
/// `WaterWaveSpeed` (Studio's defaults 0.15 and 10), `Workspace.GlobalWind`,
/// and how far grass is drawn with `Decoration` on.
fn terrain_look(dom: &Dom, terrain: Option<InstanceId>) -> TerrainLook {
    let mut look = TerrainLook::default();
    if let Some(t) = terrain {
        look.wave_size = dom
            .float_property(t, "WaterWaveSize")
            .unwrap_or(look.wave_size);
        look.wave_speed = dom
            .float_property(t, "WaterWaveSpeed")
            .unwrap_or(look.wave_speed);
        if dom.bool_property(t, "Decoration").unwrap_or(false) {
            look.grass_reach = GRASS_REACH;
        }
    }
    if let Some(ws) = dom.find_service("Workspace") {
        look.wind = dom.vector3_property(ws, "GlobalWind").unwrap_or(Vec3::ZERO);
    }
    look
}

fn workspace_terrain(dom: &Dom) -> Option<InstanceId> {
    dom.find_service("Workspace")
        .and_then(|ws| dom.find_first_child_of_class(ws, "Terrain"))
}

#[cfg(test)]
mod tests;
