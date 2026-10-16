//! The audio API's methods (`docs/behavior/audio.md`, step 5; observed in
//! Studio 0.742 unless marked). The engine (`rook-engine`'s `sounds`)
//! loads AudioPlayers' files, moves `TimePosition` on, fires `Ended` and
//! `Looped`, and measures AudioAnalyzers; these change state.

use mlua::Table;

use super::*;
use crate::audio;
use crate::world::Scheduled;

/// `Play(atTime)` and `Stop(atTime)`: at once without a time, from where
/// `TimePosition` is (`Stop` keeps it; `Play` on a playing player does
/// nothing). With one, scheduled against `SoundService:GetMixerTime`,
/// returning an id for `Cancel`; servers refuse that.
fn play_or_stop(lua: &Lua, this: InstanceId, a: Args, play: bool) -> ApiResult<Outcome> {
    let Some(at) = a.opt_number(0)? else {
        mutate(lua, |w| audio::set_playing(w, this, play))?;
        // An explicit nil (observed: `log(p:Play())` printed "nil").
        return Outcome::ret(lua, LuaValue::Nil);
    };
    if !with_world(lua, |w| w.is_client) {
        let method = if play { "Play" } else { "Stop" };
        return Err(ScriptError(format!(
            "AudioPlayer:{method}(atTime) cannot be used on the server"
        )));
    }
    let id = with_world(lua, |w| {
        w.audio.next_action += 1;
        let id = w.audio.next_action;
        w.audio.scheduled.push(Scheduled {
            id,
            player: this,
            play,
            at,
        });
        id
    });
    Outcome::ret(lua, id)
}

pub(super) fn audio_play(lua: &Lua, this: InstanceId, a: Args) -> ApiResult<Outcome> {
    play_or_stop(lua, this, a, true)
}

pub(super) fn audio_stop(lua: &Lua, this: InstanceId, a: Args) -> ApiResult<Outcome> {
    play_or_stop(lua, this, a, false)
}

/// Cancels a scheduled `Play` or `Stop`: true if it was still waiting.
pub(super) fn audio_cancel(lua: &Lua, _: InstanceId, a: Args) -> ApiResult<Outcome> {
    let id = a.opt_number(0)?.map(|n| n as i64);
    let cancelled = with_world(lua, |w| {
        let before = w.audio.scheduled.len();
        w.audio.scheduled.retain(|s| Some(s.id) != id);
        w.audio.scheduled.len() != before
    });
    Outcome::ret(lua, cancelled)
}

/// `SoundService:GetMixerTime`: seconds on the clock scheduled `Play` and
/// `Stop` use. Rook's is the DataModel's own clock (Inference: Studio's
/// starts from an arbitrary value).
pub(super) fn get_mixer_time(lua: &Lua, _: InstanceId, _: Args) -> ApiResult<Outcome> {
    Outcome::ret(lua, with_world(lua, |w| w.scheduler.now()))
}

fn class_pins(lua: &Lua, this: InstanceId) -> (&'static [&'static str], &'static [&'static str]) {
    with_world(lua, |w| audio::pins(w.dom.class_name(this))).unwrap_or((&[], &[]))
}

pub(super) fn get_input_pins(lua: &Lua, this: InstanceId, _: Args) -> ApiResult<Outcome> {
    Outcome::ret(
        lua,
        lua.create_sequence_from(class_pins(lua, this).0.iter().copied())?,
    )
}

pub(super) fn get_output_pins(lua: &Lua, this: InstanceId, _: Args) -> ApiResult<Outcome> {
    Outcome::ret(
        lua,
        lua.create_sequence_from(class_pins(lua, this).1.iter().copied())?,
    )
}

/// The connected Wires on one of this instance's pins.
pub(super) fn get_connected_wires(lua: &Lua, this: InstanceId, a: Args) -> ApiResult<Outcome> {
    let pin = a.string(0)?;
    let wires = with_world(lua, |w| {
        let dom = &w.dom;
        let name = |wire, p| match dom.property_by_name(wire, p) {
            Some(Value::String(s)) => s,
            _ => String::new(),
        };
        audio::connections(dom, &audio::all_wires(dom))
            .into_iter()
            .filter(|c| {
                (c.from == this && name(c.wire, "SourceName") == pin)
                    || (c.to == this && name(c.wire, "TargetName") == pin)
            })
            .map(|c| c.wire)
            .collect::<Vec<_>>()
    });
    ret_instances(lua, wires)
}

