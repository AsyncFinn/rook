//! What a part's material draws: a SurfaceAppearance, a MaterialVariant or
//! the material's own textures (`docs/behavior/materials.md`).

use super::*;

/// A SurfaceAppearance's or MaterialVariant's map properties: files store
/// the `Content` ones; older files saved the legacy names.
const MAP_PROPERTIES: [[&str; 2]; 4] = [
    ["ColorMapContent", "ColorMap"],
    ["NormalMapContent", "NormalMap"],
    ["RoughnessMapContent", "RoughnessMap"],
    ["MetalnessMapContent", "MetalnessMap"],
];

impl SceneCache {
    /// Set what `part`'s material draws. A SurfaceAppearance on a MeshPart
    /// replaces the mesh's texture and the part's Color (but for Overlay)
    /// with its maps, laid through the mesh's UVs; otherwise the part's
    /// MaterialVariant ([`rook_physics::materials::part_variant`]) replaces
    /// the material's textures with its own; otherwise the material's
    /// textures (Studio's, from the user's content cache, or Rook's own).
    /// While a texture loads, or if there is none, the part draws without
    /// it and is redrawn when it arrives.
    pub(super) fn material_maps(&mut self, dom: &Dom, id: InstanceId, part: &mut PartInstance) {
        part.maps = MaterialMaps::NONE;
        part.material_set = material_set(dom);
        if part.material == Material::Neon {
            return;
        }
        let appearance = (dom.class_name(id) == "MeshPart")
            .then(|| {
                dom.children(id)
                    .iter()
                    .copied()
                    .find(|&c| dom.is_a(c, "SurfaceAppearance"))
            })
            .flatten();
        if let Some(sa) = appearance {
            // Observed: with a SurfaceAppearance the part's Color doesn't
            // show except through Overlay's see-through pixels.
            part.texture = None;
            let [color, normal, roughness, metalness] = self.map_images(dom, id, sa);
            part.maps = MaterialMaps {
                color,
                normal,
                roughness,
                metalness,
                on_mesh_uv: true,
                alpha: match enum_value(dom, sa, "AlphaMode") {
                    Some(1) => MapAlpha::Transparency,
                    Some(2) => MapAlpha::TintMask,
                    Some(3) => MapAlpha::Ignore,
                    _ => MapAlpha::Overlay,
                },
                tint: self::color(dom, sa, "Color").unwrap_or([1.0; 3]),
                ..MaterialMaps::NONE
            };
            return;
        }
        if let Some(variant) = rook_physics::materials::part_variant(dom, id) {
            let [color, normal, roughness, metalness] = self.map_images(dom, id, variant);
            part.maps = MaterialMaps {
                color,
                normal,
                roughness,
                metalness,
                // Confirmed: creator-docs MaterialVariant.StudsPerTile,
                // 10 by default (also read in Studio 0.742).
                studs_per_tile: Vec2::splat(number(dom, variant, "StudsPerTile").unwrap_or(10.0)),
                ..MaterialMaps::NONE
            };
            return;
        }
        part.maps = self.builtin_maps(id, part.material, part.material_set);
    }

    /// The four maps of a SurfaceAppearance or MaterialVariant, as loaded.
    fn map_images(
        &mut self,
        dom: &Dom,
        part: InstanceId,
        owner: InstanceId,
    ) -> [Option<ImageHash>; 4] {
        MAP_PROPERTIES.map(|names| {
            let url = content_url(dom, owner, &names);
            if url.is_empty() {
                return None;
            }
            match self.file_state(part, &url, Want::Image) {
                FileState::Loaded(hash) => Some(ImageHash(hash)),
                FileState::Loading | FileState::Failed => None,
            }
        })
    }

    /// The textures of a built-in `material` in `set`: Studio's from the
    /// user's content cache, or Rook's own.
    fn builtin_maps(
        &mut self,
        part: InstanceId,
        material: Material,
        set: MaterialSet,
    ) -> MaterialMaps {
        if !material.has_textures() {
            return MaterialMaps::NONE;
        }
        let mut map = |m: MaterialMap| match self.file_state_of(
            part,
            ContentRef::Material(material, set, m),
            Want::Image,
        ) {
            FileState::Loaded(hash) => Some(ImageHash(hash)),
            FileState::Loading | FileState::Failed => None,
        };
        let roughness_metalness = map(MaterialMap::RoughnessMetalness);
        MaterialMaps {
            color: map(MaterialMap::Color),
            normal: map(MaterialMap::Normal),
            roughness: roughness_metalness,
            metalness: roughness_metalness,
            studs_per_tile: Vec2::splat(material.studs_per_tile(set)),
            ..MaterialMaps::NONE
        }
    }
}

/// The set of built-in materials the place draws: the 2022 set when
/// MaterialService's `Use2022Materials` is on, else the legacy set, also
/// when the place doesn't save it or has no MaterialService (observed in
/// Studio 0.742). Only a saved value counts, under either name (XML files
/// save `Use2022MaterialsXml`): the class default would hide it.
pub(super) fn material_set(dom: &Dom) -> MaterialSet {
    let db = rook_reflection::database();
    let saved = dom
        .find_service("MaterialService")
        .and_then(|ms| dom.get(ms))
        .and_then(|inst| {
            inst.properties().iter().find_map(|(id, v)| {
                matches!(
                    db.property(*id).name,
                    "Use2022Materials" | "Use2022MaterialsXml"
                )
                .then(|| v.as_bool())
                .flatten()
            })
        });
    MaterialSet::of_place(saved)
}
