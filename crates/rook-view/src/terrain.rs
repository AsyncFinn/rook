//! Workspace's Terrain: its `SmoothGrid` voxels meshed by `rook_terrain`,
//! each chunk's surface split by texture layer (a material and which way
//! it faces), and drawn with that material's top, side or bottom textures
//! from Studio's terrain material table (or the MaterialVariant
//! MaterialService puts in its place), tinted by `Terrain.MaterialColors`,
//! blending where layers meet (`rook_terrain::mesh::draws`). Water is drawn in
//! `WaterColor` at `WaterTransparency`, waving and rippled by the install's
//! water normal frames (the renderer's, `Installed`), and Grass grows
//! blades near the camera ([`grass`]) with `Decoration` on
//! (`docs/behavior/terrain.md`). When scripts edit the voxels, only the
//! chunks they changed, and their neighbours, are meshed again.

mod grass;
mod materials;
mod meshing;

use glam::IVec3;
use rook_content::MaterialMap;
use rook_content::decode::mesh::{Mesh, TerrainCorner, TerrainMapping};
use rook_render::{Detile, TERRAIN_LAYERS, TerrainLayer, TerrainLook};
use rook_terrain::grass::{GrassTops, add_grass_tops, blade_color};
use rook_terrain::{
    ChunkMesh, Facing, Grid, Layer, Seam, TerrainMaterial, chunks_to_mesh, mesh_chunk,
};

// The renderer's table holds every layer.
const _: () = assert!(Layer::COUNT <= TERRAIN_LAYERS);
use rook_types::MaterialColors;

use super::*;
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

/// A chunk's surfaces and its Grass tops.
#[derive(Debug, Default)]
struct MeshedChunk {
    surfaces: Vec<TerrainSurface>,
    tops: GrassTops,
}

/// The terrain's surfaces, kept until its `SmoothGrid` changes, and the
/// scene entries drawing them.
#[derive(Debug, Default)]
pub(super) struct TerrainCache {
    /// The hash of the `SmoothGrid` the meshes were built from.
    source: Option<blake3::Hash>,
    /// The voxels meshed, to find the chunks an edit changed.
    grid: Grid,
    chunks: HashMap<IVec3, MeshedChunk>,
    /// How many times the terrain was meshed: part of each mesh's key, so
    /// a chunk meshed again gets new keys.
    epoch: u64,
    /// Every chunk's surfaces, in chunk order.
    surfaces: Vec<TerrainSurface>,
    /// The Terrain instance, which waits for the textures.
    instance: Option<InstanceId>,
    /// Where the terrain's entries start in `scene.parts`, and how many.
    first: usize,
    count: usize,
    grass: Grass,
}

impl SceneCache {
    /// How many scene entries draw the terrain.
    pub fn terrain_meshes(&self) -> usize {
        self.terrain.count
    }

    /// Add the terrain's entries to the scene, meshing it first if its
    /// voxels aren't the ones meshed last.
    pub(super) fn add_terrain(&mut self, dom: &Dom) {
        profiling::scope!("SceneCache::add_terrain");
        let terrain = workspace_terrain(dom);
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
        self.terrain.instance = terrain;
        for mesh in self.terrain.grass.reset() {
            self.scene.meshes.remove(&mesh);
        }
        self.read_terrain_look(dom);
        let entries = self.terrain_entries(dom);
        self.terrain.first = self.scene.parts.len();
        self.terrain.count = entries.len();
        for entry in entries {
            self.scene.parts.push(entry);
            self.poses.push(Pose::default());
        }
    }

    /// Scripts changed the Terrain's properties or `Workspace.GlobalWind`:
    /// read the waves, wind, colours and grass again and redraw the
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

    /// The Terrain's waves and wind, and whether and in what colour Grass
    /// grows blades.
    fn read_terrain_look(&mut self, dom: &Dom) {
        let terrain = self.terrain.instance;
        self.scene.terrain = terrain_look(dom, terrain);
        let colors = terrain.and_then(|t| dom.material_colors_property(t, "MaterialColors"));
        let grass = &mut self.terrain.grass;
        grass.on = self.scene.terrain.grass_reach > 0.0;
        grass.color = TerrainMaterial::Grass
            .color(colors.as_ref(), MaterialSet::Materials2022)
            .map(blade_color)
            .unwrap_or_default();
    }

    /// Redraw the terrain (its textures arrived): the entries that changed.
    pub(super) fn refresh_terrain(&mut self, dom: &Dom) -> Vec<usize> {
        let entries = self.terrain_entries(dom);
        if entries.len() != self.terrain.count {
            return Vec::new();
        }
        let first = self.terrain.first;
        self.scene.parts[first..first + entries.len()].copy_from_slice(&entries);
        (first..first + entries.len()).collect()
    }

    /// Whether `id` is the Terrain whose entries the scene draws.
    pub(super) fn is_terrain(&self, id: InstanceId) -> bool {
        self.terrain.instance == Some(id)
    }

    /// The entries drawing the terrain's surfaces, with what has loaded of
    /// their textures; the Terrain waits for the rest.
    fn terrain_entries(&mut self, dom: &Dom) -> Vec<PartInstance> {
        let Some(terrain) = self.terrain.instance else {
            return Vec::new();
        };
        // Terrain draws the place's set of textures and default colours
        // (`TerrainMaterial::textures`), as parts do.
        let set = look::materials::material_set(dom);
        let colors = dom.material_colors_property(terrain, "MaterialColors");
        let water = dom
            .color3_property(terrain, "WaterColor")
            .map(Color3::to_array)
            .unwrap_or([12.0 / 255.0, 84.0 / 255.0, 92.0 / 255.0]);
        let water_transparency = dom
            .float_property(terrain, "WaterTransparency")
            .unwrap_or(0.3);
        // The shader mixes towards the skybox by `WaterReflectance`.
        let water_reflectance = dom
            .float_property(terrain, "WaterReflectance")
            .unwrap_or(1.0);
        let surfaces = self.terrain.surfaces.clone();
        let mut looks: HashMap<Layer, (MaterialMaps, [f32; 3])> = HashMap::new();
        let mut entries = Vec::with_capacity(surfaces.len());
        let mut layers = [TerrainLayer::default(); TERRAIN_LAYERS];
        for s in surfaces {
            let material = s.layer.material;
            let mut part =
                PartInstance::plain(DrawShape::File(s.mesh), s.origin, Mat3::IDENTITY, Vec3::ONE);
            part.material = material.material();
            part.material_set = set;
            if material == TerrainMaterial::Water {
                part.color = water;
                // Studio still draws water at WaterTransparency 1, letting
                // through only a fifth of what's behind it (the shader's
                // `WATER_PASSES`), where a part would be gone.
                part.transparency = water_transparency.clamp(0.0, 0.999);
                part.reflectance = water_reflectance.clamp(0.0, 1.0);
                part.cast_shadow = false;
            } else {
                let (maps, color) = *looks.entry(s.layer).or_insert_with(|| {
                    let (maps, color, blend) =
                        self.terrain_look(dom, terrain, s.layer, set, colors.as_ref());
                    layers[s.layer.index()] = blend;
                    (maps, color)
                });
                part.maps = maps;
                part.color = color;
                part.over = s.seam == Some(Seam::Over);
                part.cast_shadow = !part.over;
            }
            entries.push(part);
        }
        self.scene.terrain.layers = layers;
        entries
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
