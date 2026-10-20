//! Sound's methods, by the rules observed in Studio 0.743
//! (`docs/behavior/audio.md`, conformance case `content/sound_api`).
//!
//! The engine (`rook-engine`'s `sounds`) loads the file, moves
//! `TimePosition` on and fires `Loaded`, `Ended` and `DidLoop`; these only
//! change state and fire the events a call fires.

use super::*;
use crate::world::Listener;

/// The `soundId` the Sound events pass: the `SoundId` string.
pub(crate) fn sound_id(w: &World, id: InstanceId) -> String {
    w.dom
        .property_by_name(id, "AudioContent")
        .and_then(|v| v.content_url().map(str::to_owned))
        .unwrap_or_default()
}

fn playing(w: &World, id: InstanceId) -> bool {
    w.dom.property_by_name(id, "Playing") == Some(Value::Bool(true))
}

/// Fire `event` with the sound's id, then apply `change`: Studio fires the
/// method's event before `Changed` for `Playing`.
fn fire_then(
    lua: &Lua,
    this: InstanceId,
    event: &'static str,
    change: impl FnOnce(&mut World) -> Result<(), DomError>,
) -> ApiResult<Outcome> {
    let id = with_world(lua, |w| sound_id(w, this));
    fire(
        lua,
        &SignalKey::event(this, event),
        &[LuaValue::String(lua.create_string(&id)?)],
    );
    mutate(lua, change)?;
    Outcome::ret(lua, ())
}

/// Plays from the last `TimePosition` a script set (0 if none, or since
/// `Stop`), restarting a sound already playing; `Played` fires only when
/// it wasn't playing.
pub(super) fn sound_play(lua: &Lua, this: InstanceId, _: &Args) -> ApiResult<Outcome> {
    let was_playing = with_world(lua, |w| playing(w, this));
    let restart = |w: &mut World| {
        let control = w.sounds.entry(this).or_default();
        control.loops = 0;
        let start = control.start;
        w.dom
            .set_property_by_name(this, "TimePosition", Value::Float64(start))?;
        w.dom
            .set_property_by_name(this, "Playing", Value::Bool(true))
    };
    if was_playing {
        mutate(lua, restart)?;
        return Outcome::ret(lua, ());
    }
    fire_then(lua, this, "Played", restart)
}

/// Stops and rewinds a playing sound; does nothing to one that isn't
/// playing (a paused sound keeps its place).
pub(super) fn sound_stop(lua: &Lua, this: InstanceId, _: &Args) -> ApiResult<Outcome> {
    if !with_world(lua, |w| playing(w, this)) {
        return Outcome::ret(lua, ());
    }
    fire_then(lua, this, "Stopped", |w| {
        w.sounds.entry(this).or_default().start = 0.0;
        w.dom
            .set_property_by_name(this, "Playing", Value::Bool(false))?;
        w.dom
            .set_property_by_name(this, "TimePosition", Value::Float64(0.0))
    })
}

/// Pauses a playing sound where it is.
pub(super) fn sound_pause(lua: &Lua, this: InstanceId, _: &Args) -> ApiResult<Outcome> {
    if !with_world(lua, |w| playing(w, this)) {
        return Outcome::ret(lua, ());
    }
    fire_then(lua, this, "Paused", |w| {
        w.dom
            .set_property_by_name(this, "Playing", Value::Bool(false))
    })
}

/// Plays a sound that isn't playing from where it is.
pub(super) fn sound_resume(lua: &Lua, this: InstanceId, _: &Args) -> ApiResult<Outcome> {
    if with_world(lua, |w| playing(w, this)) {
        return Outcome::ret(lua, ());
    }
    fire_then(lua, this, "Resumed", |w| {
        w.dom
            .set_property_by_name(this, "Playing", Value::Bool(true))
    })
}

/// `SoundService:SetListener`: where 3D sounds are heard from
/// (creator-docs; `Enum.ListenerType` Camera, CFrame, ObjectPosition,
/// ObjectCFrame).
pub(super) fn set_listener(lua: &Lua, _: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let e = database()
        .enum_by_name("ListenerType")
        .expect("ListenerType enum");
    let kind = crate::types::enums::coerce(lua, a.get(0), e.id).map_err(ScriptError)?;
    let listener = match e.item_name(kind) {
        Some("CFrame") => Listener::CFrame(a.userdata::<LCFrame>(1, "CFrame")?.0),
        Some("ObjectPosition") => Listener::ObjectPosition(a.instance(1)?),
        Some("ObjectCFrame") => Listener::ObjectCFrame(a.instance(1)?),
        _ => Listener::Camera,
    };
    with_world(lua, |w| w.listener = listener);
    Outcome::ret(lua, ())
}

/// `SoundService:GetListener`: the type and what goes with it (nil for
/// Camera), as set.
pub(super) fn get_listener(lua: &Lua, _: InstanceId, _: &Args) -> ApiResult<Outcome> {
    let listener = with_world(lua, |w| w.listener);
    let (kind, value) = match listener {
        Listener::Camera => ("Camera", LuaValue::Nil),
        Listener::CFrame(c) => (
            "CFrame",
            LuaValue::UserData(lua.create_userdata(LCFrame(c))?),
        ),
        Listener::ObjectPosition(i) => {
            ("ObjectPosition", LuaValue::UserData(push_instance(lua, i)?))
        }
        Listener::ObjectCFrame(i) => ("ObjectCFrame", LuaValue::UserData(push_instance(lua, i)?)),
    };
    let item = crate::types::enums::named_item(lua, "ListenerType", kind)?;
    Outcome::ret(lua, (item, value))
}

/// `SoundService:PlayLocalSound`: the sound plays once from the start for
/// this DataModel alone, wherever it is (creator-docs), changing nothing
/// scripts see: no `Playing`, `TimePosition` or events (Confirmed). On a
/// server it is an error. The engine plays it (`rook-engine`'s `sounds`).
pub(super) fn play_local_sound(lua: &Lua, _: InstanceId, a: &Args) -> ApiResult<Outcome> {
    if !with_world(lua, |w| w.is_client) {
        return Err(ScriptError(
            "SoundService:PlayLocalSound only works on a client.".into(),
        ));
    }
    let sound = a.opt_instance(0)?;
    let sound = sound
        .filter(|&s| with_world(lua, |w| w.dom.is_a(s, "Sound")))
        .ok_or_else(|| ScriptError("SoundService:PlayLocalSound expects a Sound object.".into()))?;
    with_world(lua, |w| w.local_sounds.push(sound));
    Outcome::ret(lua, ())
}
