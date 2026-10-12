//! NumberRange, NumberSequence, ColorSequence and their keypoints.

use mlua::{
    Lua, MetaMethod, MultiValue, UserData, UserDataFields, UserDataMethods, UserDataRef,
    Value as LuaValue,
};
use rbx_types::{
    ColorSequence, ColorSequenceKeypoint, NumberRange, NumberSequence, NumberSequenceKeypoint,
};
use rook_types::Color3;

use super::{LColor3, fmt_f32};
use crate::args::{borrow_clone, number_of};

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct LNumberRange(pub NumberRange);

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct LNumberSequence(pub NumberSequence);

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct LNumberSequenceKeypoint(pub NumberSequenceKeypoint);

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct LColorSequence(pub ColorSequence);

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct LColorSequenceKeypoint(pub ColorSequenceKeypoint);

fn err(msg: &str) -> mlua::Error {
    mlua::Error::RuntimeError(msg.to_owned())
}

impl UserData for LNumberRange {
    fn add_fields<F: UserDataFields<Self>>(f: &mut F) {
        f.add_meta_field("__type", "NumberRange");
        f.add_field_method_get("Min", |_, t| Ok(t.0.min));
        f.add_field_method_get("Max", |_, t| Ok(t.0.max));
    }

    fn add_methods<M: UserDataMethods<Self>>(m: &mut M) {
        m.add_meta_method(MetaMethod::Eq, |_, t, o: UserDataRef<LNumberRange>| {
            Ok(t.0 == o.0)
        });
        m.add_meta_method(MetaMethod::ToString, |_, t, ()| {
            Ok(format!("{} {} ", fmt_f32(t.0.min), fmt_f32(t.0.max)))
        });
    }
}

impl UserData for LNumberSequenceKeypoint {
    fn add_fields<F: UserDataFields<Self>>(f: &mut F) {
        f.add_meta_field("__type", "NumberSequenceKeypoint");
        f.add_field_method_get("Time", |_, t| Ok(t.0.time));
        f.add_field_method_get("Value", |_, t| Ok(t.0.value));
        f.add_field_method_get("Envelope", |_, t| Ok(t.0.envelope));
    }

    fn add_methods<M: UserDataMethods<Self>>(m: &mut M) {
        m.add_meta_method(MetaMethod::Eq, |_, t, o: UserDataRef<Self>| Ok(t.0 == o.0));
        m.add_meta_method(MetaMethod::ToString, |_, t, ()| {
            Ok(number_keypoint_string(&t.0))
        });
    }
}

/// `tostring` of a NumberSequenceKeypoint, which a NumberSequence's is
/// made of: `"time value envelope "`.
fn number_keypoint_string(k: &rbx_types::NumberSequenceKeypoint) -> String {
    format!(
        "{} {} {} ",
        fmt_f32(k.time),
        fmt_f32(k.value),
        fmt_f32(k.envelope)
    )
}

impl UserData for LNumberSequence {
    fn add_fields<F: UserDataFields<Self>>(f: &mut F) {
        f.add_meta_field("__type", "NumberSequence");
        f.add_field_method_get("Keypoints", |lua, t| {
            lua.create_sequence_from(t.0.keypoints.iter().map(|k| LNumberSequenceKeypoint(*k)))
        });
    }

    fn add_methods<M: UserDataMethods<Self>>(m: &mut M) {
        m.add_meta_method(MetaMethod::Eq, |_, t, o: UserDataRef<Self>| Ok(t.0 == o.0));
        m.add_meta_method(MetaMethod::ToString, |_, t, ()| {
            Ok(t.0
                .keypoints
                .iter()
                .map(number_keypoint_string)
                .collect::<String>())
        });
    }
}

impl UserData for LColorSequenceKeypoint {
    fn add_fields<F: UserDataFields<Self>>(f: &mut F) {
        f.add_meta_field("__type", "ColorSequenceKeypoint");
        f.add_field_method_get("Time", |_, t| Ok(t.0.time));
        f.add_field_method_get("Value", |_, t| Ok(LColor3(Color3::from(t.0.color))));
    }

    fn add_methods<M: UserDataMethods<Self>>(m: &mut M) {
        m.add_meta_method(MetaMethod::Eq, |_, t, o: UserDataRef<Self>| Ok(t.0 == o.0));
    }
}

impl UserData for LColorSequence {
    fn add_fields<F: UserDataFields<Self>>(f: &mut F) {
        f.add_meta_field("__type", "ColorSequence");
        f.add_field_method_get("Keypoints", |lua, t| {
            lua.create_sequence_from(t.0.keypoints.iter().map(|k| LColorSequenceKeypoint(*k)))
        });
    }

    fn add_methods<M: UserDataMethods<Self>>(m: &mut M) {
        m.add_meta_method(MetaMethod::Eq, |_, t, o: UserDataRef<Self>| Ok(t.0 == o.0));
        m.add_meta_method(MetaMethod::ToString, |_, t, ()| {
            Ok(t.0
                .keypoints
                .iter()
                .map(|k| {
                    format!(
                        "{} {} {} {} 0 ",
                        fmt_f32(k.time),
                        fmt_f32(k.color.r),
                        fmt_f32(k.color.g),
                        fmt_f32(k.color.b)
                    )
                })
                .collect::<String>())
        });
    }
}

