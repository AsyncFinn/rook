//! Visual effects in Workspace (docs/behavior/effects/classic.md,
//! docs/behavior/effects/particle-emitters.md, docs/behavior/effects/beams-and-trails.md,
//! docs/behavior/effects/explosions.md): Fire, Smoke, Sparkles, ParticleEmitter
//! and Explosion as the renderer's particle emitters, Beam and Trail as
//! its ribbons.

mod classic;
mod emitter;
mod explosion;
mod ribbons;
mod textures;

use rook_engine::{EffectCall, is_effect_class};
use rook_render::{Beam, Emitter, Trail};

pub(super) use textures::EffectTextures;

use super::*;

/// Whether `id` is an effect this module draws.
pub(super) fn is_effect(dom: &Dom, id: InstanceId) -> bool {
    is_effect_class(dom.class_name(id))
}

/// Where an effect is drawn in the scene's lists. Each keeps its place
/// while it exists, so particles and Trail pieces stay with it.
#[derive(Debug, Clone, Copy)]
enum Slot {
    Emitters { first: usize, count: usize },
    Beam(usize),
    Trail(usize),
}

/// What scripts did to an effect beyond its properties.
#[derive(Debug, Clone, Copy, Default)]
struct Calls {
    emitted: u64,
    cleared: u64,
    texture_offset: f32,
}

/// The scene's effect lists, which the cache keeps.
pub(super) struct EffectLists<'a> {
    pub(super) emitters: &'a mut Vec<Emitter>,
    pub(super) beams: &'a mut Vec<Beam>,
    pub(super) trails: &'a mut Vec<Trail>,
}

/// An effect's image by URL: its hash once loaded (noting that the effect
/// waits for it while it loads).
pub(super) type Images<'a> = dyn FnMut(InstanceId, &str) -> Option<ImageHash> + 'a;

/// How an effect is read: the document, its images, the textures Fire,
/// Smoke and Sparkles draw, and what scripts did to it.
struct Reader<'a, 'b> {
    dom: &'a Dom,
    images: &'a mut Images<'b>,
    textures: &'a EffectTextures,
    calls: Calls,
}

/// The effects in Workspace, where they're drawn, and the instances that
/// move or change them.
#[derive(Debug, Default)]
pub(super) struct EffectCache {
    ids: Vec<InstanceId>,
    slots: Vec<Slot>,
    /// Indices into `ids` of the effects each part or Attachment carries
    /// (or a Beam or Trail runs between), and of each effect itself.
    holders: HashMap<InstanceId, Vec<usize>>,
    calls: HashMap<InstanceId, Calls>,
    textures: Option<EffectTextures>,
}

impl EffectCache {
    pub(super) fn new(textures: EffectTextures) -> Self {
        Self {
            textures: Some(textures),
            ..Default::default()
        }
    }

    /// Whether `id` is an effect drawn here.
    pub(super) fn has(&self, id: InstanceId) -> bool {
        self.ids.contains(&id)
    }

    /// Find every effect in Workspace again, and read them all.
    pub(super) fn find(&mut self, dom: &Dom, drawn: EffectLists<'_>, images: &mut Images<'_>) {
        profiling::scope!("EffectCache::find");
        drawn.emitters.clear();
        drawn.beams.clear();
        drawn.trails.clear();
        self.ids.clear();
        self.slots.clear();
        self.holders.clear();
        let Some(ws) = dom.find_service("Workspace") else {
            return;
        };
        let mut ids = dom.descendants(ws);
        ids.retain(|id| is_effect(dom, *id));
        let ids_set: HashSet<InstanceId> = ids.iter().copied().collect();
        self.calls.retain(|id, _| ids_set.contains(id));
        for (i, &id) in ids.iter().enumerate() {
            for holder in holders(dom, id) {
                self.holders.entry(holder).or_default().push(i);
            }
            let slot = match dom.class_name(id) {
                "Beam" => {
                    drawn.beams.push(self.reader(dom, id, images).beam(id));
                    Slot::Beam(drawn.beams.len() - 1)
                }
                "Trail" => {
                    drawn.trails.push(self.reader(dom, id, images).trail(id));
                    Slot::Trail(drawn.trails.len() - 1)
                }
                _ => {
                    let read = self.reader(dom, id, images).emitters(id);
                    let first = drawn.emitters.len();
                    let count = read.len();
                    drawn.emitters.extend(read);
                    Slot::Emitters { first, count }
                }
            };
            self.slots.push(slot);
        }
        self.ids = ids;
    }

    /// Count scripts' effect calls, before the effects are read (again).
    pub(super) fn note_calls(&mut self, calls: &[(InstanceId, EffectCall)]) {
        for &(id, call) in calls {
            let calls = self.calls.entry(id).or_default();
            match call {
                EffectCall::Emit(n) => calls.emitted += u64::from(n),
                EffectCall::Clear => calls.cleared += 1,
                EffectCall::TextureOffset(offset) => calls.texture_offset = offset,
            }
        }
    }

    /// Bring the effects up to date with a frame's changes (their calls
    /// already noted): those whose properties or calls changed are read
    /// again, and those on parts and Attachments that only moved only
    /// move. Whether any changed.
    pub(super) fn changed(
        &mut self,
        dom: &Dom,
        changes: &RenderChanges,
        drawn: EffectLists<'_>,
        images: &mut Images<'_>,
    ) -> bool {
        let placed = changes
            .placed
            .iter()
            .any(|id| self.holders.contains_key(id));
        if changes.effects_placed || placed {
            self.find(dom, drawn, images);
            return true;
        }
        let holders = &self.holders;
        let stale: HashSet<usize> = changes
            .effects
            .iter()
            .chain(&changes.parts)
            .chain(changes.effect_calls.iter().map(|(id, _)| id))
            .filter_map(|id| holders.get(id))
            .flatten()
            .copied()
            .collect();
        let moved: HashSet<usize> = changes
            .moved
            .iter()
            .filter_map(|id| holders.get(id))
            .flatten()
            .copied()
            .filter(|i| !stale.contains(i))
            .collect();
        let any = !stale.is_empty() || !moved.is_empty();
        self.reread(
            dom,
            stale,
            drawn.emitters,
            drawn.beams,
            drawn.trails,
            images,
        );
        for i in moved {
            self.moved(dom, i, drawn.emitters, drawn.beams, drawn.trails);
        }
        any
    }