/// The curve property a `Set...`/`Get...Attenuation` method works on.
fn curve_property(method: &str) -> &'static str {
    if method.contains("Angle") {
        "AngleAttenuation"
    } else {
        "DistanceAttenuation"
    }
}

/// `SetDistanceAttenuation(curve)` and `SetAngleAttenuation(curve)`: a
/// table of distance (or angle) to volume; empty or nil for the default.
/// A key that isn't a number is an error (observed message).
fn set_curve(lua: &Lua, this: InstanceId, a: Args, method: &str) -> ApiResult<Outcome> {
    let class = with_world(lua, |w| w.dom.class_name(this).to_owned());
    let mut points = Vec::new();
    if let LuaValue::Table(t) = a.get(0) {
        for pair in t.clone().pairs::<LuaValue, LuaValue>() {
            let (k, v) = pair?;
            let key = crate::ctx::args::number_of(&k)
                .ok_or_else(|| ScriptError(format!("{class}:{method}: numeric key expected.")))?;
            let value = crate::ctx::args::number_of(&v)
                .ok_or_else(|| ScriptError(format!("{class}:{method}: numeric value expected.")))?;
            points.push((key as f32, value as f32));
        }
    }
    let property = curve_property(method);
    mutate(lua, |w| {
        audio::write_curve(&mut w.dom, this, property, points)
    })?;
    Outcome::ret(lua, ())
}

fn get_curve(lua: &Lua, this: InstanceId, method: &str) -> ApiResult<Outcome> {
    let points = with_world(lua, |w| {
        audio::read_curve(&w.dom, this, curve_property(method))
    });
    let t: Table = lua.create_table()?;
    for (k, v) in points {
        t.raw_set(f64::from(k), f64::from(v))?;
    }
    Outcome::ret(lua, t)
}

pub(super) fn set_distance_attenuation(lua: &Lua, this: InstanceId, a: Args) -> ApiResult<Outcome> {
    set_curve(lua, this, a, "SetDistanceAttenuation")
}

pub(super) fn set_angle_attenuation(lua: &Lua, this: InstanceId, a: Args) -> ApiResult<Outcome> {
    set_curve(lua, this, a, "SetAngleAttenuation")
}

pub(super) fn get_distance_attenuation(lua: &Lua, this: InstanceId, _: Args) -> ApiResult<Outcome> {
    get_curve(lua, this, "GetDistanceAttenuation")
}

pub(super) fn get_angle_attenuation(lua: &Lua, this: InstanceId, _: Args) -> ApiResult<Outcome> {
    get_curve(lua, this, "GetAngleAttenuation")
}

/// `AudioEmitter:GetAudibilityFor(listener)` and
/// `AudioListener:GetAudibilityFor(emitter)`.
pub(super) fn get_audibility_for(lua: &Lua, this: InstanceId, a: Args) -> ApiResult<Outcome> {
    let other = a.instance(0)?;
    let level = with_world(lua, |w| {
        if w.dom.is_a(this, "AudioEmitter") {
            audio::audibility(w, this, other)
        } else {
            audio::audibility(w, other, this)
        }
    });
    Outcome::ret(lua, f64::from(level))
}

pub(super) fn get_interacting_listeners(
    lua: &Lua,
    this: InstanceId,
    _: Args,
) -> ApiResult<Outcome> {
    let ids = with_world(lua, |w| {
        audio::interacting_with(&w.dom, this, "AudioListener")
    });
    ret_instances(lua, ids)
}

pub(super) fn get_interacting_emitters(lua: &Lua, this: InstanceId, _: Args) -> ApiResult<Outcome> {
    let ids = with_world(lua, |w| {
        audio::interacting_with(&w.dom, this, "AudioEmitter")
    });
    ret_instances(lua, ids)
}

/// `AudioAnalyzer:GetSpectrum`: the engine's last measurement on a client;
/// empty on a server (creator-docs) or before anything was measured.
pub(super) fn get_spectrum(lua: &Lua, this: InstanceId, _: Args) -> ApiResult<Outcome> {
    let bins = with_world(lua, |w| w.audio.spectra.get(&this).cloned()).unwrap_or_default();
    Outcome::ret(
        lua,
        lua.create_sequence_from(bins.into_iter().map(f64::from))?,
    )
}
