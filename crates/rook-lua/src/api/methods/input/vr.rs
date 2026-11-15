//! VRService's tracked points, which Rook has no headset for.

use mlua::{Lua, Value as LuaValue};
use rook_dom::InstanceId;
use rook_types::CFrame;

use crate::runtime::ctx::args::Args;
use crate::runtime::ctx::{ApiResult, Outcome};
use crate::types::cframe::LCFrame;
use crate::types::enums::item_value;

/// Rook has no VR headset, so every tracked point sits at the origin and
/// none is tracked (Observed: Studio 0.747 without a headset gives the
/// identity CFrame and false for every `Enum.UserCFrame`).
pub(super) fn user_cframe(lua: &Lua, _: InstanceId, a: &Args) -> ApiResult<Outcome> {
    user_cframe_arg(lua, a)?;
    Outcome::ret(lua, LCFrame(CFrame::IDENTITY))
}

pub(super) fn user_cframe_enabled(lua: &Lua, _: InstanceId, a: &Args) -> ApiResult<Outcome> {
    user_cframe_arg(lua, a)?;
    Outcome::ret(lua, false)
}

fn user_cframe_arg(lua: &Lua, a: &Args) -> ApiResult<u32> {
    match a.get(0) {
        LuaValue::Nil => Err(a.missing(0)),
        v => item_value(lua, v, "UserCFrame").ok_or_else(|| a.wrong(0, "UserCFrame")),
    }
}
