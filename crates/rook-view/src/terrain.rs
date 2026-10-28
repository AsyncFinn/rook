//! Workspace's Terrain: its `SmoothGrid` voxels meshed by `rook_terrain`,
//! each chunk's surface split by texture layer and drawn with that
//! layer's textures, tint and blending ([`materials`]), water in its own
//! look, and Grass's blades near the camera ([`grass`]). Far terrain is
//! drawn coarser in rings around the camera, kept up as it moves
//! ([`rings`], [`background`]); edits re-mesh only what they reached
//! ([`meshing`]). See `docs/behavior/terrain/`.

mod background;
mod grass;
mod materials;
mod meshing;
mod rings;

use glam::IVec3;
use rook_content::MaterialMap;
use rook_content::decode::mesh::{Mesh, TerrainCorner, TerrainMapping};
use rook_render::{Detile, TERRAIN_LAYERS, TerrainLayer, TerrainLook};
use rook_terrain::grass::{GrassTops, add_grass_tops, blade_color};
use rook_terrain::lod::Rings;
use rook_terrain::{ChunkMesh, Facing, Grid, Layer, Seam, TerrainMaterial};

// The renderer's table holds every layer.
const _: () = assert!(Layer::COUNT <= TERRAIN_LAYERS);
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

/// The terrain's surfaces near and far, kept until its `SmoothGrid`
/// changes or the camera moves to other rings, and the scene entries
/// drawing them.
#[derive(Debug, Default)]
pub(super) struct TerrainCache {
    /// The hash of the `SmoothGrid` the meshes were built from.
    source: Option<blake3::Hash>,
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
    /// The camera's level-0 column the Grass tops kept are around.
    near: Option<glam::IVec2>,
    /// The chunks drawn, by level.
    chunks: HashMap<LevelChunk, MeshedChunk>,
    /// The Grass tops of the level-0 chunks meshed (drawn there, or near
    /// enough to the camera for blades).
    tops: HashMap<IVec3, GrassTops>,
    /// Counts the chunk meshes made: part of each mesh's key, so a chunk
    /// meshed again gets new keys.
    serial: u64,
    /// Counts the voxels' changes, so a background job begun on older
    /// voxels is thrown away.
    epoch: u64,
    /// What each layer is drawn with.
    looks: Looks,
    /// The Terrain instance, which waits for the textures.
    instance: Option<InstanceId>,
    grass: Grass,
    /// Meshes of chunks no longer drawn, for the renderer to free.
    dropped: Vec<MeshHash>,
    background: Background,
}

/// What the terrain's layers are drawn with, read from the Terrain.
#[derive(Debug)]
struct Looks {
    set: MaterialSet,
    layers: HashMap<Layer, (MaterialMaps, [f32; 3])>,
    water: [f32; 3],
    water_transparency: f32,
    water_reflectance: f32,
}

impl Default for Looks {
    fn default() -> Self {
        Looks {
            set: MaterialSet::Legacy,
            layers: HashMap::new(),
            water: [0.0; 3],
            water_transparency: 0.0,
            water_reflectance: 0.0,
        }
    }
}

impl TerrainCache {
    #[cfg(test)]
    fn surfaces(&self) -> impl Iterator<Item = &TerrainSurface> {
        self.chunks.values().flat_map(|c| &c.surfaces)
    }
}

impl SceneCache {
    /// How many scene entries draw the terrain.
    pub fn terrain_meshes(&self) -> usize {
        self.terrain.chunks.values().map(|c| c.entries.len()).sum()
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
        let grid = terrain.and_then(|t| smooth_grid(dom, t));
        let source = grid.map(blake3::hash);
        if source != self.terrain.source {
            self.terrain.source = source;
            let grid = match grid.map(Grid::decode) {
                Some(Ok(grid)) => grid,
                Some(Err(e)) => {
                    eprintln!("warning: Terrain.SmoothGrid doesn't decode: {e}");
                    Grid::new()
                }
                None => Grid::new(),
            };
            self.mesh_terrain(grid);
        }
        self.read_looks(dom);
        let mut keys: Vec<LevelChunk> = self.terrain.chunks.keys().copied().collect();
        keys.sort_unstable_by_key(|(l, c)| (*l, c.x, c.y, c.z));
        self.place_chunks(&keys);
    }

    /// Scripts changed the Terrain's properties or `Workspace.GlobalWind`:
    /// read the waves, wind, colors and grass again and redraw the
    /// terrain's entries and blades. Returns the entries that changed, or
    /// `None` when the voxels themselves changed and the scene must be
    /// rebuilt.
    pub(super) fn terrain_changed(&mut self, dom: &Dom) -> Option<Vec<usize>> {
        let terrain = workspace_terrain(dom);
        let source = terrain.and_then(|t| smooth_grid(dom, t)).map(blake3::hash);
        if terrain != self.terrain.instance || source != self.terrain.source {
            return None;
        }
        self.read_terrain_look(dom);
        let mut changed = self.refresh_terrain(dom);
        let color = self.terrain.grass.color;
        for i in self.terrain.grass.entries() {
            self.scene.parts[i].color = color;
            changed.push(i);
        }
        Some(changed)
    }

    /// The Terrain's waves and wind, and whether and in what color Grass
    /// grows blades.
    fn read_terrain_look(&mut self, dom: &Dom) {
        let terrain = self.terrain.instance;
        self.scene.terrain = terrain_look(dom, terrain);
        let colors = terrain.and_then(|t| dom.material_colors_property(t, "MaterialColors"));
        let grass = &mut self.terrain.grass;
        grass.on = self.scene.terrain.grass_reach > 0.0;
        grass.color = TerrainMaterial::Grass
            .color(colors.as_ref(), look::materials::material_set(dom))
            .map(blade_color)
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

    /// Whether `id` is the Terrain whose entries the scene draws.
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
            layers: HashMap::new(),
        };
        let mut blends = [TerrainLayer::default(); TERRAIN_LAYERS];
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
                let (maps, color, blend) =
                    self.terrain_look(dom, terrain, layer, set, colors.as_ref());
                blends[layer.index()] = blend;
                looks.layers.insert(layer, (maps, color));
            }
        }
        self.scene.terrain.layers = blends;
        self.terrain.looks = looks;
    }
}

impl Looks {
    /// The entry drawing surface `s`.
    fn entry(&self, s: &TerrainSurface) -> PartInstance {
        let material = s.layer.material;
        let mut part =
            PartInstance::plain(DrawShape::File(s.mesh), s.origin, Mat3::IDENTITY, Vec3::ONE);
        part.material = material.material();
        part.material_set = self.set;
        if material == TerrainMaterial::Water {
            part.color = self.water;
            // Studio still draws water at WaterTransparency 1, letting
            // through only a fifth of what's behind it (the shader's
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

/// Workspace's Terrain.
fn workspace_terrain(dom: &Dom) -> Option<InstanceId> {
    dom.find_service("Workspace")
        .and_then(|ws| dom.find_first_child_of_class(ws, "Terrain"))
}

/// A Terrain's `SmoothGrid`, borrowed, not cloned: a large map's grid is
/// megabytes.
fn smooth_grid(dom: &Dom, terrain: InstanceId) -> Option<&[u8]> {
    let db = rook_reflection::database();
    dom.get(terrain)?
        .properties()
        .iter()
        .find_map(|(id, v)| match v {
            Value::BinaryString(b) if db.property(*id).name == "SmoothGrid" => {
                Some(AsRef::<[u8]>::as_ref(b))
            }
            _ => None,
        })
}

#[cfg(test)]
mod tests;
