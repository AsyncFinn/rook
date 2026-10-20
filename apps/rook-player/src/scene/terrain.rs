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

use glam::IVec3;
use rayon::prelude::*;
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
        let colors = terrain.and_then(|t| match dom.property_by_name(t, "MaterialColors") {
            Some(Value::MaterialColors(c)) => Some(c),
            _ => None,
        });
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

    /// Mesh the chunks of `grid` that differ from the voxels meshed last
    /// (all of them the first time), and the neighbours whose surfaces
    /// reach the voxels that changed.
    fn mesh_terrain(&mut self, grid: Grid) {
        profiling::scope!("mesh_terrain");
        let wanted: HashSet<IVec3> = chunks_to_mesh(&grid).into_iter().collect();
        let mut remesh: HashSet<IVec3> = HashSet::new();
        if self.terrain.chunks.is_empty() {
            remesh.extend(&wanted);
        } else {
            for bounds in self.terrain.grid.changes(&grid) {
                remesh.extend(bounds.chunks(rook_terrain::MESH_REACH));
            }
        }
        let terrain = &mut self.terrain;
        let gone: Vec<IVec3> = terrain
            .chunks
            .keys()
            .filter(|c| remesh.contains(c) || !wanted.contains(c))
            .copied()
            .collect();
        for c in gone {
            for s in terrain.chunks.remove(&c).expect("meshed").surfaces {
                self.scene.meshes.remove(&s.mesh);
            }
        }
        terrain.epoch += 1;
        let todo: Vec<IVec3> = remesh.into_iter().filter(|c| wanted.contains(c)).collect();
        let meshes: Vec<ChunkMesh> = todo.par_iter().map(|c| mesh_chunk(&grid, *c)).collect();
        for chunk in meshes {
            let meshed = self.add_chunk_meshes(chunk);
            self.terrain.chunks.insert(meshed.0, meshed.1);
        }
        let terrain = &mut self.terrain;
        terrain.grid = grid;
        let mut order: Vec<&IVec3> = terrain.chunks.keys().collect();
        order.sort_unstable_by_key(|c| (c.x, c.y, c.z));
        terrain.surfaces = order
            .iter()
            .flat_map(|c| terrain.chunks[*c].surfaces.iter().copied())
            .collect();
        let mut tops = GrassTops::default();
        for c in order {
            for (tile, tris) in &terrain.chunks[c].tops {
                tops.entry(*tile).or_default().extend(tris);
            }
        }
        terrain.grass.tops = tops;
    }

    /// Put a chunk's surfaces in the scene's meshes.
    fn add_chunk_meshes(&mut self, chunk: ChunkMesh) -> (IVec3, MeshedChunk) {
        let mut tops = GrassTops::default();
        add_grass_tops(&chunk, &mut tops);
        let origin = chunk.origin();
        let mut surfaces = Vec::new();
        for draw in chunk.draws {
            let mut key = blake3::Hasher::new();
            key.update(&self.terrain.epoch.to_le_bytes());
            let seam = match draw.seam {
                None => 0,
                Some(Seam::First) => 1,
                Some(Seam::Over) => 2,
            };
            key.update(&[draw.layer.index() as u8, seam]);
            for v in chunk.chunk.to_array() {
                key.update(&v.to_le_bytes());
            }
            let mesh = MeshHash(*key.finalize().as_bytes());
            let count = draw.positions.len();
            let material = draw.layer.material;
            let terrain_corners = draw
                .corners
                .iter()
                .map(|c| TerrainCorner {
                    layers: c.layers.map(|l| l.index() as u8),
                    corner: c.corner,
                    drawn: (0..3)
                        .filter(|&k| c.layers[k] == draw.layer)
                        .fold(0, |bits, k| bits | 1 << k),
                })
                .collect();
            self.scene.meshes.insert(
                mesh,
                Arc::new(Mesh {
                    positions: draw.positions,
                    normals: draw.normals,
                    uvs: vec![[0.0; 2]; count],
                    indices: draw.indices,
                    terrain: Some(if material == TerrainMaterial::Water {
                        TerrainMapping::Water
                    } else if material.shape().cube {
                        TerrainMapping::Cube
                    } else {
                        TerrainMapping::Planes
                    }),
                    terrain_corners,
                    ..Mesh::default()
                }),
            );
            surfaces.push(TerrainSurface {
                mesh,
                origin,
                layer: draw.layer,
                seam: draw.seam,
            });
        }
        (chunk.chunk, MeshedChunk { surfaces, tops })
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
        let colors = match dom.property_by_name(terrain, "MaterialColors") {
            Some(Value::MaterialColors(c)) => Some(c),
            _ => None,
        };
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

    /// The textures and tint of `layer`'s surfaces, and how it blends on
    /// seams: a
    /// MaterialVariant MaterialService puts in the material's place (its
    /// maps tinted by the colour, as on parts), or else the material's own
    /// pack in `set`, coloured by the material's tint for its
    /// `MaterialColors` colour ([`TerrainMaterial::tint`]). The 2022 set's
    /// colour maps are tint masks: where their alpha is opaque, the
    /// texel's luma times the tint (the packs say `alphamode` 2, TintMask;
    /// CrackedLava's alpha is its cracks, which Studio draws in the
    /// material's colour over dark rock, and Grass's sides have none, so
    /// they show plain earth, as in Studio). The legacy set's are the
    /// texel times the tint. Untextured, a material shows its colour.
    /// A pack's layer blends by its height map (in its roughness-metalness
    /// image, blue) and blend settings; a variant's has no height map, only
    /// the pack's softness (Approximation: what Studio does there isn't
    /// known).
    fn terrain_look(
        &mut self,
        dom: &Dom,
        terrain: InstanceId,
        layer: Layer,
        set: MaterialSet,
        colors: Option<&MaterialColors>,
    ) -> (MaterialMaps, [f32; 3], TerrainLayer) {
        let Layer { material, facing } = layer;
        let look = material.textures(set);
        let texture = match facing {
            Facing::Top => look.top,
            Facing::Side => look.side,
            Facing::Bottom => look.bottom,
        };
        // A variant on terrain is turned and shifted as the material's own
        // texture would be (Inference).
        let detile = texture.map_or(Detile::NONE, |t| Detile {
            shift: t.shift(),
            turn: t.rotation,
        });
        let mut blend = TerrainLayer {
            cube: material.shape().cube,
            scale: texture.map_or(1.0, |t| t.blend.scale),
            detile,
            ..TerrainLayer::default()
        };
        let c = material
            .color(colors, set)
            .unwrap_or(rook_types::Color3uint8::new(255, 255, 255));
        let color = [c.r, c.g, c.b].map(|v| f32::from(v) / 255.0);
        let variant = dom.find_service("MaterialService").and_then(|service| {
            let name = rook_physics::contacts::materials::base_material_override(
                dom,
                service,
                material.material(),
            )?;
            rook_physics::contacts::materials::find_variant(
                dom,
                service,
                material.material(),
                &name,
            )
        });
        if let Some(variant) = variant {
            let [color_map, normal, roughness, metalness] = self.map_images(dom, terrain, variant);
            let maps = MaterialMaps {
                color: color_map,
                normal,
                roughness,
                metalness,
                studs_per_tile: Vec2::splat(
                    dom.float_property(variant, "StudsPerTile").unwrap_or(10.0),
                ),
                detile,
                ..MaterialMaps::NONE
            };
            blend.studs = maps.studs_per_tile.x;
            return (maps, color, blend);
        }
        let Some(texture) = texture else {
            return (MaterialMaps::NONE, color, blend);
        };
        let mut map = |m: MaterialMap| match self.file_state_of(
            terrain,
            ContentRef::TexturePack(texture.pack, m),
            Want::Image,
        ) {
            FileState::Loaded(hash) => Some(ImageHash(hash)),
            FileState::Loading | FileState::Failed => None,
        };
        let roughness_metalness = map(MaterialMap::RoughnessMetalness);
        let maps = MaterialMaps {
            color: map(MaterialMap::Color),
            normal: map(MaterialMap::Normal),
            roughness: roughness_metalness,
            metalness: roughness_metalness,
            studs_per_tile: Vec2::splat(texture.studs()),
            detile,
            alpha: match set {
                MaterialSet::Legacy => MapAlpha::Ignore,
                MaterialSet::Materials2022 => MapAlpha::TintMask,
            },
            ..MaterialMaps::NONE
        };
        let color = match maps.color {
            Some(_) => material.tint(c, set),
            None => color,
        };
        let (a, b) = texture.blend.remap();
        blend.studs = texture.studs();
        if roughness_metalness.is_some() {
            blend.remap = [a, b];
            blend.height = roughness_metalness;
        }
        (maps, color, blend)
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
