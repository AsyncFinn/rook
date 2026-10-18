//! Playing sounds: `TimePosition` moving on, ends and loops,
//! `PlaybackLoudness`, and what the player should hear.

use rook_types::CFrame;

use super::*;

/// A sound the player should be hearing.
#[derive(Debug, Clone)]
pub struct PlayingSound {
    pub id: InstanceId,
    pub audio: Arc<Audio>,
    /// `TimePosition`, seconds.
    pub position: f64,
    /// `Volume`, as a linear gain (Approximation: scripts can't see the
    /// curve, `docs/behavior/audio.md`).
    pub volume: f32,
    /// `PlaybackSpeed`.
    pub speed: f32,
    pub looped: bool,
    /// Changes with every `Play`, so the mixer restarts the sound even
    /// when it was near the start already.
    pub generation: u32,
    /// Where a 3D sound is; `None` for a global one.
    pub emitter: Option<Emitter>,
    /// Its own effects, in the order they run (before `Volume`).
    pub effects: Vec<rook_audio::Effect>,
    /// The SoundGroup it plays through ([`DataModel::sound_groups`]).
    pub group: Option<InstanceId>,
}

/// A 3D sound's place and roll-off: a Sound whose parent is a BasePart (at
/// its position) or an Attachment (at its world position) in Workspace
/// (creator-docs Sound; Confirmed by ear-free measurement, see
/// `docs/behavior/audio.md`). Anywhere else a sound is global.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Emitter {
    pub position: glam::Vec3,
    /// `Enum.RollOffMode` value.
    pub roll_off: u32,
    pub min: f32,
    pub max: f32,
}

/// Where 3D sounds are heard from, and how fast they fade.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Listener {
    pub cframe: CFrame,
    /// `SoundService.RolloffScale`.
    pub rolloff_scale: f32,
    /// `SoundService.AmbientReverb`, added to every sound heard.
    pub ambient_reverb: Option<rook_audio::effects::Reverb>,
}

fn cframe_of(dom: &Dom, id: InstanceId) -> Option<CFrame> {
    dom.property_by_name(id, "CFrame")?.as_cframe()
}

/// A sound's emitter, if it is a 3D sound.
pub(super) fn emitter(dom: &Dom, id: InstanceId) -> Option<Emitter> {
    let parent = dom.parent(id)?;
    let workspace = dom.find_service("Workspace")?;
    if !dom.is_ancestor_of(workspace, parent) {
        return None;
    }
    let position = if dom.is_a(parent, "BasePart") {
        cframe_of(dom, parent)?.position
    } else if dom.is_a(parent, "Attachment") {
        let part = cframe_of(dom, dom.parent(parent)?).unwrap_or(CFrame::IDENTITY);
        (part * cframe_of(dom, parent)?).position
    } else {
        return None;
    };
    Some(Emitter {
        position,
        roll_off: dom
            .property_by_name(id, "RollOffMode")
            .and_then(|v| v.as_enum())
            .unwrap_or(0),
        min: number(dom, id, "RollOffMinDistance", 10.0) as f32,
        max: number(dom, id, "RollOffMaxDistance", 10_000.0) as f32,
    })
}

/// A sound playing out once from the start, with the `Volume`,
/// `PlaybackSpeed`, effects and group it had: one removed with
/// `PlayOnRemove`, or given to `SoundService:PlayLocalSound`.
#[derive(Debug, Clone)]
pub(crate) struct OneShot {
    sound: PlayingSound,
}

impl OneShot {
    /// `emitter` is where it is heard from: `None` for a removed sound,
    /// whose part is gone (Approximation: played as a global sound).
    pub(super) fn new(
        dom: &Dom,
        id: InstanceId,
        audio: Arc<Audio>,
        emitter: Option<Emitter>,
    ) -> Self {
        Self {
            sound: PlayingSound {
                id,
                audio,
                position: 0.0,
                volume: number(dom, id, "Volume", 0.5) as f32,
                speed: number(dom, id, "PlaybackSpeed", 1.0) as f32,
                looped: false,
                generation: u32::MAX,
                emitter,
                effects: effects::effects(dom, id),
                group: effects::group(dom, id),
            },
        }
    }
}