/// Keypoints must start at time 0, end at time 1 and be in order
/// (Confirmed: creator-docs NumberSequence / ColorSequence).
fn check_times(times: &[f32]) -> mlua::Result<()> {
    if times.len() < 2 {
        return Err(err("NumberSequence: requires at least 2 keypoints"));
    }
    if times[0] != 0.0 || *times.last().expect("len >= 2") != 1.0 {
        return Err(err("NumberSequence: time must start at 0, and end at 1"));
    }
    if times.windows(2).any(|w| w[1] < w[0]) {
        return Err(err("NumberSequence: all keypoints must be ordered by time"));
    }
    Ok(())
}

fn number_sequence_new(_: &Lua, args: MultiValue) -> mlua::Result<LNumberSequence> {
    let args = args.into_vec();
    let kp = |t, v| NumberSequenceKeypoint::new(t, v, 0.0);
    let keypoints = match args.as_slice() {
        [LuaValue::Table(t)] => {
            let mut out = Vec::new();
            for v in t.sequence_values::<LuaValue>() {
                let LuaValue::UserData(u) = v? else {
                    return Err(err(
                        "NumberSequence.new(): table of NumberSequenceKeypoints expected.",
                    ));
                };
                let k = borrow_clone::<LNumberSequenceKeypoint>(&u).ok_or_else(|| {
                    err("NumberSequence.new(): table of NumberSequenceKeypoints expected.")
                })?;
                out.push(k.0);
            }
            let times: Vec<f32> = out.iter().map(|k| k.time).collect();
            check_times(&times)?;
            out
        }
        [a] => {
            let a =
                number_of(a).ok_or_else(|| err("NumberSequence.new(): number expected"))? as f32;
            vec![kp(0.0, a), kp(1.0, a)]
        }
        [a, b] => {
            let a =
                number_of(a).ok_or_else(|| err("NumberSequence.new(): number expected"))? as f32;
            let b =
                number_of(b).ok_or_else(|| err("NumberSequence.new(): number expected"))? as f32;
            vec![kp(0.0, a), kp(1.0, b)]
        }
        _ => return Err(err("NumberSequence.new(): invalid arguments")),
    };
    Ok(LNumberSequence(NumberSequence { keypoints }))
}

fn color_sequence_new(_: &Lua, args: MultiValue) -> mlua::Result<LColorSequence> {
    let args = args.into_vec();
    let color = |v: &LuaValue| match v {
        LuaValue::UserData(u) => borrow_clone::<LColor3>(u).map(|c| c.0),
        _ => None,
    };
    let kp = |t, c: Color3| ColorSequenceKeypoint::new(t, c.into());
    let keypoints = match args.as_slice() {
        [LuaValue::Table(t)] => {
            let mut out = Vec::new();
            for v in t.sequence_values::<LuaValue>() {
                let k = match v? {
                    LuaValue::UserData(u) => borrow_clone::<LColorSequenceKeypoint>(&u),
                    _ => None,
                }
                .ok_or_else(|| {
                    err("ColorSequence.new(): table of ColorSequenceKeypoints expected.")
                })?;
                out.push(k.0);
            }
            let times: Vec<f32> = out.iter().map(|k| k.time).collect();
            check_times(&times)
                .map_err(|_| err("ColorSequence: time must start at 0, and end at 1"))?;
            out
        }
        [a] => {
            let a = color(a).ok_or_else(|| err("ColorSequence.new(): Color3 expected"))?;
            vec![kp(0.0, a), kp(1.0, a)]
        }
        [a, b] => {
            let a = color(a).ok_or_else(|| err("ColorSequence.new(): Color3 expected"))?;
            let b = color(b).ok_or_else(|| err("ColorSequence.new(): Color3 expected"))?;
            vec![kp(0.0, a), kp(1.0, b)]
        }
        _ => return Err(err("ColorSequence.new(): invalid arguments")),
    };
    Ok(LColorSequence(ColorSequence { keypoints }))
}

pub(crate) fn install(lua: &Lua) -> mlua::Result<()> {
    let g = lua.globals();

    let nr = lua.create_table()?;
    nr.set(
        "new",
        lua.create_function(|_, (a, b): (f32, Option<f32>)| {
            let b = b.unwrap_or(a);
            if b < a {
                return Err(err("NumberRange: invalid range"));
            }
            Ok(LNumberRange(NumberRange::new(a, b)))
        })?,
    )?;
    g.set("NumberRange", nr)?;

    let ns = lua.create_table()?;
    ns.set("new", lua.create_function(number_sequence_new)?)?;
    g.set("NumberSequence", ns)?;

    let nsk = lua.create_table()?;
    nsk.set(
        "new",
        lua.create_function(|_, (t, v, e): (f32, f32, Option<f32>)| {
            Ok(LNumberSequenceKeypoint(NumberSequenceKeypoint::new(
                t,
                v,
                e.unwrap_or(0.0),
            )))
        })?,
    )?;
    g.set("NumberSequenceKeypoint", nsk)?;

    let cs = lua.create_table()?;
    cs.set("new", lua.create_function(color_sequence_new)?)?;
    g.set("ColorSequence", cs)?;

    let csk = lua.create_table()?;
    csk.set(
        "new",
        lua.create_function(|_, (t, c): (f32, UserDataRef<LColor3>)| {
            Ok(LColorSequenceKeypoint(ColorSequenceKeypoint::new(
                t,
                c.0.into(),
            )))
        })?,
    )?;
    g.set("ColorSequenceKeypoint", csk)?;
    Ok(())
}
