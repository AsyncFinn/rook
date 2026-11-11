//! What the mixer is given ([`rook_audio::Mix`]), kept from frame to frame
//! as game audio engines keep their voices: a sound, SoundGroup or node of
//! the audio API's graph is read again only when the journal (or the
//! engine's own playback) says it changed, and the mix's version moves
//! with it. What changes every frame is only how loud each 3D sound and
//! each emitter is where it is now, worked out from cached settings.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::LazyLock;

use rook_audio::graph::{Graph, Node, NodeKind, Source, Wire};
use rook_audio::spatial::{attenuation, pan};
use rook_audio::{Group, Mix, Play};
use rook_lua::audio_api::{self, Hearer};
use rook_reflection::ClassId;
use rook_types::CFrame;

use super::effects::{effects, group, parent_group};
use super::graph::effect as graph_effect;
use super::playback::{Listener, PlayingSound, emitter};
use super::*;

/// The classes whose changes the cache follows, resolved once: it looks at
/// every journal entry.
struct Classes {
    sound: ClassId,
    /// Sound's `TimePosition`, read each frame.
    time_position: rook_reflection::PropertyId,
    effect: ClassId,
    group: ClassId,
    player: ClassId,
    players: ClassId,
}

fn classes() -> &'static Classes {
    static CLASSES: LazyLock<Classes> = LazyLock::new(|| {
        let db = rook_reflection::database();
        let id = |name| db.class_by_name(name).expect(name).id;
        let sound = id("Sound");
        let time_position = (db.find_canonical_property(sound, "TimePosition"))
            .expect("TimePosition")
            .id;
        Classes {
            sound,
            time_position,
            effect: id("SoundEffect"),
            group: id("SoundGroup"),
            player: id("AudioPlayer"),
            players: id("Players"),
        }
    });
    &CLASSES
}

/// What a 3D sound's place is read from each frame.
#[derive(Debug, Clone, Copy)]
enum Anchor {
    Part(InstanceId),
    Attachment(InstanceId),
}

/// A listener of the audio API's graph: its node, its settings, and the
/// emitters it can hear (indices into [`Hearing::emitters`]).
#[derive(Debug)]
struct ListenerNode {
    node: usize,
    id: InstanceId,
    hearer: Hearer,
    emitters: Vec<usize>,
}

#[derive(Debug, Default)]
pub(crate) struct Hearing {
    watch: JournalWatch,
    /// Sounds (and AudioPlayers) to read again before the next mix.
    stale: BTreeSet<InstanceId>,
    groups_stale: bool,
    graph_stale: bool,
    /// The Sounds heard, with where each 3D one is heard from.
    sounds: BTreeMap<InstanceId, (PlayingSound, Option<Anchor>)>,
    /// The one-shots in the mix, in the order listed.
    one_shots: Vec<InstanceId>,
    /// The graph's emitters, with their settings, and its listeners.
    emitters: Vec<(InstanceId, Hearer)>,
    listeners: Vec<ListenerNode>,
    mix: Mix<InstanceId>,
}

impl Hearing {
    /// Read `id` again before the next mix: the engine changed what it
    /// plays without the journal (its file, a seek, an end).
    pub(crate) fn touch(&mut self, id: InstanceId) {
        self.stale.insert(id);
    }

    /// Look again at the whole tree from `cursor` (a client DataModel
    /// filled by hand).
    pub(crate) fn rescan(&mut self, cursor: u64) {
        self.watch.rescan(cursor);
    }

    /// The effects a heard sound plays through, as last read.
    pub(crate) fn effects_of(&self, id: InstanceId) -> Option<&[rook_audio::effects::Effect]> {
        self.sounds.get(&id).map(|(s, _)| &s.effects[..])
    }

    pub(crate) fn mix(&self) -> &Mix<InstanceId> {
        &self.mix
    }

    /// The Sounds heard and the one-shots playing out, by id.
    pub(crate) fn playing(&self, one_shots: &[playback::OneShot]) -> Vec<PlayingSound> {
        let mut out: Vec<PlayingSound> = (one_shots.iter().map(|o| o.sound.clone()))
            .chain(self.sounds.values().map(|(s, _)| s.clone()))
            .collect();
        out.sort_by_key(|s| s.id);
        out
    }

