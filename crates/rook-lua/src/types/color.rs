//! Color3 and BrickColor.

use super::err;
use std::sync::LazyLock;

use mlua::{
    Lua, MetaMethod, MultiValue, UserData, UserDataFields, UserDataMethods, UserDataRef,
    Value as LuaValue,
};
use rbx_types::BrickColor;
use rook_types::Color3;

use super::fmt_f32;
use crate::ctx::args::{borrow_clone, number_of};

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct LColor3(pub Color3);

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct LBrickColor(pub BrickColor);

impl UserData for LColor3 {
    fn add_fields<F: UserDataFields<Self>>(f: &mut F) {
        f.add_meta_field("__type", "Color3");
        for name in ["R", "r"] {
            f.add_field_method_get(name, |_, t| Ok(t.0.r));
        }
        for name in ["G", "g"] {
            f.add_field_method_get(name, |_, t| Ok(t.0.g));
        }
        for name in ["B", "b"] {
            f.add_field_method_get(name, |_, t| Ok(t.0.b));
        }
    }

    fn add_methods<M: UserDataMethods<Self>>(m: &mut M) {
        for name in ["Lerp", "lerp"] {
            m.add_method(name, |_, t, (o, a): (UserDataRef<LColor3>, f32)| {
                Ok(LColor3(t.0.lerp(o.0, a)))
            });
        }
        m.add_method("ToHSV", |_, t, ()| Ok(t.0.to_hsv()));
        m.add_method("ToHex", |_, t, ()| {
            let (r, g, b) = t.0.to_rgb();
            Ok(format!("{r:02x}{g:02x}{b:02x}"))
        });
        m.add_meta_method(MetaMethod::Eq, |_, t, o: UserDataRef<LColor3>| {
            Ok(t.0 == o.0)
        });
        m.add_meta_method(MetaMethod::ToString, |_, t, ()| {
            Ok(format!(
                "{}, {}, {}",
                fmt_f32(t.0.r),
                fmt_f32(t.0.g),
                fmt_f32(t.0.b)
            ))
        });
    }
}

/// Every BrickColor, in number order.
static ALL_BRICK_COLORS: LazyLock<Vec<BrickColor>> =
    LazyLock::new(|| (0..=1100u16).filter_map(BrickColor::from_number).collect());

/// The BrickColor nearest to `c` in RGB (Inference: Roblox documents the
/// closest match without giving the metric).
pub(crate) fn closest_brick_color(c: Color3) -> BrickColor {
    let (r, g, b) = c.to_rgb();
    *ALL_BRICK_COLORS
        .iter()
        .min_by_key(|bc| {
            let x = bc.to_color3uint8();
            let d = |a: u8, b: u8| (i32::from(a) - i32::from(b)).pow(2);
            d(x.r, r) + d(x.g, g) + d(x.b, b)
        })
        .expect("BrickColor list is not empty")
}

/// Medium stone grey, what unknown names and numbers fall back to.
const DEFAULT_BRICK_COLOR: u16 = 194;

fn by_number(n: u16) -> BrickColor {
    BrickColor::from_number(n)
        .unwrap_or_else(|| BrickColor::from_number(DEFAULT_BRICK_COLOR).expect("194 exists"))
}

fn brick_new(_: &Lua, args: MultiValue) -> mlua::Result<LBrickColor> {
    let args = args.into_vec();
    let bc = match args.as_slice() {
        [LuaValue::String(s)] => {
            let s = s.to_string_lossy();
            BrickColor::from_name(&s).unwrap_or_else(|| by_number(DEFAULT_BRICK_COLOR))
        }
        [LuaValue::UserData(u)] => match borrow_clone::<LColor3>(u) {
            Some(c) => closest_brick_color(c.0),
            None => {
                return Err(err("Unable to cast to BrickColor"));
            }
        },
        [n] => by_number(number_of(n).unwrap_or(f64::from(DEFAULT_BRICK_COLOR)) as u16),
        [r, g, b] => closest_brick_color(Color3::new(
            number_of(r).unwrap_or(0.0) as f32,
            number_of(g).unwrap_or(0.0) as f32,
            number_of(b).unwrap_or(0.0) as f32,
        )),
        _ => by_number(DEFAULT_BRICK_COLOR),
    };
    Ok(LBrickColor(bc))
}