    /// Read the effects `ids` again (their images loaded, or scripts
    /// called them). Whether any is drawn here.
    pub(super) fn reread_ids(
        &mut self,
        dom: &Dom,
        ids: &[InstanceId],
        drawn: EffectLists<'_>,
        images: &mut Images<'_>,
    ) -> bool {
        let stale: HashSet<usize> = ids
            .iter()
            .filter_map(|id| self.ids.iter().position(|e| e == id))
            .collect();
        let any = !stale.is_empty();
        self.reread(
            dom,
            stale,
            drawn.emitters,
            drawn.beams,
            drawn.trails,
            images,
        );
        any
    }

    fn reread(
        &self,
        dom: &Dom,
        stale: HashSet<usize>,
        emitters: &mut [Emitter],
        beams: &mut [Beam],
        trails: &mut [Trail],
        images: &mut Images<'_>,
    ) {
        for i in stale {
            let id = self.ids[i];
            let mut reader = self.reader(dom, id, images);
            match self.slots[i] {
                Slot::Emitters { first, count } => {
                    let read = reader.emitters(id);
                    for (slot, e) in emitters[first..first + count].iter_mut().zip(read) {
                        *slot = e;
                    }
                }
                Slot::Beam(b) => beams[b] = reader.beam(id),
                Slot::Trail(t) => trails[t] = reader.trail(id),
            }
        }
    }

    /// Effect `i`'s holder moved: carry it along.
    fn moved(
        &self,
        dom: &Dom,
        i: usize,
        emitters: &mut [Emitter],
        beams: &mut [Beam],
        trails: &mut [Trail],
    ) {
        let id = self.ids[i];
        match self.slots[i] {
            Slot::Emitters { first, count } => {
                let Some(frame) = holder_frame(dom, id) else {
                    return;
                };
                for e in &mut emitters[first..first + count] {
                    e.position = frame.position;
                    e.rotation = frame.rotation;
                }
            }
            Slot::Beam(b) => {
                if let Some(ends) = ribbons::anchors(dom, id) {
                    beams[b].ends = ends;
                }
            }
            Slot::Trail(t) => {
                if let Some([a, z]) = ribbons::anchors(dom, id) {
                    trails[t].ends = [a.position, z.position];
                }
            }
        }
    }

    fn reader<'a, 'b>(
        &'a self,
        dom: &'a Dom,
        id: InstanceId,
        images: &'a mut Images<'b>,
    ) -> Reader<'a, 'b> {
        Reader {
            dom,
            images,
            textures: self.textures.as_ref().expect("made with textures"),
            calls: self.calls.get(&id).copied().unwrap_or_default(),
        }
    }
}

impl Reader<'_, '_> {
    /// Effect `id`'s image (`Texture`), once loaded.
    fn texture(&mut self, id: InstanceId) -> Option<ImageHash> {
        let url = content_url(self.dom, id, &["TextureContent", "Texture"]);
        if url.is_empty() {
            return None;
        }
        (self.images)(id, &url)
    }
}

/// Emitter `layer` of effect `id`: the key its particles follow.
fn key(id: InstanceId, layer: usize) -> u64 {
    id.to_ref().0 << 4 | layer as u64
}

/// The instances whose changes change effect `id`: itself, and the part
/// and Attachment it hangs from, or a Beam's or Trail's Attachments and
/// their parts. An Explosion stays where it went off.
fn holders(dom: &Dom, id: InstanceId) -> Vec<InstanceId> {
    let mut out = vec![id];
    if dom.class_name(id) == "Explosion" {
        return out;
    }
    if matches!(dom.class_name(id), "Beam" | "Trail") {
        for name in ["Attachment0", "Attachment1"] {
            if let Some(a) = dom.ref_property(id, name) {
                out.push(a);
                out.extend(dom.parent(a));
            }
        }
        return out;
    }
    let mut holder = dom.parent(id);
    while let Some(h) = holder {
        out.push(h);
        if dom.class_name(h) != "Attachment" {
            break;
        }
        holder = dom.parent(h);
    }
    out
}

/// An instance's `CFrame` property.
fn cframe(dom: &Dom, id: InstanceId) -> Option<CFrame> {
    match dom.property_by_name(id, "CFrame")? {
        Value::CFrame(cf) => Some(cf),
        _ => None,
    }
}

/// An Attachment's frame in the world, under a part. `None` elsewhere.
fn attachment_frame(dom: &Dom, attachment: InstanceId) -> Option<CFrame> {
    let part = dom.parent(attachment).filter(|&p| is_part(dom, p))?;
    Some(cframe(dom, part)? * cframe(dom, attachment)?)
}

/// Where an effect emits from and which way: its part's CFrame, or its
/// Attachment's in the world. `None` when it's under neither.
fn holder_frame(dom: &Dom, id: InstanceId) -> Option<CFrame> {
    let parent = dom.parent(id)?;
    if is_part(dom, parent) {
        return cframe(dom, parent);
    }
    if dom.class_name(parent) == "Attachment" {
        return attachment_frame(dom, parent);
    }
    None
}

#[cfg(test)]
mod tests;
