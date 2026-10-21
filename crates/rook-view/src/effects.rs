//! Visual effects in Workspace (docs/behavior/effects/classic.md,
//! docs/behavior/effects/particle-emitters.md, docs/behavior/effects/beams-and-trails.md,
//! docs/behavior/effects/explosions.md): Fire, Smoke, Sparkles, ParticleEmitter
//! and Explosion as the renderer's particle emitters, Beam and Trail as
//! its ribbons, and so a FloorWire's moving images
//! (docs/behavior/adornments.md; its wire is an adornment).

mod classic;
mod emitter;
mod explosion;
mod ribbons;
mod textures;

use rook_engine::is_effect_class;
use rook_lua::EffectCall;
use rook_render::{Beam, Emitter, FloorWire, Trail};

pub(super) use textures::EffectTextures;

use super::*;

/// Whether `id` is an effect this module draws.
pub(super) fn is_effect(dom: &Dom, id: InstanceId) -> bool {
    is_effect_class(dom.class_name(id)) || dom.class_name(id) == "FloorWire"
}

/// Where an effect is drawn in the scene's lists. Each keeps its place
/// while it exists, so particles and Trail pieces stay with it.
#[derive(Debug, Clone, Copy)]
enum Slot {
    Emitters { first: usize, count: usize },
    Beam(usize),
    Trail(usize),
    FloorWire(usize),
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
    pub(super) floor_wires: &'a mut Vec<FloorWire>,
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
    #[expect(
        clippy::needless_pass_by_value,
        reason = "EffectLists is a set of &mut lists, reborrowed for the call"
    )]
    pub(super) fn find(&mut self, dom: &Dom, drawn: EffectLists<'_>, images: &mut Images<'_>) {
        profiling::scope!("EffectCache::find");
        drawn.emitters.clear();
        drawn.beams.clear();
        drawn.trails.clear();
        drawn.floor_wires.clear();
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
                "FloorWire" => {
                    drawn
                        .floor_wires
                        .push(self.reader(dom, id, images).floor_wire(id));
                    Slot::FloorWire(drawn.floor_wires.len() - 1)
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
                // A wireframe's (`adornments::wireframe`).
                EffectCall::Line(..) => {}
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
        mut drawn: EffectLists<'_>,
        images: &mut Images<'_>,
    ) -> bool {
        let placed = changes
            .placed
            .iter()
            .any(|id| self.holders.contains_key(id));
        // A FloorWire's change may give it other parts to hang from.
        let rewired = changes
            .effects
            .iter()
            .any(|&id| dom.class_name(id) == "FloorWire");
        if changes.effects_placed || placed || rewired {
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
        self.reread(dom, stale, &mut drawn, images);
        for i in moved {
            self.moved(dom, i, &mut drawn);
        }
        any
    }

    /// Read the effects `ids` again (their images loaded, or scripts
    /// called them). Whether any is drawn here.
    pub(super) fn reread_ids(
        &self,
        dom: &Dom,
        ids: &[InstanceId],
        mut drawn: EffectLists<'_>,
        images: &mut Images<'_>,
    ) -> bool {
        let stale: HashSet<usize> = ids
            .iter()
            .filter_map(|id| self.ids.iter().position(|e| e == id))
            .collect();
        let any = !stale.is_empty();
        self.reread(dom, stale, &mut drawn, images);
        any
    }

    fn reread(
        &self,
        dom: &Dom,
        stale: HashSet<usize>,
        drawn: &mut EffectLists<'_>,
        images: &mut Images<'_>,
    ) {
        for i in stale {
            let id = self.ids[i];
            let mut reader = self.reader(dom, id, images);
            match self.slots[i] {
                Slot::Emitters { first, count } => {
                    let read = reader.emitters(id);
                    let slots = &mut drawn.emitters[first..first + count];
                    for (slot, e) in slots.iter_mut().zip(read) {
                        *slot = e;
                    }
                }
                Slot::Beam(b) => drawn.beams[b] = reader.beam(id),
                Slot::Trail(t) => drawn.trails[t] = reader.trail(id),
                Slot::FloorWire(w) => drawn.floor_wires[w] = reader.floor_wire(id),
            }
        }
    }

    /// Effect `i`'s holder moved: carry it along.
    fn moved(&self, dom: &Dom, i: usize, drawn: &mut EffectLists<'_>) {
        let id = self.ids[i];
        match self.slots[i] {
            Slot::Emitters { first, count } => {
                let Some(frame) = holder_frame(dom, id) else {
                    return;
                };
                for e in &mut drawn.emitters[first..first + count] {
                    e.position = frame.position;
                    e.rotation = frame.rotation;
                }
            }
            Slot::Beam(b) => {
                if let Some(ends) = ribbons::anchors(dom, id) {
                    drawn.beams[b].ends = ends;
                }
            }
            Slot::Trail(t) => {
                if let Some([a, z]) = ribbons::anchors(dom, id) {
                    drawn.trails[t].ends = [a.position, z.position];
                }
            }
            Slot::FloorWire(w) => {
                drawn.floor_wires[w].path =
                    super::adornments::floor_wire::path(dom, id).unwrap_or_default();
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
/// and Attachment it hangs from, a Beam's or Trail's Attachments and
/// their parts, or a FloorWire's parts. An Explosion stays where it went
/// off.
fn holders(dom: &Dom, id: InstanceId) -> Vec<InstanceId> {
    let mut out = vec![id];
    match dom.class_name(id) {
        "Explosion" => return out,
        "FloorWire" => {
            out.extend(
                ["From", "To"]
                    .iter()
                    .filter_map(|p| dom.ref_property(id, p)),
            );
            return out;
        }
        _ => {}
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

/// An Attachment's frame in the world, under a part. `None` elsewhere.
fn attachment_frame(dom: &Dom, attachment: InstanceId) -> Option<CFrame> {
    let part = dom.parent(attachment).filter(|&p| is_part(dom, p))?;
    Some(dom.cframe_property(part, "CFrame")? * dom.cframe_property(attachment, "CFrame")?)
}

/// Where an effect emits from and which way: its part's CFrame, or its
/// Attachment's in the world. `None` when it's under neither.
fn holder_frame(dom: &Dom, id: InstanceId) -> Option<CFrame> {
    let parent = dom.parent(id)?;
    if is_part(dom, parent) {
        return dom.cframe_property(parent, "CFrame");
    }
    if dom.class_name(parent) == "Attachment" {
        return attachment_frame(dom, parent);
    }
    None
}

#[cfg(test)]
mod tests;