    /// Note what the journal says changed since the last look: when the
    /// mix is asked for, and each frame before the journal is trimmed.
    pub(super) fn note(&mut self, dom: &Dom) {
        let (first, changes) = self.watch.look(dom);
        if first {
            let c = classes();
            let all = dom.descendants(dom.root());
            (self.stale).extend(all.into_iter().filter(|&i| dom.is_a_class(i, c.sound)));
            self.groups_stale = true;
            self.graph_stale = true;
        }
        rook_types::count!(JournalReadAudio, changes.len());
        for change in changes {
            match change {
                Change::Property { id, .. } => self.changed(dom, *id),
                Change::Reparented {
                    id,
                    old_parent,
                    new_parent,
                    subtree,
                    ..
                } => {
                    for &i in subtree {
                        self.changed(dom, i);
                    }
                    // An effect moved between sounds or groups changes both.
                    if dom.is_a_class(*id, classes().effect) {
                        for owner in [old_parent, new_parent].into_iter().flatten() {
                            self.owner_changed(dom, *owner);
                        }
                    }
                }
                Change::Collected(id) => _ = self.stale.insert(*id),
                _ => {}
            }
        }
    }

    /// `id` changed or moved: what it is part of is read again.
    fn changed(&mut self, dom: &Dom, id: InstanceId) {
        let c = classes();
        if dom.is_a_class(id, c.sound) {
            self.stale.insert(id);
        } else if dom.is_a_class(id, c.effect) {
            if let Some(owner) = dom.parent(id) {
                self.owner_changed(dom, owner);
            }
        } else if dom.is_a_class(id, c.group) {
            self.groups_stale = true;
        } else if dom.is_a_class(id, c.players) || audio_api::in_graph(dom.class_name(id)) {
            self.graph_stale = true;
        }
    }

    fn owner_changed(&mut self, dom: &Dom, owner: InstanceId) {
        let c = classes();
        if dom.is_a_class(owner, c.sound) {
            self.stale.insert(owner);
        } else if dom.is_a_class(owner, c.group) {
            self.groups_stale = true;
        }
    }

    /// Bring the mix's sounds, groups and graph up to date with what
    /// changed; how loud each is where it is now is [`Hearing::place`].
    /// `graph` lists the audio API's nodes and Wires, when the graph is
    /// built again.
    fn catch_up(&mut self, dom: &Dom, sounds: &Sounds, graph: impl FnOnce() -> Vec<InstanceId>) {
        self.note(dom);
        let mut changed = false;
        let mut regroup = std::mem::take(&mut self.groups_stale);
        for id in std::mem::take(&mut self.stale) {
            if dom.is_a_class(id, classes().player) {
                self.graph_stale = true;
                continue;
            }
            let now = heard(dom, sounds, id);
            let before = match &now {
                Some((s, anchor)) => {
                    insert(&mut self.mix.sounds, play(s));
                    self.sounds.insert(id, (s.clone(), *anchor)).map(|(b, _)| b)
                }
                None => {
                    let gone = self.sounds.remove(&id).map(|(b, _)| b);
                    if gone.is_some() {
                        remove(&mut self.mix.sounds, id);
                    }
                    gone
                }
            };
            if before.is_none() && now.is_none() {
                continue;
            }
            changed = true;
            let group_of = |s: Option<&PlayingSound>| s.and_then(|s| s.group);
            regroup |= group_of(before.as_ref()) != group_of(now.as_ref().map(|(s, _)| s));
        }
        // One-shots are few: their list is compared whole.
        if !self
            .one_shots
            .iter()
            .eq(sounds.one_shots.iter().map(|o| &o.sound.id))
        {
            for &id in &self.one_shots {
                if !self.sounds.contains_key(&id) {
                    remove(&mut self.mix.sounds, id);
                }
            }
            for o in &sounds.one_shots {
                insert(&mut self.mix.sounds, play(&o.sound));
            }
            self.one_shots = sounds.one_shots.iter().map(|o| o.sound.id).collect();
            changed = true;
            regroup = true;
        }
        if regroup {
            let wanted = self.mix.sounds.iter().map(|p| p.group);
            self.mix.groups = groups(dom, wanted);
            changed = true;
        }
        if std::mem::take(&mut self.graph_stale) {
            self.build_graph(dom, sounds, &graph());
            changed = true;
        }
        if changed {
            self.mix.version += 1;
        }
    }

