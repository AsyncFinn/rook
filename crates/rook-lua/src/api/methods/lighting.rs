//! Lighting's clock and sky: the time in minutes, and where it puts the
//! sun and the moon (`rook_types::Clock` holds the rules).
//! docs/behavior/lighting/clock.md.

use mlua::Lua;
use rook_dom::InstanceId;
use rook_types::{Clock, Vector3};

use crate::api::methods::{RawMethod, mutate};
use crate::runtime::ctx::args::Args;
use crate::runtime::ctx::convert::vector3_to_lua;
use crate::runtime::ctx::{ApiResult, Outcome, with_world};

fn get_minutes_after_midnight(lua: &Lua, this: InstanceId, _: &Args) -> ApiResult<Outcome> {
    let minutes = with_world(lua, |w| w.dom.clock(this).minutes());
    Outcome::ret(lua, minutes)
}

fn set_minutes_after_midnight(lua: &Lua, this: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let clock = Clock::from_minutes(a.number(0)?);
    mutate(lua, |w| crate::api::properties::set_clock(w, this, clock))?;
    Outcome::ret(lua, ())
}

fn get_sun_direction(lua: &Lua, this: InstanceId, _: &Args) -> ApiResult<Outcome> {
    let sun = with_world(lua, |w| w.dom.sun_direction(this));
    Outcome::ret(lua, vector3_to_lua(Vector3(sun)))
}

fn get_moon_direction(lua: &Lua, this: InstanceId, _: &Args) -> ApiResult<Outcome> {
    let moon = with_world(lua, |w| {
        rook_types::moon_direction(w.dom.sun_direction(this))
    });
    Outcome::ret(lua, vector3_to_lua(Vector3(moon)))
}

fn get_moon_phase(lua: &Lua, _: InstanceId, _: &Args) -> ApiResult<Outcome> {
    Outcome::ret(lua, rook_types::MOON_PHASE)
}

pub(super) fn native(owner: &str, name: &str) -> Option<RawMethod> {
    Some(match (owner, name) {
        ("Lighting", "GetMinutesAfterMidnight") => get_minutes_after_midnight,
        ("Lighting", "SetMinutesAfterMidnight") => set_minutes_after_midnight,
        ("Lighting", "GetSunDirection") => get_sun_direction,
        ("Lighting", "GetMoonDirection") => get_moon_direction,
        ("Lighting", "GetMoonPhase") => get_moon_phase,
        _ => return None,
    })
}
