//! The effect cache's bookkeeping: effects placed into the drawn lists,
//! reread when they change, moved with what holds them and released.

use super::*;

impl EffectCache {
    pub(crate) fn new(textures: EffectTextures) -> Self {
        Self {
            textures: Some(textures),
            ..Default::default()
        }
    }

    pub(crate) fn has(&self, id: InstanceId) -> bool {
        self.effects.contains_key(&id)
    }

    pub(crate) fn take_changes(&mut self) -> EffectSlots {
        std::mem::take(&mut self.changed)
    }

    /// Find every effect in Workspace (a new scene), and read them all.
    pub(crate) fn build(&mut self, dom: &Dom, mut drawn: EffectLists<'_>, images: &mut Images<'_>) {
        profiling::scope!("EffectCache::build");
        drawn.emitters.clear();
        drawn.beams.clear();
        drawn.trails.clear();
        drawn.floor_wires.clear();
        self.effects.clear();
        self.held.clear();
        self.free = Free::default();
        self.changed = EffectSlots::default();
        let Some(ws) = dom.find_service("Workspace") else {
            self.calls.clear();
            return;
        };
        let mut ids = dom.descendants(ws);
        ids.retain(|&id| is_effect(dom, id));
        let known: FxHashSet<InstanceId> = ids.iter().copied().collect();
        self.calls.retain(|id, _| known.contains(id));
        for id in ids {
            self.place(dom, id, &mut drawn, images);
        }
    }

    /// Count scripts' effect calls, before the effects are read (again).
    pub(crate) fn note_calls(&mut self, calls: &[(InstanceId, EffectCall)]) {
        for &(id, call) in calls {
            let calls = self.calls.entry(id).or_default();
            match call {
                EffectCall::Emit(n) => calls.emitted += u64::from(n),
                EffectCall::Clear => calls.cleared += 1,
                EffectCall::TextureOffset(offset) => calls.texture_offset = offset,
                // A wireframe's (`adornments::wireframe`).
                EffectCall::Line(..) => {}
            }
        }
    }

    /// Bring the effects up to date with a frame's changes (their calls
    /// already noted): effects entering or leaving Workspace come or go,
    /// those whose properties or calls changed are read again, and those
    /// on parts and Attachments that only moved only move.
    pub(crate) fn changed(
        &mut self,
        dom: &Dom,
        changes: &RenderChanges,
        mut drawn: EffectLists<'_>,
        images: &mut Images<'_>,
    ) {
        // Freed: dropped unread.
        for id in &changes.collected {
            self.remove(*id, &mut drawn);
        }
        // Placed again: effects that came or went, those whose holders did,
        // and those whose properties changed (a Beam's Attachment0 or a
        // FloorWire's From may hang them from something else).
        let mut placed: FxHashSet<InstanceId> = (changes.effects_placed.iter())
            .chain(changes.effects.iter().filter(|&&id| is_effect(dom, id)))
            .copied()
            .collect();
        let holders = (changes.collected.iter())
            .chain(&changes.placed)
            .chain(&changes.effects_placed);
        for id in holders {
            placed.extend(self.held.get(id).into_iter().flatten());
        }
        for &id in &placed {
            self.place(dom, id, &mut drawn, images);
        }
        let mut stale = FxHashSet::default();
        let read = (changes.effects.iter())
            .chain(&changes.parts)
            .chain(changes.effect_calls.iter().map(|(id, _)| id));
        for id in read {
            stale.extend(self.held.get(id).into_iter().flatten());
        }
        stale.retain(|id| !placed.contains(id));
        let mut moved = FxHashSet::default();
        for id in &changes.moved {
            moved.extend(self.held.get(id).into_iter().flatten());
        }
        moved.retain(|id| !placed.contains(id) && !stale.contains(id));
        for id in stale {
            self.reread(dom, id, &mut drawn, images);
        }
        for id in moved {
            self.moved(dom, id, &mut drawn);
        }
    }

    /// Read the effects `ids` again (their images loaded, or scripts
    /// called them), those drawn here.
    pub(crate) fn reread_ids(
        &mut self,
        dom: &Dom,
        ids: &[InstanceId],
        mut drawn: EffectLists<'_>,
        images: &mut Images<'_>,
    ) {
        for &id in ids {
            if self.has(id) {
                self.reread(dom, id, &mut drawn, images);
            }
        }
    }

    /// Read effect `id` again, adding it if it's new to Workspace's world
    /// and dropping it if it left. One in a ViewportFrame draws nothing
    /// (observed), even a frame in Workspace.
    pub(super) fn place(
        &mut self,
        dom: &Dom,
        id: InstanceId,
        drawn: &mut EffectLists<'_>,
        images: &mut Images<'_>,
    ) {
        let here = dom.get(id).is_some()
            && is_effect(dom, id)
            && crate::viewports::space(dom, id) == Some(rook_render::Space::Workspace);
        if !here {
            self.remove(id, drawn);
            return;
        }
        let holders = holders(dom, id);
        let slot = match self.effects.get(&id) {
            Some(entry) if entry.holders == holders => None,
            Some(entry) => {
                let slot = entry.slot.clone();
                self.unhold(id);
                Some(slot)
            }
            None => {
                let (free, changed) = (&mut self.free, &mut self.changed);
                Some(match dom.class_name(id) {
                    "Beam" => Slot::Beam(take(drawn.beams, &mut free.beams, &mut changed.beams)),
                    "Trail" => {
                        Slot::Trail(take(drawn.trails, &mut free.trails, &mut changed.trails))
                    }
                    "FloorWire" => Slot::FloorWire(take(
                        drawn.floor_wires,
                        &mut free.floor_wires,
                        &mut changed.floor_wires,
                    )),
                    _ => Slot::Emitters(Vec::new()),
                })
            }
        };
        if let Some(slot) = slot {
            for &h in &holders {
                self.held.entry(h).or_default().push(id);
            }
            self.effects.insert(id, Entry { slot, holders });
        }
        self.reread(dom, id, drawn, images);
    }