    /// Work out how loud each 3D sound and each emitter is now, from
    /// where the listener, the sounds and the emitters are.
    /// `place` is where an emitter or listener is ([`audio_api::place`]).
    fn place(
        &mut self,
        dom: &Dom,
        sounds: &Sounds,
        listener: &Listener,
        place: impl Fn(InstanceId) -> Option<CFrame>,
    ) {
        if self.mix.ambient != listener.ambient_reverb {
            self.mix.ambient = listener.ambient_reverb;
            self.mix.version += 1;
        }
        let (at, facing) = (listener.cframe.position, listener.cframe.rotation);
        let one_shots = &sounds.one_shots;
        for p in &mut self.mix.sounds {
            let found = match self.sounds.get_mut(&p.key) {
                Some((s, anchor)) => {
                    // Where it is in its file, for a mixer that starts it
                    // now (one opened late, or on another device).
                    let time = dom.read_property(s.id, classes().time_position, Value::as_number);
                    s.position = time.unwrap_or(0.0).max(0.0);
                    if let (Some(e), Some(anchor)) = (&mut s.emitter, anchor)
                        && let Some(position) = position(dom, *anchor)
                    {
                        e.position = position;
                    }
                    Some(&*s)
                }
                None => one_shots.iter().map(|o| &o.sound).find(|s| s.id == p.key),
            };
            let Some(s) = found else {
                continue;
            };
            p.position = s.position;
            // A global sound is heard at its Volume on both sides; a 3D
            // one by distance and direction from the listener.
            p.gains = match s.emitter {
                None => [s.volume; 2],
                Some(e) => {
                    let offset = e.position - at;
                    let fade = attenuation(
                        e.roll_off,
                        offset.length(),
                        e.min,
                        e.max,
                        listener.rolloff_scale,
                    );
                    pan(facing.transpose() * offset).map(|g| g * fade * s.volume)
                }
            };
        }
        if self.listeners.is_empty() {
            return;
        }
        let places: Vec<Option<CFrame>> = (self.emitters.iter()).map(|(e, _)| place(*e)).collect();
        for l in &self.listeners {
            let NodeKind::Listener(heard) = &mut self.mix.graph.nodes[l.node].kind else {
                continue;
            };
            heard.clear();
            let Some(at) = place(l.id) else {
                continue;
            };
            for &i in &l.emitters {
                let (e, hearer) = &self.emitters[i];
                if let Some(gains) = places[i].and_then(|p| l.hearer.heard(at, hearer, p)) {
                    heard.push((*e, gains));
                }
            }
        }
    }

    /// The audio API's graph as it is now: every player, effect, emitter,
    /// listener, output and analyzer in the DataModel, and the connected
    /// wires, with each listener's emitters noted for
    /// [`Hearing::place`]. An AudioDeviceOutput is heard when its `Player`
    /// is nil or this client's player (creator-docs).
    fn build_graph(&mut self, dom: &Dom, sounds: &Sounds, ids: &[InstanceId]) {
        self.emitters.clear();
        self.listeners.clear();
        let topology = self.mix.graph.topology + 1;
        if ids.is_empty() {
            self.mix.graph = Graph {
                topology,
                ..Graph::default()
            };
            return;
        }
        let local = dom
            .find_service("Players")
            .and_then(|p| dom.ref_property(p, "LocalPlayer"));
        let mut wires = Vec::new();
        let mut nodes = Vec::new();
        for &id in ids {
            let kind = match dom.class_name(id) {
                "Wire" => {
                    wires.push(id);
                    continue;
                }
                "AudioPlayer" => NodeKind::Player(
                    sounds
                        .audio(id)
                        .filter(|_| dom.bool_property(id, "IsPlaying").unwrap_or(false))
                        .map(|audio| Source {
                            audio: audio.clone(),
                            position: dom
                                .number_property(id, "TimePosition")
                                .unwrap_or(0.0)
                                .max(0.0),
                            volume: dom.float_property(id, "Volume").unwrap_or(1.0),
                            speed: dom.float_property(id, "PlaybackSpeed").unwrap_or(1.0),
                            looped: dom.bool_property(id, "Looping").unwrap_or(false),
                            generation: sounds.generation(id),
                        }),
                ),
                "AudioEmitter" => {
                    self.emitters.push((id, Hearer::read(dom, id)));
                    NodeKind::Emitter
                }
                "AudioListener" => {
                    self.listeners.push(ListenerNode {
                        node: nodes.len(),
                        id,
                        hearer: Hearer::read(dom, id),
                        emitters: Vec::new(),
                    });
                    NodeKind::Listener(Vec::new())
                }
                "AudioDeviceOutput" => {
                    let player = dom.ref_property(id, "Player");
                    if player.is_none() || player == local {
                        NodeKind::Output
                    } else {
                        NodeKind::Effect(None)
                    }
                }
                "AudioAnalyzer" => NodeKind::Analyzer,
                _ => NodeKind::Effect(graph_effect(dom, id)),
            };
            nodes.push(Node { key: id, kind });
        }
        for l in &mut self.listeners {
            let heard =
                (self.emitters.iter().enumerate()).filter(|(_, (_, e))| l.hearer.interacts(e));
            l.emitters = heard.map(|(i, _)| i).collect();
        }
        let wires = audio_api::connections(dom, &wires)
            .into_iter()
            .map(|c| Wire {
                from: c.from,
                to: c.to,
                sidechain: c.sidechain,
            })
            .collect();
        self.mix.graph = Graph {
            topology,
            nodes,
            wires,
        };
    }
}

