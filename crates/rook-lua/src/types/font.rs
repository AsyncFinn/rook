//! Font: a family file, a weight and a style, as `FontFace` holds them.
//! Members follow creator-docs' Font page; `tostring` as Studio 0.743
//! prints it.

use mlua::{
    Lua, MetaMethod, UserData, UserDataFields, UserDataMethods, UserDataRef, Value as LuaValue,
};
use rook_types::{FAMILY_FOLDER, Font, FontStyle, FontWeight, font_from_enum};

use super::enums::{enum_item, item_name, item_value};
use super::err;

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct LFont(pub Font);

fn style_value(style: FontStyle) -> u32 {
    u32::from(style == FontStyle::Italic)
}

fn weight_name(weight: FontWeight) -> &'static str {
    rook_reflection::database()
        .enum_by_name("FontWeight")
        .and_then(|e| e.item_name(u32::from(weight.as_u16())))
        .unwrap_or("Regular")
}

impl UserData for LFont {
    fn add_fields<F: UserDataFields<Self>>(f: &mut F) {
        f.add_meta_field("__type", "Font");
        f.add_field_method_get("Family", |_, t| Ok(t.0.family.clone()));
        f.add_field_method_get("Weight", |lua, t| {
            enum_item(lua, "FontWeight", u32::from(t.0.weight.as_u16()))
        });
        f.add_field_method_get("Style", |lua, t| {
            enum_item(lua, "FontStyle", style_value(t.0.style))
        });
        // Inference: SemiBold and heavier count as bold.
        f.add_field_method_get("Bold", |_, t| Ok(t.0.weight.as_u16() >= 600));
    }

    fn add_methods<M: UserDataMethods<Self>>(m: &mut M) {
        m.add_meta_method(MetaMethod::Eq, |_, t, o: UserDataRef<LFont>| {
            let key = |f: &Font| (f.family.clone(), f.weight, f.style);
            Ok(key(&t.0) == key(&o.0))
        });
        m.add_meta_method(MetaMethod::ToString, |_, t, ()| {
            let style = if t.0.style == FontStyle::Italic {
                "Italic"
            } else {
                "Normal"
            };
            Ok(format!(
                "Font {{ Family = {}, Weight = {}, Style = {style} }}",
                t.0.family,
                weight_name(t.0.weight),
            ))
        });
    }
}

/// The weight and style arguments constructors take, both optional.
fn weight_and_style(lua: &Lua, weight: &LuaValue, style: &LuaValue) -> (FontWeight, FontStyle) {
    let weight = item_value(lua, weight, "FontWeight")
        .and_then(|w| FontWeight::from_u16(w as u16))
        .unwrap_or_default();
    let style = match item_value(lua, style, "FontStyle") {
        Some(1) => FontStyle::Italic,
        _ => FontStyle::Normal,
    };
    (weight, style)
}

pub(crate) fn install(lua: &Lua) -> mlua::Result<()> {
    let t = lua.create_table()?;
    t.set(
        "new",
        lua.create_function(
            |lua, (family, weight, style): (String, LuaValue, LuaValue)| {
                let (w, s) = weight_and_style(lua, &weight, &style);
                Ok(LFont(Font::new(&family, w, s)))
            },
        )?,
    )?;
    t.set(
        "fromName",
        lua.create_function(|lua, (name, weight, style): (String, LuaValue, LuaValue)| {
            let (w, s) = weight_and_style(lua, &weight, &style);
            Ok(LFont(Font::new(
                &format!("{FAMILY_FOLDER}{name}.json"),
                w,
                s,
            )))
        })?,
    )?;
    t.set(
        "fromId",
        lua.create_function(|lua, (id, weight, style): (u64, LuaValue, LuaValue)| {
            let (w, s) = weight_and_style(lua, &weight, &style);
            Ok(LFont(Font::new(&format!("rbxassetid://{id}"), w, s)))
        })?,
    )?;
    t.set(
        "fromEnum",
        lua.create_function(|lua, item: LuaValue| {
            item_name(lua, &item, "Font")
                .and_then(font_from_enum)
                .map(LFont)
                .ok_or_else(|| err("Font.fromEnum: invalid font"))
        })?,
    )?;
    lua.globals().set("Font", t)
}