impl UserData for LBrickColor {
    fn add_fields<F: UserDataFields<Self>>(f: &mut F) {
        f.add_meta_field("__type", "BrickColor");
        f.add_field_method_get("Name", |_, t| Ok(t.0.to_string()));
        f.add_field_method_get("Number", |_, t| Ok(t.0 as u16));
        f.add_field_method_get("Color", |_, t| Ok(LColor3(Color3::from(t.0))));
        f.add_field_method_get("r", |_, t| Ok(Color3::from(t.0).r));
        f.add_field_method_get("g", |_, t| Ok(Color3::from(t.0).g));
        f.add_field_method_get("b", |_, t| Ok(Color3::from(t.0).b));
    }

    fn add_methods<M: UserDataMethods<Self>>(m: &mut M) {
        m.add_meta_method(MetaMethod::Eq, |_, t, o: UserDataRef<LBrickColor>| {
            Ok(t.0 == o.0)
        });
        m.add_meta_method(MetaMethod::ToString, |_, t, ()| Ok(t.0.to_string()));
    }
}

pub(crate) fn install(lua: &Lua) -> mlua::Result<()> {
    let c3 = lua.create_table()?;
    c3.set(
        "new",
        lua.create_function(|_, (r, g, b): (Option<f32>, Option<f32>, Option<f32>)| {
            Ok(LColor3(Color3::new(
                r.unwrap_or(0.0),
                g.unwrap_or(0.0),
                b.unwrap_or(0.0),
            )))
        })?,
    )?;
    c3.set(
        "fromRGB",
        lua.create_function(|_, (r, g, b): (Option<f64>, Option<f64>, Option<f64>)| {
            let c = |x: Option<f64>| (x.unwrap_or(0.0) / 255.0) as f32;
            Ok(LColor3(Color3::new(c(r), c(g), c(b))))
        })?,
    )?;
    c3.set(
        "fromHSV",
        lua.create_function(|_, (h, s, v): (f32, f32, f32)| {
            Ok(LColor3(Color3::from_hsv(h, s, v)))
        })?,
    )?;
    c3.set(
        "toHSV",
        lua.create_function(|_, c: UserDataRef<LColor3>| Ok(c.0.to_hsv()))?,
    )?;
    c3.set(
        "fromHex",
        lua.create_function(|_, hex: String| {
            let h = hex.trim_start_matches('#');
            let full = match h.len() {
                3 => h.chars().flat_map(|c| [c, c]).collect(),
                6 => h.to_owned(),
                _ => {
                    return Err(err("Unable to convert characters to hex value"));
                }
            };
            let n = u32::from_str_radix(&full, 16)
                .map_err(|_| err("Unable to convert characters to hex value"))?;
            Ok(LColor3(Color3::from_rgb(
                (n >> 16) as u8,
                (n >> 8) as u8,
                n as u8,
            )))
        })?,
    )?;
    lua.globals().set("Color3", c3)?;

    let bc = lua.create_table()?;
    bc.set("new", lua.create_function(brick_new)?)?;
    for (name, number) in [
        ("White", 1),
        ("Gray", 194),
        ("DarkGray", 199),
        ("Black", 26),
        ("Red", 21),
        ("Yellow", 24),
        ("Green", 28),
        ("Blue", 23),
    ] {
        bc.set(
            name,
            lua.create_function(move |_, ()| Ok(LBrickColor(by_number(number))))?,
        )?;
    }
    bc.set(
        "random",
        lua.create_function(|lua, ()| {
            let math: mlua::Table = lua.globals().get("math")?;
            let random: mlua::Function = math.get("random")?;
            let i: usize = random.call(ALL_BRICK_COLORS.len())?;
            Ok(LBrickColor(ALL_BRICK_COLORS[i.saturating_sub(1)]))
        })?,
    )?;
    lua.globals().set("BrickColor", bc)?;
    Ok(())
}
