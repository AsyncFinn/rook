//! What each terrain layer's surfaces are drawn with: textures and color.

use super::*;

impl SceneCache {
    /// The textures and color of `layer`'s surfaces: a MaterialVariant MaterialService puts in the material's
    /// place, tinted by the material's color, or else the material's own
    /// pack in the place's set, colored as that set is
    /// ([`TerrainMaterial::coloring`]; a recoloring is sent in the color's
    /// place); untextured, a material shows its color. `set` also gives
    /// the default colors of a place that saved none. How Studio colors
    /// them is in `docs/behavior/terrain/materials.md`.
    pub(super) fn terrain_look(
        &mut self,
        dom: &Dom,
        terrain: InstanceId,
        layer: Layer,
        set: MaterialSet,
        colors: Option<&MaterialColors>,
    ) -> (MaterialMaps, [f32; 3]) {
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
            return (maps, color);
        }
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
            detile,
            ..MaterialMaps::NONE
        };
        let (maps, color) = match (maps.color, material.coloring(set, c)) {
            (None, _) => (maps, color),
            (Some(_), Coloring::Recolor(r)) => (
                MaterialMaps {
                    terrain_color: TerrainColor::Recolor,
                    ..maps
                },
                r.to_array(),
            ),
            (Some(_), Coloring::Tint(t)) => (
                MaterialMaps {
                    terrain_color: TerrainColor::Tint,
                    ..maps
                },
                t,
            ),
        };
        (maps, color)
    }
}
