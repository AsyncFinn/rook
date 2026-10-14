//! Sounds: loading them through the content loader, `TimeLength` and
//! `IsLoaded`, and `TimePosition` advancing while they play.
//!
//! Every DataModel does this, server and client alike, as Studio does on
//! both (`docs/behavior/audio.md`). Hearing them is the player's job: it
//! asks for [`DataModel::playing_sounds`] each frame and mixes them.

use std::collections::HashSet;

use rook_content::audio::Audio;
use rook_content::{ContentRef, Failure, Fetched, Ticket};

use super::*;

/// What one sound file came to.
#[derive(Debug, Clone)]
enum Load {
    Loading,
    Loaded(Arc<Audio>),
    Failed,
}

#[derive(Debug, Default)]
pub(crate) struct Sounds {
    /// Journal position up to which Sound changes have been looked at.
    cursor: u64,
    /// Whether the DataModel's sounds have been looked for once (a loaded
    /// place's are in the tree before the journal starts).
    scanned: bool,
    /// Sounds in the DataModel, with the file each refers to (`None` for
    /// an empty or malformed `SoundId`, which never loads).
    tracked: HashMap<InstanceId, Option<ContentRef>>,
    /// Each file once, however many sounds share it.
    files: HashMap<ContentRef, Load>,
    pub(crate) tickets: HashMap<Ticket, ContentRef>,
    /// Files to hand the loader at the start of the next frame.
    pub(crate) pending: Vec<ContentRef>,
}

/// A sound the player should be hearing.
#[derive(Debug, Clone)]
pub struct PlayingSound {
    pub id: InstanceId,
    pub audio: Arc<Audio>,
    /// `TimePosition`, seconds.
    pub position: f64,
    /// `Volume`, as a linear gain (Approximation until M5 step 2).
    pub volume: f32,
    /// `PlaybackSpeed`.
    pub speed: f32,
    pub looped: bool,
}

fn sound_url(dom: &Dom, id: InstanceId) -> Option<ContentRef> {
    let value = dom
        .property_by_name(id, "AudioContent")
        .or_else(|| dom.property_by_name(id, "SoundId"))?;
    ContentRef::parse(value.content_url()?)
}

fn number(dom: &Dom, id: InstanceId, name: &str, default: f64) -> f64 {
    dom.property_by_name(id, name)
        .and_then(|v| v.as_number())
        .unwrap_or(default)
}

fn flag(dom: &Dom, id: InstanceId, name: &str) -> bool {
    dom.property_by_name(id, name)
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}

impl Sounds {
    /// Look at every sound again (a client DataModel filled by hand).
    pub(crate) fn rescan(&mut self, cursor: u64) {
        self.cursor = cursor;
        self.scanned = false;
    }

    /// Sounds whose `SoundId` or place in the tree changed since the last
    /// look (every sound, the first time).
    fn changed(&mut self, dom: &Dom) -> HashSet<InstanceId> {
        let mut out = HashSet::new();
        if !self.scanned {
            self.scanned = true;
            out.extend(
                dom.descendants(dom.root())
                    .into_iter()
                    .filter(|&i| dom.is_a(i, "Sound")),
            );
        }
        let ids = SoundProps::get();
        for c in dom.journal().since(self.cursor) {
            match c {
                Change::Property { id, property } if ids.content.contains(property) => {
                    out.insert(*id);
                }
                Change::Reparented { subtree, .. } => {
                    out.extend(subtree.iter().filter(|&&i| dom.is_a(i, "Sound")));
                }
                Change::Destroying(id) => {
                    self.tracked.remove(id);
                }
                _ => {}
            }
        }
        self.cursor = dom.journal().end();
        out.retain(|&i| dom.get(i).is_some());
        out
    }
}

/// The `SoundId` properties, by id: the stored `AudioContent` and its
/// legacy view.
#[derive(Debug)]
struct SoundProps {
    content: Vec<rook_reflection::PropertyId>,
}

impl SoundProps {
    fn get() -> &'static Self {
        static PROPS: std::sync::LazyLock<SoundProps> = std::sync::LazyLock::new(|| {
            let db = rook_reflection::database();
            let sound = db.class_by_name("Sound").expect("Sound class").id;
            SoundProps {
                content: ["AudioContent", "SoundId"]
                    .iter()
                    .filter_map(|n| db.find_property(sound, n).map(|p| p.id))
                    .collect(),
            }
        });
        &PROPS
    }
}

