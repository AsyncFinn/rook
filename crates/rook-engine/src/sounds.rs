//! Sounds: loading them through the content loader, `IsLoaded`,
//! `TimeLength` and `Loaded`; playback (time, ends, loops) is in
//! [`playback`].
//!
//! Every DataModel does this, server and client alike, as Studio does on
//! both (`docs/behavior/audio.md`). Hearing them is the player's job: it
//! asks for [`DataModel::playing_sounds`] each frame and mixes them.
//!
//! Changes are noticed at the end of a frame ([`DataModel::check_sounds`],
//! before the journal is trimmed) and acted on at the start of the next
//! ([`DataModel::apply_sound_changes`]), so the events they fire run in
//! the frame like other engine events.

mod playback;

pub use playback::PlayingSound;

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

/// An event for a sound's scripts: the event, the sound's `SoundId`, and
/// `DidLoop`'s count.
pub(crate) type SoundEvent = (InstanceId, &'static str, String, Option<u32>);

#[derive(Debug, Default)]
pub(crate) struct Sounds {
    /// Journal position up to which Sound changes have been looked at.
    cursor: u64,
    /// Whether the DataModel's sounds have been looked for once (a loaded
    /// place's are in the tree before the journal starts).
    scanned: bool,
    /// Sounds whose `SoundId` or place in the tree changed last frame.
    dirty: HashSet<InstanceId>,
    /// Sounds in the DataModel, with the file each refers to (`None` for
    /// an empty or malformed `SoundId`, which never loads).
    tracked: HashMap<InstanceId, Option<ContentRef>>,
    /// Each file once, however many sounds share it.
    files: HashMap<ContentRef, Load>,
    pub(crate) tickets: HashMap<Ticket, ContentRef>,
    /// Sounds removed with `PlayOnRemove`, playing out once.
    one_shots: Vec<playback::OneShot>,
}

fn sound_url(dom: &Dom, id: InstanceId) -> Option<ContentRef> {
    let value = dom
        .property_by_name(id, "AudioContent")
        .or_else(|| dom.property_by_name(id, "SoundId"))?;
    ContentRef::parse(value.content_url()?)
}

fn sound_id(dom: &Dom, id: InstanceId) -> String {
    dom.property_by_name(id, "AudioContent")
        .and_then(|v| v.content_url().map(str::to_owned))
        .unwrap_or_default()
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

    fn audio(&self, id: InstanceId) -> Option<&Arc<Audio>> {
        match self.files.get(self.tracked.get(&id)?.as_ref()?)? {
            Load::Loaded(audio) => Some(audio),
            _ => None,
        }
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
    /// Note sounds that entered or left the DataModel or got a new
    /// `SoundId` this frame, before the journal is trimmed (every sound,
    /// the first time).
    pub(crate) fn check_sounds(&mut self) {
        let w = self.runtime.world();
        let dom = &w.dom;
        let sounds = &mut self.sounds;
        if !sounds.scanned {
            sounds.scanned = true;
            let all = dom.descendants(dom.root());
            sounds
                .dirty
                .extend(all.into_iter().filter(|&i| dom.is_a(i, "Sound")));
        }
        let ids = SoundProps::get();
        for c in dom.journal().since(sounds.cursor) {
            match c {
                Change::Property { id, property } if ids.content.contains(property) => {
                    sounds.dirty.insert(*id);
                }
                Change::Reparented { subtree, .. } => {
                    let moved = subtree.iter().filter(|&&i| dom.is_a(i, "Sound"));
                    sounds.dirty.extend(moved);
                }
                _ => {}
            }
        }
        sounds.cursor = dom.journal().end();
    }

    /// Act on last frame's sound changes: a sound that entered the
    /// DataModel or got a new `SoundId` loads (files not asked for yet are
    /// returned for the loader), and fires `Loaded` at once if its file
    /// already has; one that left is unloaded, and plays out once if it has
    /// `PlayOnRemove` (all observed in Studio, except `PlayOnRemove`,
    /// creator-docs).
    pub(crate) fn apply_sound_changes(&mut self, events: &mut Vec<SoundEvent>) -> Vec<ContentRef> {
        let mut wanted = Vec::new();
        if self.sounds.dirty.is_empty() {
            return wanted;
        }
        let mut w = self.runtime.world_mut();
        let dom = &mut w.dom;
        let sounds = &mut self.sounds;
        let root = dom.root();
        let mut dirty: Vec<_> = sounds.dirty.drain().collect();
        dirty.sort();
        for id in dirty {
            if dom.get(id).is_none() {
                sounds.tracked.remove(&id);
                continue;
            }
            if !dom.is_ancestor_of(root, id) {
                if let Some(audio) = sounds.audio(id).cloned()
                    && flag(dom, id, "PlayOnRemove")
                {
                    sounds
                        .one_shots
                        .push(playback::OneShot::new(dom, id, audio));
                }
                if sounds.tracked.remove(&id).is_some() {
                    let _ = dom.set_property_by_name(id, "IsLoaded", Value::Bool(false));
                }
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
                        wanted.push(r.clone());
                        Load::Loading
                    })
                    .clone(),
            };
            sounds.tracked.insert(id, r);
            apply(dom, id, &load, events);
        }
        wanted
    }

    /// A sound file finished loading: every sound using it is loaded, or
    /// stays unloaded for good.
    pub(crate) fn sound_loaded(
        &mut self,
        r: ContentRef,
        outcome: Result<Fetched, Failure>,
        events: &mut Vec<SoundEvent>,
    ) {
        let load = match outcome.ok().and_then(|f| f.audio) {
            Some(audio) => Load::Loaded(audio),
            None => Load::Failed,
        };
        let mut w = self.runtime.world_mut();
        let mut users: Vec<_> = self
            .sounds
            .tracked
            .iter()
            .filter(|(_, used)| used.as_ref() == Some(&r))
            .map(|(&id, _)| id)
            .collect();
        users.sort();
        for id in users {
            apply(&mut w.dom, id, &load, events);
        }
        self.sounds.files.insert(r, load);
    }

    /// Fire `Changed` for what sounds changed since `start`, then their
    /// events: Studio fires `Changed` for `IsLoaded` and `TimeLength`
    /// before `Loaded`.
    pub(crate) fn fire_sound_events(&mut self, start: u64, events: Vec<SoundEvent>) {
        if events.is_empty() && self.runtime.world().dom.journal().end() == start {
            return;
        }
        self.runtime.dispatch_since(start);
        for (id, event, sound_id, count) in events {
            let _ = match count {
                Some(n) => self.runtime.fire_event(id, event, (sound_id, n)),
                None => self.runtime.fire_event(id, event, sound_id),
            };
        }
    }
}

/// Show a sound's load state to scripts: `IsLoaded`, and `TimeLength` (0
/// until it loads), with `Loaded` once it has.
fn apply(dom: &mut Dom, id: InstanceId, load: &Load, events: &mut Vec<SoundEvent>) {
    let (loaded, length) = match load {
        Load::Loaded(audio) => (true, audio.seconds()),
        Load::Loading | Load::Failed => (false, 0.0),
    };
    let _ = dom.set_property_by_name(id, "IsLoaded", Value::Bool(loaded));
    let _ = dom.set_property_by_name(id, "TimeLength", Value::Float64(length));
    if loaded {
        events.push((id, "Loaded", sound_id(dom, id), None));
    }
}
