//! Visual effects in Workspace (docs/behavior/effects/fire-smoke-and-sparkles.md,
//! docs/behavior/effects/particle-emitters.md, docs/behavior/effects/beams-and-trails.md,
//! docs/behavior/effects/explosions.md): Fire, Smoke, Sparkles, ParticleEmitter
//! and Explosion as the renderer's particle emitters, Beam and Trail as
//! its ribbons, and so a FloorWire's moving images
//! (docs/behavior/rendering/adornments.md; its wire is an adornment).

mod cache;
mod classic;
mod emitter;
mod explosion;
mod ribbons;
mod textures;

use rook_engine::is_effect_class;
use rook_lua::EffectCall;
use rook_render::{Beam, EffectSlots, Emitter, FloorWire, Trail};

pub(super) use textures::EffectTextures;

use super::*;

pub(super) fn is_effect(dom: &Dom, id: InstanceId) -> bool {
    is_effect_class(dom.class_name(id)) || dom.class_name(id) == "FloorWire"
}

/// Where an effect is drawn in the scene's lists. Each keeps its slots
/// while it exists, so particles and Trail pieces stay with it.
#[derive(Debug, Clone)]
enum Slot {
    /// One per emitter it's drawn as (Fire draws two).
    Emitters(Vec<usize>),
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

/// The scene's effect lists, which the cache keeps, and where the
/// FloorWires' floors are.
pub(super) struct EffectLists<'a> {
    pub(super) emitters: &'a mut Vec<Option<Emitter>>,
    pub(super) beams: &'a mut Vec<Option<Beam>>,
    pub(super) trails: &'a mut Vec<Option<Trail>>,
    pub(super) floor_wires: &'a mut Vec<Option<FloorWire>>,
    pub(super) floors: &'a Floors,
}

/// An effect's image by URL: its hash once loaded (noting that the effect
/// waits for it while it loads).
pub(super) type Images<'a> = dyn FnMut(InstanceId, &str) -> Option<ImageHash> + 'a;

/// How an effect is read: the document, its images, the textures Fire,
/// Smoke and Sparkles draw, and what scripts did to it.
struct Reader<'a, 'b> {
    dom: &'a Dom,
    floors: &'a Floors,
    images: &'a mut Images<'b>,
    textures: &'a EffectTextures,
    calls: Calls,
}

#[derive(Debug)]
struct Entry {
    slot: Slot,
    holders: Vec<InstanceId>,
}

#[derive(Debug, Default)]
struct Free {
    emitters: Vec<usize>,
    beams: Vec<usize>,
    trails: Vec<usize>,
    floor_wires: Vec<usize>,
}

/// The effects in Workspace, where they're drawn, and the instances that
/// move or change them: found once, then kept up to date effect by effect
/// from the journal.
#[derive(Debug, Default)]
pub(super) struct EffectCache {
    effects: FxHashMap<InstanceId, Entry>,
    /// The effects each instance changes (see [`holders`]).
    held: FxHashMap<InstanceId, Vec<InstanceId>>,
    free: Free,
    /// Slots changed since the renderer took them.
    changed: EffectSlots,
    calls: FxHashMap<InstanceId, Calls>,
    textures: Option<EffectTextures>,
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
    Some(dom.drawn_cframe(part)? * dom.cframe_property(attachment, "CFrame")?)
}

/// Where an effect emits from and which way: its part's CFrame, or its
/// Attachment's in the world. `None` when it's under neither.
fn holder_frame(dom: &Dom, id: InstanceId) -> Option<CFrame> {
    let parent = dom.parent(id)?;
    if is_part(dom, parent) {
        return dom.drawn_cframe(parent);
    }
    if dom.class_name(parent) == "Attachment" {
        return attachment_frame(dom, parent);
    }
    None
}

#[cfg(test)]
mod tests;
