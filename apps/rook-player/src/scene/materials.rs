//! What a part's material draws: its textures (`docs/behavior/materials.md`).

use super::*;

impl SceneCache {
    /// The textures of `material`, as they've loaded: Studio's from the
    /// user's content cache, or Rook's own. Studio 0.742 draws the 2022
    /// materials whatever `MaterialService.Use2022Materials` says
    /// (observed), so there is one set. While a texture loads, or if there
    /// is none, the part draws without it and is redrawn when it arrives.
    pub(super) fn material_maps(&mut self, part: InstanceId, material: Material) -> MaterialMaps {
        if !material.has_textures() {
            return MaterialMaps::NONE;
        }
        let mut map = |m: MaterialMap| match self.file_state_of(
            part,
            ContentRef::Material(material, m),
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
            studs_per_tile: Vec2::splat(material.studs_per_tile()),
        }
    }
}
