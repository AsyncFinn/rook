//! Sounds and AudioPlayers: loading their files through the content
//! loader (one cache for both), `IsLoaded` (`IsReady`), `TimeLength` and
//! `Loaded`; playback (time, ends, loops) is in [`playback`] and
//! [`players`], and the audio API's graph in [`graph`].
//!
//! Every DataModel does this, server and client alike, as Studio does on
//! both (`docs/behavior/audio/playback.md`). Hearing them is the player's job: it
//! asks for [`DataModel::playing_sounds`] each frame and mixes them.
//!
//! Changes are noticed at the end of a frame ([`DataModel::check_sounds`],
//! before the journal is trimmed) and acted on at the start of the next
//! ([`DataModel::apply_sound_changes`]), so the events they fire run in
//! the frame like other engine events.

mod effects;
mod graph;
mod playback;
mod players;

use playback::PlayingSound;

use std::collections::{BTreeSet, HashSet};

use rook_content::decode::audio::Audio;
use rook_content::{ContentRef, Failure, Fetched, Ticket};

use super::journal_watch::JournalWatch;
use crate::*;

/// What one sound file came to.
#[derive(Debug, Clone)]
enum Load {
    Loading,
    Loaded(Arc<Audio>),
    Failed,
}

/// An event for a sound's scripts: the event, the sound's `SoundId` (none
/// for an AudioPlayer's events, which pass nothing), and `DidLoop`'s
/// count.
pub(crate) type SoundEvent = (InstanceId, &'static str, Option<String>, Option<u32>);

#[derive(Debug, Default)]
pub(crate) struct Sounds {
    pub(crate) watch: JournalWatch,
    /// Sounds whose `SoundId` or place in the tree changed last frame.
    dirty: HashSet<InstanceId>,
    /// Sounds in the DataModel, with the file each refers to (`None` for
    /// an empty or malformed `SoundId`, which never loads).
    tracked: HashMap<InstanceId, Option<ContentRef>>,
    /// The tracked sounds each frame moves on: AudioPlayers, and Sounds
    /// that may be playing (`Playing` set since they were last looked at,
    /// or a file still loading). A place's thousands of idle sounds cost
    /// nothing a frame.
    active: BTreeSet<InstanceId>,
    /// Each file once, however many sounds share it.
    files: HashMap<ContentRef, Load>,
    pub(crate) tickets: HashMap<Ticket, ContentRef>,
    /// Per sound, how often scripts or replication set its `TimePosition`
    /// or started it: the mixer jumps to the engine's position only then
    /// (time passing isn't journaled).
    seeks: HashMap<InstanceId, u32>,
    /// Sounds removed with `PlayOnRemove`, playing out once.
    one_shots: Vec<playback::OneShot>,
    /// On clients, the effects `PlaybackLoudness` is read through, per
    /// sound with effects.
    loudness: HashMap<InstanceId, rook_audio::effects::Chain>,
    /// Sounds given to `PlayLocalSound` whose file is still loading.
    local: Vec<(InstanceId, ContentRef)>,
    /// AudioPlayers already seen, so `AutoPlay` plays each once.
    seen_players: HashSet<InstanceId>,
    /// The audio API's instances and Wires in the DataModel.
    graph_nodes: HashSet<InstanceId>,
    /// Whether this client has made its default AudioListener.
    default_listener: bool,
    /// On clients with AudioAnalyzers: the graph rendered to measure them.
    meter: Option<graph::Metering>,
}

impl Sounds {
    /// Drop what is kept about `id`, which is being freed.
    fn forget(&mut self, id: InstanceId) {
        self.dirty.remove(&id);
        self.tracked.remove(&id);
        self.active.remove(&id);
        self.seeks.remove(&id);
        self.loudness.remove(&id);
        self.local.retain(|(l, _)| *l != id);
        self.seen_players.remove(&id);
        self.graph_nodes.remove(&id);
    }
}

fn sound_url(dom: &Dom, id: InstanceId) -> Option<ContentRef> {
    let value = dom
        .content_property(id, "AudioContent")
        .or_else(|| dom.content_property(id, "SoundId"))?;
    ContentRef::parse(&value)
}

fn is_player(dom: &Dom, id: InstanceId) -> bool {
    dom.class_name(id) == "AudioPlayer"
}

fn is_sound(dom: &Dom, id: InstanceId) -> bool {
    dom.is_a(id, "Sound") || is_player(dom, id)
}

fn sound_id(dom: &Dom, id: InstanceId) -> String {
    dom.content_property(id, "AudioContent").unwrap_or_default()
}

impl Sounds {
    /// What the mixer restarts a sound by: a new value whenever a `Play`
    /// or a set `TimePosition` (by a script, or replicated) moved it.
    fn generation(&self, id: InstanceId) -> u32 {
        self.seeks.get(&id).copied().unwrap_or(0)
    }

    fn audio(&self, id: InstanceId) -> Option<&Arc<Audio>> {
        match self.files.get(self.tracked.get(&id)?.as_ref()?)? {
            Load::Loaded(audio) => Some(audio),
            _ => None,
        }
    }

    /// Whether a sound's file is still loading.
    fn loading(&self, id: InstanceId) -> bool {
        let file = self.tracked.get(&id).and_then(Option::as_ref);
        matches!(file.and_then(|f| self.files.get(f)), Some(Load::Loading))
    }
}

/// The properties whose change makes a sound look at its file again, by
/// id: Sound's `AudioContent` and its legacy view `SoundId`, AudioPlayer's
/// `AudioContent` and `Asset`, and AudioPlayer's `IsPlaying` and
/// `AutoLoad` (a player that doesn't load by itself loads when played).
#[derive(Debug)]
struct SoundProps {
    content: Vec<rook_reflection::PropertyId>,
    /// Sound's `Playing`.
    playing: Vec<rook_reflection::PropertyId>,
    /// Where a sound is in its file, and whether it plays.
    clock: Vec<rook_reflection::PropertyId>,
}

impl SoundProps {
    fn get() -> &'static Self {
        static PROPS: std::sync::LazyLock<SoundProps> = std::sync::LazyLock::new(|| {
            let db = rook_reflection::database();
            let find = |class: &str, names: &[&str]| {
                let c = db.class_by_name(class).expect("audio class").id;
                names
                    .iter()
                    .filter_map(|n| db.find_property(c, n).map(|p| p.id))
                    .collect::<Vec<_>>()
            };
            let mut content = find("Sound", &["AudioContent", "SoundId"]);
            content.extend(find(
                "AudioPlayer",
                &["AudioContent", "Asset", "IsPlaying", "AutoLoad"],
            ));
            let playing = find("Sound", &["Playing"]);
            let mut clock = find("Sound", &["TimePosition", "Playing"]);
            clock.extend(find("AudioPlayer", &["TimePosition", "IsPlaying"]));
            SoundProps {
                content,
                playing,
                clock,
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
        let (first, changes) = sounds.watch.look(dom);
        if first {
            let all = dom.descendants(dom.root());
            sounds
                .dirty
                .extend(all.iter().copied().filter(|&i| is_sound(dom, i)));
            sounds.graph_nodes.extend(
                all.into_iter()
                    .filter(|&i| rook_lua::audio_api::in_graph(dom.class_name(i))),
            );
        }
        let ids = SoundProps::get();
        for c in changes {
            match c {
                Change::Property { id, property } => {
                    if ids.content.contains(property) {
                        sounds.dirty.insert(*id);
                    }
                    if ids.clock.contains(property) {
                        let n = sounds.seeks.entry(*id).or_default();
                        *n = n.wrapping_add(1);
                    }
                    if ids.playing.contains(property) && sounds.tracked.contains_key(id) {
                        sounds.active.insert(*id);
                    }
                }
                Change::Collected(id) => sounds.forget(*id),
                Change::Reparented { subtree, .. } => {
                    let moved = subtree.iter().filter(|&&i| is_sound(dom, i));
                    sounds.dirty.extend(moved);
                    for &i in subtree {
                        if dom.is_ancestor_of(dom.root(), i)
                            && rook_lua::audio_api::in_graph(dom.class_name(i))
                        {
                            sounds.graph_nodes.insert(i);
                        } else {
                            sounds.graph_nodes.remove(&i);
                        }
                    }
                }
                _ => {}
            }
        }
    }

    /// Act on last frame's sound changes: a sound that entered the
    /// DataModel or got a new `SoundId` loads (files not asked for yet are
    /// returned for the loader), and fires `Loaded` at once if its file
    /// already has; one that left is unloaded, and plays out once if it has
    /// `PlayOnRemove` (all observed in Studio, except `PlayOnRemove`,
    /// creator-docs). AudioPlayers load the same way, but one with
    /// `AutoLoad` off only once it plays (observed), and one with
    /// `AutoPlay` plays the first time it enters the DataModel, except on a
    /// client, where players arrive by replication (creator-docs; a
    /// client's own players are taken to arrive that way too, Inference).
    pub(crate) fn apply_sound_changes(&mut self, events: &mut Vec<SoundEvent>) -> Vec<ContentRef> {
        let mut wanted = self.play_local_sounds();
        if self.sounds.dirty.is_empty() {
            return wanted;
        }
        let client = self.role == Role::Client;
        let mut w = self.runtime.world_mut();
        let dom = &mut w.dom;
        let sounds = &mut self.sounds;
        let root = dom.root();
        let mut dirty: Vec<_> = sounds.dirty.drain().collect();
        dirty.sort();
        for id in dirty {
            if !dom.is_ancestor_of(root, id) {
                if let Some(audio) = sounds.audio(id).cloned()
                    && dom.bool_property(id, "PlayOnRemove").unwrap_or(false)
                {
                    sounds
                        .one_shots
                        .push(playback::OneShot::new(dom, id, audio, None));
                }
                sounds.active.remove(&id);
                if sounds.tracked.remove(&id).is_some() {
                    rook_dom::ignore_stale(dom.set_property_by_name(
                        id,
                        "IsLoaded",
                        Value::Bool(false),
                    ));
                }
                continue;
            }
            if is_player(dom, id) {
                if sounds.seen_players.insert(id)
                    && !client
                    && dom.bool_property(id, "AutoPlay").unwrap_or(false)
                {
                    rook_dom::ignore_stale(dom.set_property_by_name(
                        id,
                        "IsPlaying",
                        Value::Bool(true),
                    ));
                }
                let auto_load = dom.bool_property(id, "AutoLoad") != Some(false);
                if !auto_load
                    && !dom.bool_property(id, "IsPlaying").unwrap_or(false)
                    && !sounds.tracked.contains_key(&id)
                {
                    continue;
                }
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
            sounds.active.insert(id);
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
        profiling::scope!("sound_loaded");
        let load = match outcome.map(|f| f.audio) {
            Ok(Some(audio)) => Load::Loaded(audio),
            Ok(None) => Load::Failed,
            Err(failure) => {
                // Studio's Output shows every sound that fails, in red.
                tracing::error!(target: "audio", "Failed to load sound {r}: {}", failure.message);
                Load::Failed
            }
        };
        let mut w = self.runtime.world_mut();
        if let Load::Loaded(audio) = &load {
            let dom = &w.dom;
            let sounds = &mut self.sounds;
            sounds.local.retain(|(id, wants)| {
                if *wants != r {
                    return true;
                }
                let at = playback::emitter(dom, *id);
                sounds
                    .one_shots
                    .push(playback::OneShot::new(dom, *id, audio.clone(), at));
                false
            });
        } else {
            self.sounds.local.retain(|(_, wants)| *wants != r);
        }
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

    /// Start the sounds scripts gave to `SoundService:PlayLocalSound`: at
    /// once if their file has loaded, else once it does (the file is
    /// returned for the loader if nothing asked for it yet). Heard from
    /// where the sound is when called (Inference), even if it isn't in
    /// the DataModel.
    fn play_local_sounds(&mut self) -> Vec<ContentRef> {
        let mut wanted = Vec::new();
        let ids = std::mem::take(&mut self.runtime.world_mut().local_sounds);
        if ids.is_empty() {
            return wanted;
        }
        let w = self.runtime.world();
        let dom = &w.dom;
        let sounds = &mut self.sounds;
        for id in ids {
            let Some(r) = sound_url(dom, id) else {
                continue;
            };
            let load = sounds.files.entry(r.clone()).or_insert_with(|| {
                wanted.push(r.clone());
                Load::Loading
            });
            match load {
                Load::Loaded(audio) => {
                    let audio = audio.clone();
                    let at = playback::emitter(dom, id);
                    sounds
                        .one_shots
                        .push(playback::OneShot::new(dom, id, audio, at));
                }
                Load::Loading => sounds.local.push((id, r)),
                Load::Failed => {}
            }
        }
        wanted
    }

    /// Fire `Changed` for what sounds changed since `start`, then their
    /// events: Studio fires `Changed` for `IsLoaded` and `TimeLength`
    /// before `Loaded`.
    pub(crate) fn fire_sound_events(&self, start: u64, events: Vec<SoundEvent>) {
        if events.is_empty() && self.runtime.world().dom.journal().end() == start {
            return;
        }
        self.runtime.dispatch_since(start);
        for (id, event, sound_id, count) in events {
            match (sound_id, count) {
                (Some(s), Some(n)) => self.runtime.fire_event(id, event, (s, n)),
                (Some(s), None) => self.runtime.fire_event(id, event, s),
                (None, _) => self.runtime.fire_event(id, event, ()),
            };
        }
    }
}

/// Show a sound's load state to scripts: `IsLoaded` (an AudioPlayer's
/// `IsReady`), and `TimeLength` (0 until it loads), with a Sound's `Loaded`
/// once it has. An AudioPlayer has no such event; `IsReady` changes before
/// `TimeLength` (observed).
fn apply(dom: &mut Dom, id: InstanceId, load: &Load, events: &mut Vec<SoundEvent>) {
    let (loaded, length) = match load {
        Load::Loaded(audio) => (true, audio.seconds()),
        Load::Loading | Load::Failed => (false, 0.0),
    };
    let player = is_player(dom, id);
    let ready = if player { "IsReady" } else { "IsLoaded" };
    rook_dom::ignore_stale(dom.set_property_by_name(id, ready, Value::Bool(loaded)));
    rook_dom::ignore_stale(dom.set_property_by_name(id, "TimeLength", Value::Float64(length)));
    if loaded && !player {
        events.push((id, "Loaded", Some(sound_id(dom, id)), None));
    }
}
