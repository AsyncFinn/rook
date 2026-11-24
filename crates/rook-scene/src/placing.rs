//! Slots in the drawn lists: taken and released as parts come and go, a
//! part's layers kept in step with its base, effects borrowed alongside.

use super::*;

impl SceneCache {
    pub(super) fn take_index(&mut self, entry: PartInstance, pose: Pose) -> usize {
        let i = match self.free.pop() {
            Some(i) => {
                self.scene.parts[i] = entry;
                self.poses[i] = pose;
                i
            }
            None => {
                self.scene.parts.push(entry);
                self.poses.push(pose);
                self.scene.parts.len() - 1
            }
        };
        self.note_streaming(i);
        i
    }

    /// Stop drawing entry `i`, keeping its index for reuse.
    pub(super) fn release(&mut self, i: usize) {
        self.scene.parts[i].transparency = 1.0;
        self.streaming.remove(&i);
        self.free.push(i);
    }

    /// Draw `id` (when `drawn`) or stop drawing it, adding the entries that
    /// changed to `changed`.
    pub(super) fn place(
        &mut self,
        dom: &Dom,
        id: InstanceId,
        drawn: bool,
        changed: &mut Vec<usize>,
    ) {
        let look = if drawn { self.part(dom, id) } else { None };
        let old_images = self.layers.remove(&id).unwrap_or_default();
        let old_adornments = self.adornment_layers.remove(&id).unwrap_or_default();
        let Some(Drawn {
            part,
            pose,
            images,
            adornments,
        }) = look
        else {
            if let Some(i) = self.index.remove(&id) {
                self.shown.remove(&id);
                self.release(i);
                changed.push(i);
            }
            for i in old_images.into_iter().chain(old_adornments) {
                self.release(i);
                changed.push(i);
            }
            return;
        };
        let base = match self.index.get(&id) {
            Some(&i) => {
                self.scene.parts[i] = part;
                self.poses[i] = pose;
                self.note_streaming(i);
                i
            }
            None => {
                let i = self.take_index(part, pose);
                self.index.insert(id, i);
                i
            }
        };
        changed.push(base);
        let images = self.set_layers(base, old_images, images, Some(changed));
        let adornments = self.set_layers(base, old_adornments, adornments, Some(changed));
        if !images.is_empty() {
            self.layers.insert(id, images);
        }
        if !adornments.is_empty() {
            self.adornment_layers.insert(id, adornments);
        }
    }

    /// Draw `layers` over entry `base` at `old`'s indices, in order (extra
    /// ones take free indices, leftover ones are released), adding the
    /// entries that changed to `changed`; the indices they're at now.
    pub(super) fn set_layers(
        &mut self,
        base: usize,
        old: Vec<usize>,
        layers: Vec<(PartInstance, Pose)>,
        mut changed: Option<&mut Vec<usize>>,
    ) -> Vec<usize> {
        let mut kept = Vec::with_capacity(layers.len());
        let mut old = old.into_iter();
        for (layer, pose) in layers {
            let layer = PartInstance {
                drawn_over: Some(base),
                space: self.scene.parts[base].space,
                ..layer
            };
            let i = match old.next() {
                Some(i) => {
                    self.scene.parts[i] = layer;
                    self.poses[i] = pose;
                    self.note_streaming(i);
                    i
                }
                None => self.take_index(layer, pose),
            };
            kept.push(i);
            if let Some(c) = changed.as_deref_mut() {
                c.push(i);
            }
        }
        for i in old {
            self.release(i);
            if let Some(c) = changed.as_deref_mut() {
                c.push(i);
            }
        }
        kept
    }

    /// Run `f` on the effect cache with the scene's effect lists and a way
    /// to load the effects' images.
    pub(super) fn with_effects<R>(
        &mut self,
        f: impl FnOnce(&mut EffectCache, EffectLists<'_>, &mut Images<'_>) -> R,
    ) -> R {
        let mut cache = std::mem::take(&mut self.effects);
        let mut emitters = std::mem::take(&mut self.scene.emitters);
        let mut beams = std::mem::take(&mut self.scene.beams);
        let mut trails = std::mem::take(&mut self.scene.trails);
        let mut floor_wires = std::mem::take(&mut self.scene.floor_wires);
        let floors = std::mem::take(&mut self.floors);
        let drawn = EffectLists {
            emitters: &mut emitters,
            beams: &mut beams,
            trails: &mut trails,
            floor_wires: &mut floor_wires,
            floors: &floors,
        };
        let mut images =
            |id: InstanceId, url: &str| match self.file_state(id, url, Want::WHOLE_TEXTURE) {
                FileState::Loaded(hash) => Some(ImageHash(hash)),
                FileState::Loading | FileState::Failed => None,
            };
        let out = f(&mut cache, drawn, &mut images);
        self.effects = cache;
        self.scene.emitters = emitters;
        self.scene.beams = beams;
        self.scene.trails = trails;
        self.scene.floor_wires = floor_wires;
        self.floors = floors;
        out
    }
}
