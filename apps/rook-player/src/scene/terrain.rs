//! Workspace's Terrain: its `SmoothGrid` voxels meshed by `rook_terrain`,
//! each chunk's surface split by material and by which way it faces, and
//! drawn with that material's top, side or bottom textures from Studio's
//! terrain material table (or the MaterialVariant MaterialService puts in
//! its place), tinted by `Terrain.MaterialColors`. Water is drawn in
//! `WaterColor` at `WaterTransparency`, waving and rippled by the install's
//! water normal map, and Grass grows blades near the camera ([`grass`])
//! with `Decoration` on (`docs/behavior/terrain.md`).

mod grass;

use rayon::prelude::*;
use rook_content::MaterialMap;
use rook_content::mesh::{Mesh, TerrainMapping};
use rook_render::TerrainLook;
use rook_terrain::grass::{GrassTops, add_grass_tops, blade_color};
use rook_terrain::{Facing, Grid, TerrainMaterial, chunks_to_mesh, mesh_chunk};
use rook_types::MaterialColors;

use super::*;
use grass::{GRASS_REACH, Grass};

/// The install's water normal map (DXT5nm), rippling the water's surface.
/// Studio animates 25 of them (`normal_01` to `normal_25`); Rook scrolls
/// the first.
const WATER_NORMALS: &str = "rbxasset://textures/water/normal_01.dds";

/// One chunk's surface of one material facing one way, as a mesh.
#[derive(Debug, Clone, Copy)]
struct TerrainSurface {
    mesh: MeshHash,
    origin: Vec3,
    material: TerrainMaterial,
    facing: Facing,
}

/// The terrain's surfaces, kept until its `SmoothGrid` changes, and the
/// scene entries drawing them.
#[derive(Debug, Default)]
pub(super) struct TerrainCache {
    /// The hash of the `SmoothGrid` the meshes were built from.
    source: Option<blake3::Hash>,
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
        let terrain = dom
            .find_service("Workspace")
            .and_then(|ws| dom.find_first_child_of_class(ws, "Terrain"));
        // Borrowed, not cloned: a large map's grid is megabytes.
        let db = rook_reflection::database();
        let grid = terrain.and_then(|t| {
            dom.get(t)?.properties().iter().find_map(|(id, v)| match v {
                Value::BinaryString(b) if db.property(*id).name == "SmoothGrid" => {
                    Some(AsRef::<[u8]>::as_ref(b))
                }
                _ => None,
            })
        });
        let source = grid.map(blake3::hash);
        if source != self.terrain.source {
            self.terrain.source = source;
            self.terrain.surfaces = match grid {
                Some(bytes) => self.mesh_terrain(bytes),
                None => {
                    self.terrain.grass.tops = GrassTops::default();
                    Vec::new()
                }
            };
        }
        self.terrain.instance = terrain;
        for mesh in self.terrain.grass.reset() {
            self.scene.meshes.remove(&mesh);
        }
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
        let entries = self.terrain_entries(dom);
        self.terrain.first = self.scene.parts.len();
        self.terrain.count = entries.len();
        for entry in entries {
            self.scene.parts.push(entry);
            self.poses.push(Pose::default());
        }
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

    fn mesh_terrain(&mut self, bytes: &[u8]) -> Vec<TerrainSurface> {
        profiling::scope!("mesh_terrain");
        let grid = match Grid::decode(bytes) {
            Ok(grid) => grid,
            Err(e) => {
                eprintln!("warning: Terrain.SmoothGrid doesn't decode: {e}");
                return Vec::new();
            }
        };
        let source = blake3::hash(bytes);
        let meshes: Vec<_> = chunks_to_mesh(&grid)
            .par_iter()
            .map(|c| mesh_chunk(&grid, *c))
            .collect();
        let mut tops = GrassTops::default();
        for chunk in &meshes {
            add_grass_tops(chunk, &mut tops);
        }
        self.terrain.grass.tops = tops;
        let mut surfaces = Vec::new();
        for chunk in meshes {
            let origin = chunk.origin();
            for surface in chunk.surfaces {
                let mut key = blake3::Hasher::new();
                key.update(source.as_bytes());
                key.update(&[surface.material as u8, surface.facing as u8]);
                for v in chunk.chunk.to_array() {
                    key.update(&v.to_le_bytes());
                }
                let mesh = MeshHash(*key.finalize().as_bytes());
                let count = surface.positions.len();
                let look = surface.material.shape();
                self.scene.meshes.insert(
                    mesh,
                    Arc::new(Mesh {
                        positions: surface.positions,
                        normals: surface.normals,
                        uvs: vec![[0.0; 2]; count],
                        indices: surface.indices,
                        terrain: Some(if surface.material == TerrainMaterial::Water {
                            TerrainMapping::Water
                        } else if look.cube {
                            TerrainMapping::Cube
                        } else {
                            TerrainMapping::Planes
                        }),
                        ..Mesh::default()
                    }),
                );
                surfaces.push(TerrainSurface {
                    mesh,
                    origin,
                    material: surface.material,
                    facing: surface.facing,
                });
            }
        }
        surfaces
    }