    pub(super) fn remove(&mut self, id: InstanceId, drawn: &mut EffectLists<'_>) {
        if !self.has(id) {
            return;
        }
        self.unhold(id);
        self.calls.remove(&id);
        let entry = self.effects.remove(&id).expect("drawn");
        let (free, changed) = (&mut self.free, &mut self.changed);
        match entry.slot {
            Slot::Emitters(slots) => {
                for i in slots {
                    release(drawn.emitters, i, &mut free.emitters, &mut changed.emitters);
                }
            }
            Slot::Beam(i) => release(drawn.beams, i, &mut free.beams, &mut changed.beams),
            Slot::Trail(i) => release(drawn.trails, i, &mut free.trails, &mut changed.trails),
            Slot::FloorWire(i) => release(
                drawn.floor_wires,
                i,
                &mut free.floor_wires,
                &mut changed.floor_wires,
            ),
        }
    }

    pub(super) fn unhold(&mut self, id: InstanceId) {
        for h in &self.effects[&id].holders {
            if let Some(effects) = self.held.get_mut(h) {
                effects.retain(|e| *e != id);
                if effects.is_empty() {
                    self.held.remove(h);
                }
            }
        }
    }

    pub(super) fn reread(
        &mut self,
        dom: &Dom,
        id: InstanceId,
        drawn: &mut EffectLists<'_>,
        images: &mut Images<'_>,
    ) {
        let mut reader = Reader {
            dom,
            floors: drawn.floors,
            images,
            textures: self.textures.as_ref().expect("made with textures"),
            calls: self.calls.get(&id).copied().unwrap_or_default(),
        };
        let entry = self.effects.get_mut(&id).expect("drawn");
        let changed = &mut self.changed;
        match &mut entry.slot {
            Slot::Emitters(slots) => {
                let free = &mut self.free.emitters;
                let read = reader.emitters(id);
                // Each emitter keeps its slot; extra slots are freed.
                for i in slots.drain(read.len().min(slots.len())..) {
                    release(drawn.emitters, i, free, &mut changed.emitters);
                }
                for (n, e) in read.into_iter().enumerate() {
                    if n == slots.len() {
                        slots.push(take(drawn.emitters, free, &mut changed.emitters));
                    }
                    drawn.emitters[slots[n]] = Some(e);
                    changed.emitters.push(slots[n]);
                }
            }
            &mut Slot::Beam(i) => {
                drawn.beams[i] = Some(reader.beam(id));
                changed.beams.push(i);
            }
            &mut Slot::Trail(i) => {
                drawn.trails[i] = Some(reader.trail(id));
                changed.trails.push(i);
            }
            &mut Slot::FloorWire(i) => {
                drawn.floor_wires[i] = Some(reader.floor_wire(id));
                changed.floor_wires.push(i);
            }
        }
    }

    /// Drawn effect `id`'s holder moved: carry it along.
    pub(super) fn moved(&mut self, dom: &Dom, id: InstanceId, drawn: &mut EffectLists<'_>) {
        let changed = &mut self.changed;
        match &self.effects[&id].slot {
            Slot::Emitters(slots) => {
                let Some(frame) = holder_frame(dom, id) else {
                    return;
                };
                for &i in slots {
                    if let Some(e) = &mut drawn.emitters[i] {
                        e.position = frame.position;
                        e.rotation = frame.rotation;
                        changed.emitters.push(i);
                    }
                }
            }
            &Slot::Beam(i) => {
                if let (Some(ends), Some(beam)) = (ribbons::anchors(dom, id), &mut drawn.beams[i]) {
                    beam.ends = ends;
                    changed.beams.push(i);
                }
            }
            &Slot::Trail(i) => {
                if let (Some([a, z]), Some(trail)) =
                    (ribbons::anchors(dom, id), &mut drawn.trails[i])
                {
                    trail.ends = [a.position, z.position];
                    changed.trails.push(i);
                }
            }
            &Slot::FloorWire(i) => {
                if let Some(wire) = &mut drawn.floor_wires[i] {
                    wire.path = super::adornments::floor_wire::path(dom, id, drawn.floors)
                        .unwrap_or_default();
                    changed.floor_wires.push(i);
                }
            }
        }
    }
}

/// A slot in `list` for a new entry: a free one, or a new one at the end.
pub(super) fn take<T>(
    list: &mut Vec<Option<T>>,
    free: &mut Vec<usize>,
    changed: &mut Vec<usize>,
) -> usize {
    let i = free.pop().unwrap_or_else(|| {
        list.push(None);
        list.len() - 1
    });
    changed.push(i);
    i
}

pub(super) fn release<T>(
    list: &mut [Option<T>],
    i: usize,
    free: &mut Vec<usize>,
    changed: &mut Vec<usize>,
) {
    list[i] = None;
    free.push(i);
    changed.push(i);
}

impl Reader<'_, '_> {
    pub(super) fn texture(&mut self, id: InstanceId) -> Option<ImageHash> {
        let url = content_url(self.dom, id, &["TextureContent", "Texture"]);
        if url.is_empty() {
            return None;
        }
        (self.images)(id, &url)
    }
}