/// The Sound `id` as heard: in the DataModel, loaded and `Playing`, with
/// what it plays through and where it's heard from.
fn heard(dom: &Dom, sounds: &Sounds, id: InstanceId) -> Option<(PlayingSound, Option<Anchor>)> {
    let audio = sounds.audio(id)?;
    if !dom.bool_property(id, "Playing").unwrap_or(false) {
        return None;
    }
    let sound = PlayingSound {
        id,
        audio: audio.clone(),
        position: dom
            .number_property(id, "TimePosition")
            .unwrap_or(0.0)
            .max(0.0),
        volume: dom.float_property(id, "Volume").unwrap_or(0.5),
        speed: dom.float_property(id, "PlaybackSpeed").unwrap_or(1.0),
        looped: dom.bool_property(id, "Looped").unwrap_or(false),
        generation: sounds.generation(id),
        emitter: emitter(dom, id),
        effects: effects(dom, id),
        group: group(dom, id),
    };
    let anchor = sound.emitter.and_then(|_| {
        let parent = dom.parent(id)?;
        Some(if dom.is_a(parent, "BasePart") {
            Anchor::Part(parent)
        } else {
            Anchor::Attachment(parent)
        })
    });
    Some((sound, anchor))
}

/// Where an anchor is now (see [`emitter`]).
fn position(dom: &Dom, anchor: Anchor) -> Option<glam::Vec3> {
    match anchor {
        Anchor::Part(part) => Some(dom.cframe_property(part, "CFrame")?.position),
        Anchor::Attachment(a) => {
            let part = (dom.parent(a))
                .and_then(|p| dom.cframe_property(p, "CFrame"))
                .unwrap_or(CFrame::IDENTITY);
            Some((part * dom.cframe_property(a, "CFrame")?).position)
        }
    }
}

/// What the mixer plays of `s`, heard at its Volume until placed.
fn play(s: &PlayingSound) -> Play<InstanceId> {
    Play {
        key: s.id,
        audio: s.audio.clone(),
        position: s.position,
        gains: [s.volume; 2],
        speed: s.speed,
        looped: s.looped,
        generation: s.generation,
        effects: s.effects.clone(),
        group: s.group,
    }
}

/// Put `p` in `list`, kept in key order, over any of its key.
fn insert(list: &mut Vec<Play<InstanceId>>, p: Play<InstanceId>) {
    match list.binary_search_by_key(&p.key, |q| q.key) {
        Ok(i) => list[i] = p,
        Err(i) => list.insert(i, p),
    }
}

fn remove(list: &mut Vec<Play<InstanceId>>, key: InstanceId) {
    if let Ok(i) = list.binary_search_by_key(&key, |q| q.key) {
        list.remove(i);
    }
}

/// The SoundGroups `wanted` play through, and every group those are in.
pub(super) fn groups(
    dom: &Dom,
    wanted: impl IntoIterator<Item = Option<InstanceId>>,
) -> Vec<Group<InstanceId>> {
    let mut out: Vec<Group<InstanceId>> = Vec::new();
    for mut next in wanted {
        while let Some(g) = next {
            if out.iter().any(|o| o.key == g) {
                break;
            }
            let parent = parent_group(dom, g);
            out.push(Group {
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

impl DataModel {
    /// The mix as it is now ([`Hearing`]): what changed is read again, and
    /// every sound and emitter is placed.
    pub fn mix(&self) -> std::cell::Ref<'_, Mix<InstanceId>> {
        self.hear();
        std::cell::Ref::map(self.sounds.hearing.borrow(), Hearing::mix)
    }

    /// Bring [`Hearing`] up to date, and place what it hears.
    pub(crate) fn hear(&self) {
        let listener = self.sound_listener();
        let w = self.runtime.world();
        let mut hearing = self.sounds.hearing.borrow_mut();
        let graph = || w.audio.graph.borrow_mut().all(&w.dom);
        hearing.catch_up(&w.dom, &self.sounds, graph);
        let place = |id| audio_api::place(&w, id);
        hearing.place(&w.dom, &self.sounds, &listener, place);
    }

    /// Bring [`Hearing`]'s sounds up to date, without placing them.
    pub(crate) fn catch_up_hearing(&self) {
        let w = self.runtime.world();
        let graph = || w.audio.graph.borrow_mut().all(&w.dom);
        (self.sounds.hearing.borrow_mut()).catch_up(&w.dom, &self.sounds, graph);
    }
}
