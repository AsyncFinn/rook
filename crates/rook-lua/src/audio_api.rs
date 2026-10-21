//! The audio API's rules that scripts and the engine share: which pins
//! each class has, which Wires are connected, where emitters and
//! listeners are, how audible one is to the other, and the ranges its
//! properties keep to (`docs/behavior/audio/audio-api.md`; all observed in
//! Studio 0.742 unless marked).
//!
//! The engine builds the graph it hears from these; scripts read
//! `Wire.Connected` and call `GetAudibilityFor` through the same
//! functions.

mod curves;
mod ranges;

pub use curves::{read_curve, write_curve};
pub(crate) use ranges::clamp;

use std::collections::{HashMap, HashSet};

use rook_dom::{Dom, InstanceId};
use rook_types::{CFrame, RollOffMode, Value};

use crate::world::World;

/// The pins of a class Rook wires: its inputs (what a Wire's
/// `TargetName` may name) and outputs (`SourceName`), as Studio's
/// `GetInputPins` and `GetOutputPins` list them.
pub fn pins(class: &str) -> Option<(&'static [&'static str], &'static [&'static str])> {
    const IN: &[&str] = &["Input"];
    const OUT: &[&str] = &["Output"];
    const NONE: &[&str] = &[];
    Some(match class {
        "AudioPlayer" | "AudioListener" => (NONE, OUT),
        "AudioEmitter" | "AudioDeviceOutput" | "AudioAnalyzer" => (IN, NONE),
        "AudioCompressor" => (&["Input", "Sidechain"], OUT),
        "AudioChannelSplitter" => (
            IN,
            &[
                "BackLeft",
                "BackRight",
                "Center",
                "Left",
                "Output",
                "Right",
                "Sub",
                "SurroundLeft",
                "SurroundRight",
                "TopBackLeft",
                "TopBackRight",
                "TopLeft",
                "TopRight",
            ],
        ),
        c if is_effect(c) => (IN, OUT),
        _ => return None,
    })
}

/// The effect classes: one input, one output, and `Bypass`.
pub fn is_effect(class: &str) -> bool {
    matches!(
        class,
        "AudioFader"
            | "AudioEqualizer"
            | "AudioReverb"
            | "AudioDistortion"
            | "AudioPitchShifter"
            | "AudioEcho"
            | "AudioChorus"
            | "AudioCompressor"
            | "AudioFilter"
            | "AudioFlanger"
            | "AudioTremolo"
            | "AudioLimiter"
            | "AudioGate"
    )
}

/// Whether instances of `class` take part in the graph (as a node or a
/// Wire).
pub fn in_graph(class: &str) -> bool {
    class == "Wire" || pins(class).is_some()
}

/// Play or stop an AudioPlayer where its `TimePosition` is. Playing one
/// that plays already changes nothing.
pub fn set_playing(w: &mut World, id: InstanceId, play: bool) -> Result<(), rook_dom::DomError> {
    let playing = w.dom.bool_property(id, "IsPlaying") == Some(true);
    if play && !playing {
        w.sounds.entry(id).or_default().loops = 0;
    }
    w.dom
        .set_property_by_name(id, "IsPlaying", Value::Bool(play))
}

/// A connected Wire.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Connection {
    pub wire: InstanceId,
    pub from: InstanceId,
    pub to: InstanceId,
    /// The target pin is a compressor's **Sidechain**.
    pub sidechain: bool,
}

fn in_tree(dom: &Dom, id: InstanceId) -> bool {
    dom.get(id).is_some() && dom.is_ancestor_of(dom.root(), id)
}

/// What `wire` would connect, before loops are looked at: both ends set,
/// the wire and both ends in the DataModel, pins that exist on the ends
/// (names are case-sensitive), and not an instance to itself.
fn ends(dom: &Dom, wire: InstanceId) -> Option<Connection> {
    if !in_tree(dom, wire) {
        return None;
    }
    let from = dom.ref_property(wire, "SourceInstance")?;
    let to = dom.ref_property(wire, "TargetInstance")?;
    if from == to || !in_tree(dom, from) || !in_tree(dom, to) {
        return None;
    }
    let source = dom.string_property(wire, "SourceName").unwrap_or_default();
    let target = dom.string_property(wire, "TargetName").unwrap_or_default();
    let (_, outputs) = pins(dom.class_name(from))?;
    let (inputs, _) = pins(dom.class_name(to))?;
    (outputs.contains(&source.as_str()) && inputs.contains(&target.as_str())).then(|| Connection {
        wire,
        from,
        to,
        sidechain: target == "Sidechain",
    })
}

/// The connected wires among `wires`. A wire that would close a loop isn't
/// connected; of the wires in a loop, the ones made first are kept
/// (observed for two: the second wire to close the loop read false;
/// Inference for more, by instance order).
pub fn connections(dom: &Dom, wires: &[InstanceId]) -> Vec<Connection> {
    let mut sorted = wires.to_vec();
    sorted.sort();
    let mut next: HashMap<InstanceId, Vec<InstanceId>> = HashMap::new();
    let mut out = Vec::new();
    for wire in sorted {
        let Some(c) = ends(dom, wire) else {
            continue;
        };
        if reaches(&next, c.to, c.from) {
            continue;
        }
        next.entry(c.from).or_default().push(c.to);
        out.push(c);
    }
    out
}

