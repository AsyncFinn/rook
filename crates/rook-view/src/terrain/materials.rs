//! What each terrain layer's surfaces are drawn with: textures, tint and
//! blending.

use super::*;

impl SceneCache {
    /// The textures and tint of `layer`'s surfaces, and how it blends on seams:
    /// a MaterialVariant MaterialService puts in the material's place, or else
    /// the material's own pack in `set`, coloured by the material's tint
    /// ([`TerrainMaterial::tint`]); untextured, a material shows its colour.
    /// How Studio tints and blends them is in `docs/behavior/terrain.md`.
    pub(super) fn terrain_look(
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
