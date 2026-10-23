//! TextService: measuring text as GUI objects lay it out.

use mlua::Lua;
use rook_dom::InstanceId;
use rook_types::Vector2;

use super::RawMethod;
use crate::ctx::args::Args;
use crate::ctx::{ApiResult, Outcome, with_world};
use crate::types::LVector2;
use crate::types::enums::item_name;

pub(crate) fn native(owner: &str, name: &str) -> Option<RawMethod> {
    Some(match (owner, name) {
        ("TextService", "GetTextSize") => get_text_size,
        ("TextService", "GetTextSizeOffsetAsync") => get_text_size_offset,
        _ => return None,
    })
}

/// `GetTextSize(text, size, font, frameSize)`: the bounds of `text` in an
/// `Enum.Font` wrapped to the frame's width (creator-docs
/// TextService:GetTextSize).
fn get_text_size(lua: &Lua, _: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let text = a.string(0)?;
    let size = a.number(1)? as f32;
    let font = item_name(lua, a.get(2), "Font")
        .and_then(rook_types::font_from_enum)
        .ok_or_else(|| a.wrong(2, "Font"))?;
    let frame = a.userdata::<LVector2>(3, "Vector2")?.0;
    let bounds = with_world(lua, |w| w.gui.text_size(&text, font, size, frame.0));
    Outcome::ret(lua, LVector2(Vector2::new(bounds.x, bounds.y)))
}

/// How far a font's text sits from where its line starts; Rook's fonts
/// place text at the line's start.
fn get_text_size_offset(lua: &Lua, _: InstanceId, _: &Args) -> ApiResult<Outcome> {
    Outcome::ret(lua, 0.0)
}
