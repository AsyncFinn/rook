//! What a sound is heard through: its own effects, its SoundGroup and the
//! groups that one is in, and `SoundService.AmbientReverb`
//! (`docs/behavior/audio/soundgroups-and-effects.md`).

use rook_audio::effects::{Chain, Effect, Reverb, ordered};

use super::*;

/// The effect a SoundEffect instance is, if Rook plays its class (Chorus,
/// Compressor, Echo, Flange and Tremolo are left out, see the docs).
/// Defaults are Studio's.
fn effect(dom: &Dom, id: InstanceId) -> Option<Effect> {
    Some(match dom.class_name(id) {
        "EqualizerSoundEffect" => Effect::Equalizer {
            low: dom.float_property(id, "LowGain").unwrap_or(-20.0),
            mid: dom.float_property(id, "MidGain").unwrap_or(-10.0),
            high: dom.float_property(id, "HighGain").unwrap_or(0.0),
            range: rook_audio::effects::CROSSOVERS,
        },
        "ReverbSoundEffect" => Effect::Reverb(Reverb {
            decay: dom.float_property(id, "DecayTime").unwrap_or(1.5),
            density: dom.float_property(id, "Density").unwrap_or(1.0),
            diffusion: dom.float_property(id, "Diffusion").unwrap_or(1.0),
            // Within ReverbSoundEffect's -80 to 10 dB (creator-docs),
            // narrower than AudioReverb's.
            dry: dom
                .float_property(id, "DryLevel")
                .unwrap_or(-6.0)
                .clamp(-80.0, 10.0),
            wet: dom
                .float_property(id, "WetLevel")
                .unwrap_or(0.0)
                .clamp(-80.0, 10.0),
            ..Reverb::default()
        }),
        "DistortionSoundEffect" => Effect::Distortion {
            level: dom.float_property(id, "Level").unwrap_or(0.75),
        },
        "PitchShiftSoundEffect" => Effect::PitchShift {
            octave: dom.float_property(id, "Octave").unwrap_or(1.25),
        },
        _ => return None,
    })
}

/// The enabled effects directly under a Sound or SoundGroup, in the order
/// they run (Confirmed: an effect inside a Folder under the sound does
/// nothing; one added while it plays applies at once).
pub(crate) fn effects(dom: &Dom, id: InstanceId) -> Vec<Effect> {
    let children = dom.children(id);
    if children.is_empty() {
        return Vec::new();
    }
    ordered(children.iter().filter_map(|&c| {
        if !dom.is_a(c, "SoundEffect") || dom.bool_property(c, "Enabled") == Some(false) {
            return None;
        }
        let priority = dom.number_property(c, "Priority").unwrap_or(0.0) as i32;
        Some((effect(dom, c)?, priority))
    }))
}

/// The SoundGroup a Sound plays through: its `SoundGroup` property, when
/// that is a SoundGroup still in the tree. Parenting a Sound to a group
/// doesn't put it in the group (creator-docs).
pub(crate) fn group(dom: &Dom, sound: InstanceId) -> Option<InstanceId> {
    let g = dom.ref_property(sound, "SoundGroup")?;
    (dom.get(g).is_some() && dom.is_a(g, "SoundGroup")).then_some(g)
}

/// A group's parent group: groups nest by parenting (creator-docs "mix
/// tree"; that the volumes multiply down the tree is Inference).
fn parent_group(dom: &Dom, group: InstanceId) -> Option<InstanceId> {
    dom.parent(group).filter(|&p| dom.is_a(p, "SoundGroup"))
}

/// Studio's `PlaybackLoudness` with effects: the level after the sound's
/// own effects, before `Volume` and its groups (Confirmed: an equalizer at
/// -80 dB read 0, a full distortion 971, a group at Volume 0 or with
/// effects changed nothing). `chain` keeps the effects' state from frame
/// to frame.
pub(super) fn loudness_through(audio: &Audio, from: f64, to: f64, chain: &mut Chain) -> f64 {
    let rate = f64::from(audio.sample_rate);
    let channels = usize::from(audio.channels.max(1));
    let frames = audio.frames();
    let a = ((from.max(0.0) * rate) as usize).min(frames);
    let b = ((to.max(0.0) * rate) as usize).clamp(a, frames);
    if b == a {
        return 0.0;
    }
    let mut block: Vec<rook_audio::effects::Frame> = (a..b)
        .map(|i| {
            let s =
                |c: usize| f32::from(audio.samples[i * channels + c.min(channels - 1)]) / 32768.0;
            [s(0), s(1)]
        })
        .collect();
    chain.process(&mut block);
    let sum: f64 = block
        .iter()
        .map(|f| f64::from(f[0]).powi(2) + f64::from(f[1]).powi(2))
        .sum();
    (sum / (2 * block.len()) as f64).sqrt() * 1000.0
}

impl DataModel {
    /// The SoundGroups `sounds` play through, and every group those are
    /// in, for the mixer.
    pub fn sound_groups(&self, sounds: &[PlayingSound]) -> Vec<rook_audio::Group<InstanceId>> {
        let w = self.runtime.world();
        let dom = &w.dom;
        let mut out: Vec<rook_audio::Group<InstanceId>> = Vec::new();
        for s in sounds {
            let mut next = s.group;
            while let Some(g) = next {
                if out.iter().any(|o| o.key == g) {
                    break;
                }
                let parent = parent_group(dom, g);
                out.push(rook_audio::Group {
                    key: g,
                    parent,
                    volume: dom.float_property(g, "Volume").unwrap_or(0.5),
                    effects: effects(dom, g),
                });
                next = parent;
            }
        }
        out
    }
}

/// `SoundService.AmbientReverb`'s reverb, added to every sound.
pub(super) fn ambient_reverb(dom: &Dom) -> Option<Reverb> {
    let service = dom.find_service("SoundService")?;
    rook_audio::effects::ambient(dom.enum_property(service, "AmbientReverb")?)
}
