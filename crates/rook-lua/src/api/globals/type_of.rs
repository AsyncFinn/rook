//! Roblox's `typeof`.

use super::*;

/// Roblox's `typeof`: datatype names for userdata and `Vector3` for vectors.
#[expect(
    clippy::unnecessary_wraps,
    reason = "Luau calls it through mlua, which wants a Result"
)]
pub(super) fn typeof_(_: &Lua, args: MultiValue) -> ApiResult<String> {
    let v = args.into_iter().next().unwrap_or(LuaValue::Nil);
    Ok(type_name(&v))
}
