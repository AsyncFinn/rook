//! Render changes applied to the cache: parts placed, moved, restyled or
//! dropped, and what each change dirties.

use super::*;

impl SceneCache {
    pub(crate) fn apply(&mut self, dom: &Dom, changes: &RenderChanges) -> SceneChange {
        profiling::scope!("SceneCache::apply");
        self.model_highlights.clear();
        self.effects.note_calls(&changes.effect_calls);
        let wired =
            adornments::wireframe::note_calls(dom, &mut self.wire_lines, &changes.effect_calls);
        let mut changed = Vec::new();
        // Freed: nothing of them may be read, only dropped.
        for &id in &changes.collected {
            self.place(dom, id, false, &mut changed);
            self.wire_lines.remove(&id);
            self.highlights_changed |= self.scene.highlights.remove(&id.to_ref().0).is_some();
        }
        self.drop_viewports(changes.collected.iter().copied());
        if !changes.collected.is_empty() {
            for parts in self.waiting.values_mut() {
                parts.retain(|p| !changes.collected.contains(p));
            }
        }
        self.note_voxels(changes);
        if changes.full {
            self.rebuild(dom);
            return SceneChange::Full;
        }
        if changes.lighting {
            self.scene.lighting = self.lighting(dom);
        }
        self.lights.apply(dom, changes, &mut self.scene.lights);
        self.with_effects(|effects, drawn, images| effects.changed(dom, changes, drawn, images));
        let mut moved = Vec::new();
        if changes.terrain || !changes.terrain_edits.is_empty() {
            changed.extend(self.terrain_changed(dom, changes.terrain));
        }
        self.update_viewports(dom, changes.viewports.iter().copied());
        for id in &changes.placed {
            // Only parts in Workspace or a ViewportFrame are drawn.
            let drawn = is_part(dom, *id) && viewports::space(dom, *id).is_some();
            self.place(dom, *id, drawn, &mut changed);
        }
        for id in changes.parts.iter().chain(&wired) {
            if self.index.contains_key(id) {
                self.place(dom, *id, true, &mut changed);
            }
        }
        // Adornments and Highlights alone: the parts' own looks stay.
        // Nothing adorns or covers a part in a ViewportFrame (observed).
        let in_workspace = |cache: &Self, i: usize| cache.scene.parts[i].space == Space::Workspace;
        for id in &changes.adorned {
            if let Some(&base) = self.index.get(id)
                && in_workspace(self, base)
            {
                let old = self.adornment_layers.remove(id).unwrap_or_default();
                let new = self.adornments(dom, *id);
                let kept = self.set_layers(base, old, new, Some(&mut changed));
                if !kept.is_empty() {
                    self.adornment_layers.insert(*id, kept);
                }
            }
        }
        for id in &changes.highlighted {
            if let Some(&i) = self.index.get(id)
                && in_workspace(self, i)
            {
                let covering = self.highlight(dom, *id);
                if self.scene.parts[i].highlight != covering {
                    self.scene.parts[i].highlight = covering;
                    changed.push(i);
                }
            }
        }
        for &id in &changes.highlights {
            let key = id.to_ref().0;
            if let Some(look) = self.scene.highlights.get_mut(&key) {
                *look = adornments::highlight::look(dom, id);
                self.highlights_changed = true;
            }
        }
        // Parts that only moved keep their look: place it at the new CFrame.
        for id in &changes.moved {
            let Some(&i) = self.index.get(id) else {
                continue;
            };
            let Some(cf) = dom.drawn_cframe(*id) else {
                continue;
            };
            let layers = [&self.layers, &self.adornment_layers].map(|l| l.get(id));
            let entries = std::iter::once(i).chain(layers.into_iter().flatten().flatten().copied());
            for i in entries {
                let pose = self.poses[i];
                let part = &mut self.scene.parts[i];
                part.position = cf.position + cf.rotation * pose.offset;
                part.rotation = cf.rotation * pose.turn;
                moved.push(i);
            }
        }
        let lighting = changes.lighting || changes.terrain;
        let highlights = std::mem::take(&mut self.highlights_changed);
        if changed.is_empty() && moved.is_empty() && !lighting && !highlights {
            SceneChange::None
        } else {
            SceneChange::Parts {
                changed,
                moved,
                lighting,
                highlights,
            }
        }
    }
}
