//! Sounds kept in step with the DOM: loads started and finished, property
//! changes applied, local sounds played and their events fired.

use super::*;

impl DataModel {
    /// Note sounds that entered or left the DataModel or got a new
    /// `SoundId` this frame, before the journal is trimmed (every sound,
    /// the first time).
    pub(crate) fn check_sounds(&mut self) {
        let w = self.runtime.world();
        let dom = &w.dom;
        let sounds = &mut self.sounds;
        sounds.hearing.get_mut().note(dom);
        let (first, changes) = sounds.watch.look(dom);
        rook_types::count!(JournalReadBridges, changes.len());
        if first {
            let all = dom.descendants(dom.root());
            sounds
                .dirty
                .extend(all.into_iter().filter(|&i| is_sound(dom, i)));
        }
        let ids = SoundProps::get();
        for c in changes {
            match c {
                Change::Property { id, property } => {
                    if [&ids.content, &ids.clock, &ids.playing]
                        .iter()
                        .any(|p| p.contains(property))
                    {
                        rook_types::count!(SoundsChanged, 1);
                    }
                    if ids.content.contains(property) {
                        sounds.dirty.insert(*id);
                    }
                    if ids.clock.contains(property) {
                        let n = sounds.seeks.entry(*id).or_default();
                        *n = n.wrapping_add(1);
                        sounds.hearing.get_mut().touch(*id);
                    }
                    if ids.playing.contains(property) && sounds.tracked.contains_key(id) {
                        sounds.active.insert(*id);
                    }
                }
                Change::Collected(id) => sounds.forget(*id),
                Change::Reparented { subtree, .. } => {
                    let moved = subtree.iter().filter(|&&i| is_sound(dom, i));
                    sounds.dirty.extend(moved);
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
            sounds.hearing.get_mut().touch(id);
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
        self.sounds.files.insert(r, load.clone());
        for id in users {
            apply(&mut w.dom, id, &load, events);
            self.sounds.hearing.get_mut().touch(id);
        }
    }

    /// Start the sounds scripts gave to `SoundService:PlayLocalSound`: at
    /// once if their file has loaded, else once it does (the file is
    /// returned for the loader if nothing asked for it yet). Heard from
    /// where the sound is when called (Inference), even if it isn't in
    /// the DataModel.
    pub(super) fn play_local_sounds(&mut self) -> Vec<ContentRef> {
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
pub(super) fn apply(dom: &mut Dom, id: InstanceId, load: &Load, events: &mut Vec<SoundEvent>) {
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
