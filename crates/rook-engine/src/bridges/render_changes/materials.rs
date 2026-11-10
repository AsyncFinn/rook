//! Which parts a MaterialVariant or a MaterialService override can change
//! the look of, so changing one redraws those parts and not the scene.

use rook_physics::contacts::materials::{base_material_override, own_variant_name};
use rook_terrain::TerrainMaterial;
use rook_types::Material;
use rustc_hash::{FxHashMap, FxHashSet};

use super::{RenderChanges, RenderIds};
use crate::*;

/// Parts by material and by the variant name they ask for, and each
/// MaterialVariant's name and base material, built once and then kept up
/// to date from the journal.
#[derive(Debug, Default)]
pub(crate) struct MaterialParts {
    by_material: FxHashMap<Material, FxHashSet<InstanceId>>,
    by_name: FxHashMap<String, FxHashSet<InstanceId>>,
    /// What each part is filed under, to move it when that changes.
    parts: FxHashMap<InstanceId, (Material, Option<String>)>,
    /// Each variant as last seen: once it's renamed or rebased, the parts
    /// that drew it must redraw too.
    variants: FxHashMap<InstanceId, (String, Material)>,
}

impl MaterialParts {
    pub(crate) fn build(&mut self, dom: &Dom) {
        *self = Self::default();
        for id in dom.descendants(dom.root()) {
            self.file(dom, id);
        }
    }

    /// `id` is new to the tree, moved, or its properties changed: file it
    /// again, and if it's a variant, redraw the parts it drew and draws.
    pub(crate) fn note(&mut self, dom: &Dom, id: InstanceId, changes: &mut RenderChanges) {
        for (name, base) in self.file(dom, id) {
            self.variant_changed(dom, &name, base, changes);
        }
    }

    /// File `id` as it is now; for a variant, its name and base material
    /// as they were and are.
    fn file(&mut self, dom: &Dom, id: InstanceId) -> Vec<(String, Material)> {
        let ids = RenderIds::get();
        if dom.is_a_class(id, ids.material_variant) {
            let now = (
                dom.name(id).to_owned(),
                Material::of_part(dom.enum_property(id, "BaseMaterial")),
            );
            let mut keys = vec![now.clone()];
            keys.extend(self.variants.insert(id, now).filter(|b| *b != keys[0]));
            return keys;
        }
        if dom.is_a_class(id, ids.base_part) && !dom.is_a_class(id, ids.terrain) {
            let now = (
                Material::of_part(dom.enum_property(id, "Material")),
                own_variant_name(dom, id),
            );
            match self.parts.insert(id, now.clone()) {
                Some(before) if before == now => return Vec::new(),
                Some(before) => self.unfile(id, before),
                None => {}
            }
            let (material, name) = now;
            self.by_material.entry(material).or_default().insert(id);
            if let Some(name) = name {
                self.by_name.entry(name).or_default().insert(id);
            }
        }
        Vec::new()
    }

    pub(crate) fn forget(&mut self, id: InstanceId) {
        self.variants.remove(&id);
        if let Some(filed) = self.parts.remove(&id) {
            self.unfile(id, filed);
        }
    }

    fn unfile(&mut self, id: InstanceId, (material, name): (Material, Option<String>)) {
        if let Some(set) = self.by_material.get_mut(&material) {
            set.remove(&id);
        }
        if let Some(set) = name.as_ref().and_then(|n| self.by_name.get_mut(n)) {
            set.remove(&id);
        }
    }

    /// A variant named `name` for `base` appeared, went or changed: redraw
    /// what can draw it (`part_variant`'s rule): parts of `base` asking for
    /// it by name, and if MaterialService's override for `base` names it,
    /// the parts of `base` asking for none, and the terrain where `base`
    /// is a terrain material.
    fn variant_changed(&self, dom: &Dom, name: &str, base: Material, changes: &mut RenderChanges) {
        let of_base = |p: &&InstanceId| self.parts.get(p).is_some_and(|(m, _)| *m == base);
        let named = self.by_name.get(name).into_iter().flatten();
        changes.parts.extend(named.filter(of_base));
        let overridden = dom
            .find_service("MaterialService")
            .and_then(|s| base_material_override(dom, s, base))
            .is_some_and(|o| o == name);
        if overridden {
            let plain = |p: &&InstanceId| self.parts.get(p).is_some_and(|(_, n)| n.is_none());
            let parts = self.by_material.get(&base).into_iter().flatten();
            changes.parts.extend(parts.filter(plain));
            changes.terrain |= TerrainMaterial::from_material(base).is_some();
        }
    }

    /// MaterialService's override for `material` changed: its parts asking
    /// for no variant themselves, and its terrain, may draw another.
    pub(crate) fn override_changed(&self, material: Material, changes: &mut RenderChanges) {
        let parts = self.by_material.get(&material).into_iter().flatten();
        let plain = |p: &&InstanceId| self.parts.get(p).is_some_and(|(_, n)| n.is_none());
        changes.parts.extend(parts.filter(plain));
        changes.terrain |= TerrainMaterial::from_material(material).is_some();
    }
}