fn reaches(next: &HashMap<InstanceId, Vec<InstanceId>>, from: InstanceId, to: InstanceId) -> bool {
    let mut stack = vec![from];
    let mut seen = HashSet::new();
    while let Some(n) = stack.pop() {
        if n == to {
            return true;
        }
        if seen.insert(n) {
            stack.extend(next.get(&n).into_iter().flatten().copied());
        }
    }
    false
}

/// Every Wire in the DataModel.
pub(crate) fn all_wires(dom: &Dom) -> Vec<InstanceId> {
    dom.descendants(dom.root())
        .into_iter()
        .filter(|&i| dom.class_name(i) == "Wire")
        .collect()
}

/// Where an emitter or listener is: its parent's world CFrame, for a
/// BasePart, Attachment, Camera or Model (its pivot). Under anything else
/// it is nowhere: an emitter isn't heard and a listener hears nothing
/// (creator-docs; Confirmed for a Folder, and for a Model through its
/// pivot). Unlike a Sound it needn't be in Workspace (a part in
/// ReplicatedStorage worked). `PositionType` Instance changed nothing in
/// Studio 0.742, so `PositionInstance` is ignored.
pub fn place(w: &World, id: InstanceId) -> Option<CFrame> {
    let dom = &w.dom;
    let parent = dom.parent(id)?;
    let cframe = |i| dom.cframe_property(i, "CFrame");
    if dom.is_a(parent, "Attachment") {
        let part = dom
            .parent(parent)
            .and_then(cframe)
            .unwrap_or(CFrame::IDENTITY);
        return Some(part * cframe(parent)?);
    }
    if dom.is_a(parent, "BasePart") || dom.is_a(parent, "Camera") {
        return cframe(parent);
    }
    if dom.is_a(parent, "Model") {
        return Some(crate::api::pivot_of(w, parent));
    }
    None
}

fn group(dom: &Dom, id: InstanceId) -> String {
    dom.string_property(id, "AudioInteractionGroup")
        .unwrap_or_default()
}

/// Whether `listener` can hear `emitter` at all: the same
/// `AudioInteractionGroup` (an emitter in group "x" was silent to a
/// listener in "").
pub fn interacting(dom: &Dom, emitter: InstanceId, listener: InstanceId) -> bool {
    group(dom, emitter) == group(dom, listener)
}

/// How loud `listener` hears `emitter`, 0 to 1: the emitter's distance
/// attenuation times the listener's curve, times both angle curves
/// (`GetAudibilityFor`, Confirmed for distance; angles by creator-docs).
/// Studio updates the answer at its audio rate, so a read straight after
/// a move gives the old one; Rook's is always current (Approximation).
pub fn audibility(w: &World, emitter: InstanceId, listener: InstanceId) -> f32 {
    let dom = &w.dom;
    if !interacting(dom, emitter, listener) {
        return 0.0;
    }
    let (Some(e), Some(l)) = (place(w, emitter), place(w, listener)) else {
        return 0.0;
    };
    let offset = l.position - e.position;
    let distance = offset.length();
    let mode = match dom.enum_item_property(emitter, "DistanceAttenuationMode") {
        Some("InverseTapered") => Some(RollOffMode::InverseTapered),
        Some("Linear") => Some(RollOffMode::Linear),
        Some("LinearSquared") => Some(RollOffMode::LinearSquare),
        Some("Inverse") => Some(RollOffMode::Inverse),
        _ => None,
    };
    let bounds = dom
        .number_range_property(emitter, "DistanceAttenuationBounds")
        .map_or([4.0, 10_000.0], |r| [r.min, r.max]);
    let by_distance =
        rook_audio::spatial::emitter_attenuation(
            mode,
            bounds,
            &read_curve(dom, emitter, "DistanceAttenuation"),
            distance,
        ) * rook_audio::spatial::curve(&read_curve(dom, listener, "DistanceAttenuation"), distance);
    // The angle between each one's look vector and the other (Inference
    // for the listener's: the docs give only the emitter's).
    let angle = |c: CFrame, to: glam::Vec3| {
        let look = c.rotation * glam::Vec3::NEG_Z;
        if to.length() < 1e-6 {
            0.0
        } else {
            look.angle_between(to).to_degrees()
        }
    };
    let by_angle = rook_audio::spatial::curve(
        &read_curve(dom, emitter, "AngleAttenuation"),
        angle(e, offset),
    ) * rook_audio::spatial::curve(
        &read_curve(dom, listener, "AngleAttenuation"),
        angle(l, -offset),
    );
    by_distance * by_angle
}

/// The left and right gains `listener` hears `emitter` at: its
/// audibility, panned by direction from the listener as a Sound is
/// (Inference: Studio's listener output, split to stereo, gave the
/// same levels as a Sound's at 30 and 60 degrees; nearer than about 8
/// studs Studio also spreads a sound over more speakers, which Rook
/// doesn't, Approximation).
pub fn heard(w: &World, emitter: InstanceId, listener: InstanceId) -> Option<[f32; 2]> {
    let level = audibility(w, emitter, listener);
    if level <= 0.0 {
        return None;
    }
    let (e, l) = (place(w, emitter)?, place(w, listener)?);
    let local = l.rotation.transpose() * (e.position - l.position);
    Some(rook_audio::spatial::pan(local).map(|g| g * level))
}

/// The emitters (or listeners) in the DataModel that `id` can interact
/// with: those of the other kind in its `AudioInteractionGroup`.
pub(crate) fn interacting_with(dom: &Dom, id: InstanceId, class: &str) -> Vec<InstanceId> {
    dom.descendants(dom.root())
        .into_iter()
        .filter(|&i| dom.class_name(i) == class && interacting(dom, i, id))
        .collect()
}