/// The sound's level, as `PlaybackLoudness` reads it: the root mean
/// square of the samples just played, before `Volume`, times 1000
/// (Confirmed for a steady tone: amplitude 0.25 read 176.8, at every
/// `Volume` and `PlaybackSpeed`; the window is Inference).
fn loudness(audio: &Audio, from: f64, to: f64) -> f64 {
    let rate = f64::from(audio.sample_rate);
    let channels = usize::from(audio.channels.max(1));
    let frames = audio.frames();
    let a = ((from.max(0.0) * rate) as usize).min(frames);
    let b = ((to.max(0.0) * rate) as usize).clamp(a, frames);
    if b == a {
        return 0.0;
    }
    let samples = &audio.samples[a * channels..b * channels];
    let sum: f64 = samples
        .iter()
        .map(|&s| (f64::from(s) / 32768.0).powi(2))
        .sum();
    (sum / samples.len() as f64).sqrt() * 1000.0
}

impl DataModel {
    /// Move playing, loaded sounds on by `dt` times `PlaybackSpeed`. A
    /// sound past its end, by time or by a script's seek, stops and
    /// rewinds with `Ended`, without `Changed` for `Playing`; a `Looped`
    /// one starts again with `DidLoop` and the loop count since `Play`. A
    /// negative position plays from the start. A sound still loading, or
    /// that failed, stays where it is. (Observed in Studio 0.743.)
    pub(crate) fn advance_sounds(&mut self, dt: f64) {
        self.sounds.one_shots.retain_mut(|o| {
            let s = &mut o.sound;
            s.position += dt * f64::from(s.speed.max(0.0));
            s.position < s.audio.seconds()
        });
        if self.sounds.tracked.is_empty() {
            return;
        }
        profiling::scope!("advance_sounds");
        let client = self.role == Role::Client;
        let mut events = Vec::new();
        // An AudioPlayer that ends changes `IsPlaying` for scripts to see.
        let start = self.runtime.world().dom.journal().end();
        {
            let mut w = self.runtime.world_mut();
            let w = &mut *w;
            let dom = &mut w.dom;
            let mut ids: Vec<_> = self.sounds.tracked.keys().copied().collect();
            ids.sort();
            for id in ids {
                let Some(audio) = self.sounds.audio(id).cloned() else {
                    continue;
                };
                if is_player(dom, id) {
                    players::advance(dom, &mut w.sounds, id, &audio, dt, &mut events);
                    continue;
                }
                let set = |dom: &mut Dom, name: &str, v: Value| {
                    // Time passing isn't a change scripts hear about (no
                    // Changed each frame), so it skips the journal.
                    if let Some(p) = dom.property_id(id, name) {
                        dom.load_property(id, p, v);
                    }
                };
                if !flag(dom, id, "Playing") {
                    if client {
                        set(dom, "PlaybackLoudness", Value::Float64(0.0));
                    }
                    continue;
                }
                let length = audio.seconds();
                let speed = number(dom, id, "PlaybackSpeed", 1.0).max(0.0);
                let before = number(dom, id, "TimePosition", 0.0).max(0.0);
                let mut t = before + dt * speed;
                if client {
                    let list = effects::effects(dom, id);
                    let level = if list.is_empty() {
                        self.sounds.loudness.remove(&id);
                        loudness(&audio, before, t.min(length))
                    } else {
                        let chain =
                            self.sounds.loudness.entry(id).or_insert_with(|| {
                                rook_audio::effects::Chain::new(audio.sample_rate)
                            });
                        chain.sync(&list);
                        effects::loudness_through(&audio, before, t.min(length), chain)
                    };
                    set(dom, "PlaybackLoudness", Value::Float64(level));
                }
                if t >= length {
                    if flag(dom, id, "Looped") && length > 0.0 {
                        t %= length;
                        let control = w.sounds.entry(id).or_default();
                        control.loops += 1;
                        events.push((id, "DidLoop", Some(sound_id(dom, id)), Some(control.loops)));
                    } else {
                        set(dom, "Playing", Value::Bool(false));
                        t = 0.0;
                        events.push((id, "Ended", Some(sound_id(dom, id)), None));
                    }
                }
                set(dom, "TimePosition", Value::Float64(t));
            }
        }
        self.fire_sound_events(start, events);
    }