impl DataModel {
    /// Look at sounds that entered the DataModel or got a new `SoundId`
    /// this frame (before the journal is trimmed), queueing new files for
    /// the loader. A sound outside the DataModel doesn't load (observed in
    /// Studio).
    pub(crate) fn check_sounds(&mut self) {
        let mut w = self.runtime.world_mut();
        let dom = &mut w.dom;
        let sounds = &mut self.sounds;
        let root = dom.root();
        for id in sounds.changed(dom) {
            if !dom.is_ancestor_of(root, id) {
                sounds.tracked.remove(&id);
                continue;
            }
            let r = sound_url(dom, id);
            if sounds.tracked.get(&id) == Some(&r) {
                continue;
            }
            let load = match &r {
                None => Load::Failed,
                Some(r) => sounds
                    .files
                    .entry(r.clone())
                    .or_insert_with(|| {
                        sounds.pending.push(r.clone());
                        Load::Loading
                    })
                    .clone(),
            };
            sounds.tracked.insert(id, r);
            apply(dom, id, &load);
        }
    }

    /// A sound file finished loading: every sound using it is loaded, or
    /// stays unloaded for good.
    pub(crate) fn sound_loaded(&mut self, r: ContentRef, outcome: Result<Fetched, Failure>) {
        let load = match outcome.ok().and_then(|f| f.audio) {
            Some(audio) => Load::Loaded(audio),
            None => Load::Failed,
        };
        let mut w = self.runtime.world_mut();
        for (&id, used) in &self.sounds.tracked {
            if used.as_ref() == Some(&r) {
                apply(&mut w.dom, id, &load);
            }
        }
        self.sounds.files.insert(r, load);
    }

    /// Move playing, loaded sounds on by `dt`. One that reaches its end
    /// starts again if `Looped`, and otherwise stops and rewinds (observed
    /// in Studio). A sound still loading, or that failed, stays at 0.
    pub(crate) fn advance_sounds(&mut self, dt: f64) {
        if self.sounds.tracked.is_empty() {
            return;
        }
        profiling::scope!("advance_sounds");
        let mut w = self.runtime.world_mut();
        let dom = &mut w.dom;
        let Some(position) = dom.database().class_by_name("Sound").and_then(|c| {
            dom.database()
                .find_canonical_property(c.id, "TimePosition")
                .map(|p| p.id)
        }) else {
            return;
        };
        for (&id, r) in &self.sounds.tracked {
            let Some(Load::Loaded(audio)) = r.as_ref().and_then(|r| self.sounds.files.get(r))
            else {
                continue;
            };
            if !flag(dom, id, "Playing") {
                continue;
            }
            let length = audio.seconds();
            let speed = number(dom, id, "PlaybackSpeed", 1.0).max(0.0);
            let mut t = number(dom, id, "TimePosition", 0.0) + dt * speed;
            if t >= length {
                if flag(dom, id, "Looped") && length > 0.0 {
                    t %= length;
                } else {
                    let _ = dom.set_property_by_name(id, "Playing", Value::Bool(false));
                    t = 0.0;
                }
            }
            // Time passing isn't a change scripts hear about (no Changed
            // event each frame), so it skips the journal.
            dom.load_property(id, position, Value::Float64(t));
        }
    }

    /// The sounds to hear now: in the DataModel, loaded and `Playing`.
    pub fn playing_sounds(&self) -> Vec<PlayingSound> {
        let w = self.runtime.world();
        let dom = &w.dom;
        let mut out = Vec::new();
        for (&id, r) in &self.sounds.tracked {
            let Some(Load::Loaded(audio)) = r.as_ref().and_then(|r| self.sounds.files.get(r))
            else {
                continue;
            };
            if !flag(dom, id, "Playing") {
                continue;
            }
            out.push(PlayingSound {
                id,
                audio: audio.clone(),
                position: number(dom, id, "TimePosition", 0.0),
                volume: number(dom, id, "Volume", 0.5) as f32,
                speed: number(dom, id, "PlaybackSpeed", 1.0) as f32,
                looped: flag(dom, id, "Looped"),
            });
        }
        out.sort_by_key(|s| s.id);
        out
    }
}

/// Show a sound's load state to scripts: `IsLoaded`, and `TimeLength` (0
/// until it loads).
fn apply(dom: &mut Dom, id: InstanceId, load: &Load) {
    let (loaded, length) = match load {
        Load::Loaded(audio) => (true, audio.seconds()),
        Load::Loading | Load::Failed => (false, 0.0),
    };
    let _ = dom.set_property_by_name(id, "IsLoaded", Value::Bool(loaded));
    let _ = dom.set_property_by_name(id, "TimeLength", Value::Float64(length));
}