    /// The entries drawing the terrain's surfaces, with what has loaded of
    /// their textures; the Terrain waits for the rest.
    fn terrain_entries(&mut self, dom: &Dom) -> Vec<PartInstance> {
        let Some(terrain) = self.terrain.instance else {
            return Vec::new();
        };
        // Terrain draws the 2022 set's textures and colours in every place
        // (`TerrainMaterial::textures`).
        let set = MaterialSet::Materials2022;
        let colors = match dom.property_by_name(terrain, "MaterialColors") {
            Some(Value::MaterialColors(c)) => Some(c),
            _ => None,
        };
        let water =
            color(dom, terrain, "WaterColor").unwrap_or([12.0 / 255.0, 84.0 / 255.0, 92.0 / 255.0]);
        let water_transparency = number(dom, terrain, "WaterTransparency").unwrap_or(0.3);
        // Most of Studio's water colour is the sky it reflects: the shader
        // mixes towards the sky's colour by `WaterReflectance`, half of it
        // looking down (Approximation).
        let water_reflectance = number(dom, terrain, "WaterReflectance").unwrap_or(1.0);
        let water_normals = ContentRef::parse(WATER_NORMALS).and_then(|r| {
            match self.file_state_of(terrain, r, Want::Image) {
                FileState::Loaded(hash) => Some(ImageHash(hash)),
                FileState::Loading | FileState::Failed => None,
            }
        });
        let surfaces = self.terrain.surfaces.clone();
        let mut looks: HashMap<(TerrainMaterial, Facing), (MaterialMaps, [f32; 3])> =
            HashMap::new();
        let mut entries = Vec::with_capacity(surfaces.len());
        for s in surfaces {
            let mut part =
                PartInstance::plain(DrawShape::File(s.mesh), s.origin, Mat3::IDENTITY, Vec3::ONE);
            part.material = s.material.material();
            part.material_set = set;
            if s.material == TerrainMaterial::Water {
                part.color = water;
                part.transparency = water_transparency;
                part.reflectance = water_reflectance.clamp(0.0, 1.0);
                part.cast_shadow = false;
                part.maps.normal = water_normals;
            } else {
                let (maps, color) = *looks.entry((s.material, s.facing)).or_insert_with(|| {
                    self.terrain_look(dom, terrain, s.material, s.facing, set, colors.as_ref())
                });
                part.maps = maps;
                part.color = color;
            }
            entries.push(part);
        }
        entries
    }

    /// The textures and tint of `material`'s surfaces facing `facing`: a
    /// MaterialVariant MaterialService puts in the material's place (its
    /// maps tinted by the colour, as on parts), or else the material's own
    /// pack, whose colour map is a tint mask: the `MaterialColors` colour
    /// tints it where its alpha is opaque (the packs say `alphamode` 2,
    /// TintMask; CrackedLava's alpha is its cracks, which Studio draws in
    /// the material's colour over dark rock, and Grass's sides have none,
    /// so they show plain earth, as in Studio). Untextured, a material
    /// shows its colour.
    fn terrain_look(
        &mut self,
        dom: &Dom,
        terrain: InstanceId,
        material: TerrainMaterial,
        facing: Facing,
        set: MaterialSet,
        colors: Option<&MaterialColors>,
    ) -> (MaterialMaps, [f32; 3]) {
        let c = material
            .color(colors, set)
            .unwrap_or(rook_types::Color3uint8::new(255, 255, 255));
        let color = [c.r, c.g, c.b].map(|v| f32::from(v) / 255.0);
        let variant = dom.find_service("MaterialService").and_then(|service| {
            let name =
                rook_physics::materials::base_material_override(dom, service, material.material())?;
            rook_physics::materials::find_variant(dom, service, material.material(), &name)
        });
        if let Some(variant) = variant {
            let [color_map, normal, roughness, metalness] = self.map_images(dom, terrain, variant);
            let maps = MaterialMaps {
                color: color_map,
                normal,
                roughness,
                metalness,
                studs_per_tile: Vec2::splat(number(dom, variant, "StudsPerTile").unwrap_or(10.0)),
                ..MaterialMaps::NONE
            };
            return (maps, color);
        }
        let look = material.textures();
        let texture = match facing {
            Facing::Top => look.top,
            Facing::Side => look.side,
            Facing::Bottom => look.bottom,
        };
        let Some(texture) = texture else {
            return (MaterialMaps::NONE, color);
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
            ..MaterialMaps::NONE
        };
        (maps, color)
    }
}

/// How the terrain's water waves and its grass moves: `WaterWaveSize`,
/// `WaterWaveSpeed` (Studio's defaults 0.15 and 10), `Workspace.GlobalWind`,
/// and how far grass is drawn with `Decoration` on.
fn terrain_look(dom: &Dom, terrain: Option<InstanceId>) -> TerrainLook {
    let mut look = TerrainLook::default();
    if let Some(t) = terrain {
        look.wave_size = number(dom, t, "WaterWaveSize").unwrap_or(look.wave_size);
        look.wave_speed = number(dom, t, "WaterWaveSpeed").unwrap_or(look.wave_speed);
        if boolean(dom, t, "Decoration").unwrap_or(false) {
            look.grass_reach = GRASS_REACH;
        }
    }
    if let Some(ws) = dom.find_service("Workspace") {
        look.wind = vector3(dom, ws, "GlobalWind").unwrap_or(Vec3::ZERO);
    }
    look
}
