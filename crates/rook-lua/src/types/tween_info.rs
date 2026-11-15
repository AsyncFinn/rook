//! TweenInfo: how a tween runs, made by `TweenInfo.new`.

use crate::types::err;
use mlua::{
    Lua, MetaMethod, MultiValue, UserData, UserDataFields, UserDataMethods, UserDataRef,
    Value as LuaValue,
};

use crate::runtime::ctx::args::number_of;
use crate::types::enums::item_value;

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct LTweenInfo {
    pub(crate) time: f64,
    pub(crate) easing_style: u32,
    pub(crate) easing_direction: u32,
    pub(crate) repeat_count: i64,
    pub(crate) reverses: bool,
    pub(crate) delay_time: f64,
}

impl UserData for LTweenInfo {
    fn add_fields<F: UserDataFields<Self>>(f: &mut F) {
        f.add_meta_field("__type", "TweenInfo");
        f.add_field_method_get("Time", |_, t| Ok(t.time));
        f.add_field_method_get("EasingStyle", |lua, t| {
            crate::types::enums::enum_item(lua, "EasingStyle", t.easing_style)
        });
        f.add_field_method_get("EasingDirection", |lua, t| {
            crate::types::enums::enum_item(lua, "EasingDirection", t.easing_direction)
        });
        f.add_field_method_get("RepeatCount", |_, t| Ok(t.repeat_count));
        f.add_field_method_get("Reverses", |_, t| Ok(t.reverses));
        f.add_field_method_get("DelayTime", |_, t| Ok(t.delay_time));
    }

    fn add_methods<M: UserDataMethods<Self>>(m: &mut M) {
        crate::types::members::add_unknown(m, "TweenInfo", crate::types::members::Wording::Quoted);
        m.add_meta_method(MetaMethod::Eq, |_, t, o: UserDataRef<LTweenInfo>| {
            Ok(*t == *o)
        });
        m.add_meta_method(MetaMethod::ToString, |_, t, ()| {
            Ok(format!(
                "Time:{} DelayTime:{} RepeatCount:{} Reverses:{} EasingDirection:{} EasingStyle:{}",
                t.time,
                t.delay_time,
                t.repeat_count,
                t.reverses,
                t.easing_direction,
                t.easing_style
            ))
        });
    }
}

/// The value of `Enum.<enum_name>.<item>`, an item the database has.
fn default_item(enum_name: &str, item: &str) -> u32 {
    rook_reflection::database()
        .enum_by_name(enum_name)
        .and_then(|e| e.item_value(item))
        .expect("enum item")
}

fn tween_info_new(lua: &Lua, args: MultiValue) -> mlua::Result<LTweenInfo> {
    let a = args.into_vec();
    let get = |i: usize| a.get(i).cloned().unwrap_or(LuaValue::Nil);
    if !get(0).is_nil() && number_of(&get(0)).is_none() {
        return Err(err(
            "TweenInfo.new first argument expects a number for time.",
        ));
    }
    Ok(LTweenInfo {
        time: number_of(&get(0)).unwrap_or(1.0),
        // EasingStyle.Quad and EasingDirection.Out are the documented defaults.
        easing_style: item_value(lua, &get(1), "EasingStyle")
            .unwrap_or_else(|| default_item("EasingStyle", "Quad")),
        easing_direction: item_value(lua, &get(2), "EasingDirection")
            .unwrap_or_else(|| default_item("EasingDirection", "Out")),
        repeat_count: number_of(&get(3)).unwrap_or(0.0) as i64,
        reverses: matches!(get(4), LuaValue::Boolean(true)),
        delay_time: number_of(&get(5)).unwrap_or(0.0),
    })
}

pub(crate) fn install(lua: &Lua) -> mlua::Result<()> {
    let ti = lua.create_table()?;
    ti.set("new", lua.create_function(tween_info_new)?)?;
    lua.globals().set("TweenInfo", ti)
}