    /// The Sounds to hear now: in the DataModel, loaded and `Playing`,
    /// plus removed `PlayOnRemove` sounds playing out. (AudioPlayers are
    /// heard through [`DataModel::audio_graph`].)
    pub fn playing_sounds(&self) -> Vec<PlayingSound> {
        let w = self.runtime.world();
        let dom = &w.dom;
        let mut out: Vec<PlayingSound> = self
            .sounds
            .one_shots
            .iter()
            .map(|o| o.sound.clone())
            .collect();
        for &id in self.sounds.tracked.keys() {
            let Some(audio) = self.sounds.audio(id) else {
                continue;
            };
            if !flag(dom, id, "Playing") {
                continue;
            }
            out.push(PlayingSound {
                id,
                audio: audio.clone(),
                position: number(dom, id, "TimePosition", 0.0).max(0.0),
                volume: number(dom, id, "Volume", 0.5) as f32,
                speed: number(dom, id, "PlaybackSpeed", 1.0) as f32,
                looped: flag(dom, id, "Looped"),
                generation: self.sounds.generation(id),
                emitter: emitter(dom, id),
                effects: effects::effects(dom, id),
                group: effects::group(dom, id),
            });
        }
        out.sort_by_key(|s| s.id);
        out
    }

    /// Where 3D sounds are heard from: the camera unless a script chose
    /// otherwise with `SoundService:SetListener` (creator-docs). An
    /// ObjectPosition listener faces as the camera does (Inference); one
    /// whose part is gone falls back to the camera.
    pub fn sound_listener(&self) -> Listener {
        let w = self.runtime.world();
        let dom = &w.dom;
        let camera = self
            .current_camera()
            .and_then(|c| cframe_of(dom, c))
            .unwrap_or(CFrame::IDENTITY);
        let cframe = match w.listener {
            rook_lua::Listener::Camera => camera,
            rook_lua::Listener::CFrame(c) => c,
            rook_lua::Listener::ObjectPosition(p) => cframe_of(dom, p).map_or(camera, |c| CFrame {
                position: c.position,
                rotation: camera.rotation,
            }),
            rook_lua::Listener::ObjectCFrame(p) => cframe_of(dom, p).unwrap_or(camera),
        };
        let rolloff_scale =
            dom.find_service("SoundService")
                .map_or(1.0, |s| number(dom, s, "RolloffScale", 1.0)) as f32;
        Listener {
            cframe,
            rolloff_scale,
            ambient_reverb: effects::ambient_reverb(dom),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::Vec3;

    /// A sound under a part or an attachment in Workspace is a 3D sound at
    /// its position; under anything else, or outside Workspace, global.
    #[test]
    fn emitters() {
        let mut dom = Dom::new();
        let ws = dom.get_service("Workspace").unwrap();
        let storage = dom.get_service("ReplicatedStorage").unwrap();
        let new = |dom: &mut Dom, class: &str, parent: InstanceId| {
            let id = dom.create_by_name(class).unwrap();
            dom.set_parent(id, Some(parent)).unwrap();
            id
        };
        let part = new(&mut dom, "Part", ws);
        let at = |p: Vec3| Value::CFrame(CFrame::from_position(p));
        dom.set_property_by_name(part, "CFrame", at(Vec3::new(1.0, 2.0, 3.0)))
            .unwrap();
        let on_part = new(&mut dom, "Sound", part);
        let attachment = new(&mut dom, "Attachment", part);
        dom.set_property_by_name(attachment, "CFrame", at(Vec3::new(0.0, 0.0, -5.0)))
            .unwrap();
        let on_attachment = new(&mut dom, "Sound", attachment);
        dom.set_property_by_name(on_attachment, "RollOffMode", Value::Enum(1))
            .unwrap();
        let folder = new(&mut dom, "Folder", part);
        let in_folder = new(&mut dom, "Sound", folder);
        let stored = new(&mut dom, "Part", storage);
        let in_storage = new(&mut dom, "Sound", stored);

        let e = emitter(&dom, on_part).unwrap();
        assert_eq!(e.position, Vec3::new(1.0, 2.0, 3.0));
        assert_eq!((e.roll_off, e.min, e.max), (0, 10.0, 10_000.0));
        let e = emitter(&dom, on_attachment).unwrap();
        assert_eq!((e.position, e.roll_off), (Vec3::new(1.0, 2.0, -2.0), 1));
        assert_eq!(emitter(&dom, in_folder), None);
        assert_eq!(emitter(&dom, in_storage), None);
    }
}
