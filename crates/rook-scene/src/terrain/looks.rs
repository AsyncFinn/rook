//! Terrain's look: its material colors and textures read from the
//! Terrain and turned into each surface's draw entry.

use super::*;

impl SceneCache {
    /// The Terrain's waves and wind, and whether and in what color Grass
    /// grows blades.
    pub(super) fn read_terrain_look(&mut self, dom: &Dom) {
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

    /// Read what each layer the voxels hold is drawn with: its textures as
    /// far as they've loaded (the Terrain waits for the rest), color and
    /// blending, and the water's look. Terrain draws the legacy set's
    /// textures in every place (`TerrainMaterial::look`), with the place's
    /// set's default colors where it saved none.
    pub(super) fn read_looks(&mut self, dom: &Dom) {
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
    pub(super) fn entry(&self, s: &TerrainSurface) -> PartInstance {
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
pub(super) fn terrain_look(dom: &Dom, terrain: Option<InstanceId>) -> TerrainLook {
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
