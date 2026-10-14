//! Playing sounds: `TimePosition` moving on, ends and loops,
//! `PlaybackLoudness`, and what the player should hear.

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
}

/// A sound removed with `PlayOnRemove`, playing out once from the start
/// with the `Volume` and `PlaybackSpeed` it had.
#[derive(Debug, Clone)]
pub(crate) struct OneShot {
    sound: PlayingSound,
}

impl OneShot {
    pub(super) fn new(dom: &Dom, id: InstanceId, audio: Arc<Audio>) -> Self {
        Self {
            sound: PlayingSound {
                id,
                audio,
                position: 0.0,
                volume: number(dom, id, "Volume", 0.5) as f32,
                speed: number(dom, id, "PlaybackSpeed", 1.0) as f32,
                looped: false,
                generation: u32::MAX,
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
        {
            let mut w = self.runtime.world_mut();
            let w = &mut *w;
            let dom = &mut w.dom;
            let mut ids: Vec<_> = self.sounds.tracked.keys().copied().collect();
            ids.sort();
            for id in ids {
                let Some(audio) = self.sounds.audio(id) else {
                    continue;
                };
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
                    let level = loudness(audio, before, t.min(length));
                    set(dom, "PlaybackLoudness", Value::Float64(level));
                }
                if t >= length {
                    if flag(dom, id, "Looped") && length > 0.0 {
                        t %= length;
                        let control = w.sounds.entry(id).or_default();
                        control.loops += 1;
                        events.push((id, "DidLoop", sound_id(dom, id), Some(control.loops)));
                    } else {
                        set(dom, "Playing", Value::Bool(false));
                        t = 0.0;
                        events.push((id, "Ended", sound_id(dom, id), None));
                    }
                }
                set(dom, "TimePosition", Value::Float64(t));
            }
        }
        let start = self.runtime.world().dom.journal().end();
        self.fire_sound_events(start, events);
    }

    /// The sounds to hear now: in the DataModel, loaded and `Playing`,
    /// plus removed `PlayOnRemove` sounds playing out.
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
                generation: w.sounds.get(&id).map_or(0, |c| c.plays),
            });
        }
        out.sort_by_key(|s| s.id);
        out
    }
}
