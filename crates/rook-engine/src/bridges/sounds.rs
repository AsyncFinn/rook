//! Every DataModel loads and plays sounds, server and client alike, as Studio
//! does on both (`docs/behavior/audio/playback.md`). Hearing them is the
//! player's job: it asks for [`DataModel::mix`] each frame and mixes it.
//!
//! Changes are noticed at the end of a frame ([`DataModel::check_sounds`],
//! before the journal is trimmed) and acted on at the start of the next
//! ([`DataModel::apply_sound_changes`]), so the events they fire run in
//! the frame like other engine events.

mod changes;
mod effects;
mod graph;
mod hearing;
mod playback;
mod players;

use playback::PlayingSound;
use rustc_hash::FxHashMap;

use rustc_hash::FxHashSet;
use std::collections::BTreeSet;

use rook_content::decode::audio::{Audio, Reader};
use rook_content::{ContentRef, Failure, Fetched, Ticket};

use super::journal_watch::JournalWatch;
use crate::*;

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
    dirty: FxHashSet<InstanceId>,
    /// Sounds in the DataModel, with the file each refers to (`None` for
    /// an empty or malformed `SoundId`, which never loads).
    tracked: FxHashMap<InstanceId, Option<ContentRef>>,
    /// The tracked sounds each frame moves on: AudioPlayers, and Sounds
    /// that may be playing (`Playing` set since they were last looked at,
    /// or a file still loading). A place's thousands of idle sounds cost
    /// nothing a frame.
    active: BTreeSet<InstanceId>,
    /// Each file once, however many sounds share it.
    files: FxHashMap<ContentRef, Load>,
    pub(crate) tickets: FxHashMap<Ticket, ContentRef>,
    /// Per sound, how often scripts or replication set its `TimePosition`
    /// or started it: the mixer jumps to the engine's position only then
    /// (time passing isn't journaled).
    seeks: FxHashMap<InstanceId, u32>,
    /// Sounds removed with `PlayOnRemove`, playing out once.
    one_shots: Vec<playback::OneShot>,
    /// On clients, the effects `PlaybackLoudness` is read through, per
    /// sound with effects.
    loudness: FxHashMap<InstanceId, rook_audio::effects::Chain>,
    /// On clients, what `PlaybackLoudness` reads each playing sound's
    /// samples through.
    levels: FxHashMap<InstanceId, Reader>,
    /// Sounds given to `PlayLocalSound` whose file is still loading.
    local: Vec<(InstanceId, ContentRef)>,
    /// AudioPlayers already seen, so `AutoPlay` plays each once.
    seen_players: FxHashSet<InstanceId>,
    /// Whether this client has made its default AudioListener.
    default_listener: bool,
    /// On clients with AudioAnalyzers: the graph rendered to measure them.
    meter: Option<graph::Metering>,
    /// What the mixer is given, kept from frame to frame.
    pub(crate) hearing: std::cell::RefCell<hearing::Hearing>,
}

impl Sounds {
    fn forget(&mut self, id: InstanceId) {
        self.dirty.remove(&id);
        self.tracked.remove(&id);
        self.active.remove(&id);
        self.seeks.remove(&id);
        self.loudness.remove(&id);
        self.levels.remove(&id);
        self.local.retain(|(l, _)| *l != id);
        self.seen_players.remove(&id);
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
