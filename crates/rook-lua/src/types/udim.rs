//! UDim and UDim2.

use mlua::{
    Lua, MetaMethod, MultiValue, UserData, UserDataFields, UserDataMethods, UserDataRef,
    Value as LuaValue,
};
use rook_types::{UDim, UDim2};

use super::fmt_f32;
use crate::ctx::args::{borrow_clone, number_of};

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct LUDim(pub UDim);

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct LUDim2(pub UDim2);

/// Offsets are integers; Roblox truncates fractional offsets (Inference).
fn offset(v: Option<f64>) -> i32 {
    v.unwrap_or(0.0) as i32
}

fn fmt_udim(u: UDim) -> String {
    format!("{}, {}", fmt_f32(u.scale), u.offset)
}

impl UserData for LUDim {
    fn add_fields<F: UserDataFields<Self>>(f: &mut F) {
        f.add_meta_field("__type", "UDim");
        f.add_field_method_get("Scale", |_, t| Ok(t.0.scale));
        f.add_field_method_get("Offset", |_, t| Ok(t.0.offset));
    }

    fn add_methods<M: UserDataMethods<Self>>(m: &mut M) {
        m.add_meta_method(MetaMethod::Add, |_, t, o: UserDataRef<LUDim>| {
            Ok(LUDim(t.0 + o.0))
        });
        m.add_meta_method(MetaMethod::Sub, |_, t, o: UserDataRef<LUDim>| {
            Ok(LUDim(t.0 - o.0))
        });
        m.add_meta_method(MetaMethod::Unm, |_, t, ()| {
            Ok(LUDim(UDim::new(-t.0.scale, -t.0.offset)))
        });
        m.add_meta_method(MetaMethod::Eq, |_, t, o: UserDataRef<LUDim>| Ok(t.0 == o.0));
        m.add_meta_method(MetaMethod::ToString, |_, t, ()| Ok(fmt_udim(t.0)));
    }
}

impl UserData for LUDim2 {
    fn add_fields<F: UserDataFields<Self>>(f: &mut F) {
        f.add_meta_field("__type", "UDim2");
        for name in ["X", "Width"] {
            f.add_field_method_get(name, |_, t| Ok(LUDim(t.0.x)));
        }
        for name in ["Y", "Height"] {
            f.add_field_method_get(name, |_, t| Ok(LUDim(t.0.y)));
        }
    }

    fn add_methods<M: UserDataMethods<Self>>(m: &mut M) {
        m.add_method("Lerp", |_, t, (o, a): (UserDataRef<LUDim2>, f32)| {
            Ok(LUDim2(t.0.lerp(o.0, a)))
        });
        m.add_meta_method(MetaMethod::Add, |_, t, o: UserDataRef<LUDim2>| {
            Ok(LUDim2(t.0 + o.0))
        });
        m.add_meta_method(MetaMethod::Sub, |_, t, o: UserDataRef<LUDim2>| {
            Ok(LUDim2(t.0 - o.0))
        });
        m.add_meta_method(MetaMethod::Unm, |_, t, ()| {
            Ok(LUDim2(UDim2 {
                x: UDim::new(-t.0.x.scale, -t.0.x.offset),
                y: UDim::new(-t.0.y.scale, -t.0.y.offset),
            }))
        });
        m.add_meta_method(
            MetaMethod::Eq,
            |_, t, o: UserDataRef<LUDim2>| Ok(t.0 == o.0),
        );
        m.add_meta_method(MetaMethod::ToString, |_, t, ()| {
            Ok(format!("{{{}}}, {{{}}}", fmt_udim(t.0.x), fmt_udim(t.0.y)))
        });
    }
}

#[expect(
    clippy::unnecessary_wraps,
    reason = "Luau calls it through mlua, which wants a Result"
)]
fn udim2_new(_: &Lua, args: MultiValue) -> mlua::Result<LUDim2> {
    let args = args.into_vec();
    if let [LuaValue::UserData(a), LuaValue::UserData(b)] = args.as_slice()
        && let (Some(x), Some(y)) = (borrow_clone::<LUDim>(a), borrow_clone::<LUDim>(b))
    {
        return Ok(LUDim2(UDim2 { x: x.0, y: y.0 }));
    }
    let n = |i: usize| args.get(i).and_then(number_of);
    Ok(LUDim2(UDim2::new(
        n(0).unwrap_or(0.0) as f32,
        offset(n(1)),
        n(2).unwrap_or(0.0) as f32,
        offset(n(3)),
    )))
}

pub(crate) fn install(lua: &Lua) -> mlua::Result<()> {
    let u = lua.create_table()?;
    u.set(
        "new",
        lua.create_function(|_, (s, o): (Option<f32>, Option<f64>)| {
            Ok(LUDim(UDim::new(s.unwrap_or(0.0), offset(o))))
        })?,
    )?;
    lua.globals().set("UDim", u)?;

    let u2 = lua.create_table()?;
    u2.set("new", lua.create_function(udim2_new)?)?;
    u2.set(
        "fromScale",
        lua.create_function(|_, (x, y): (Option<f32>, Option<f32>)| {
            Ok(LUDim2(UDim2::from_scale(
                x.unwrap_or(0.0),
                y.unwrap_or(0.0),
            )))
        })?,
    )?;
    u2.set(
        "fromOffset",
        lua.create_function(|_, (x, y): (Option<f64>, Option<f64>)| {
            Ok(LUDim2(UDim2::from_offset(offset(x), offset(y))))
        })?,
    )?;
    lua.globals().set("UDim2", u2)?;
    Ok(())
}
