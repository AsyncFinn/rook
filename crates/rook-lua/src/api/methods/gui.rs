//! GUI methods.

use mlua::Lua;
use rook_dom::InstanceId;
use rook_types::Vector2;

use super::RawMethod;
use crate::ctx::args::Args;
use crate::ctx::{ApiResult, Outcome, with_world};
use crate::types::LVector2;

pub(crate) fn native(owner: &str, name: &str) -> Option<RawMethod> {
    Some(match (owner, name) {
        ("GuiService", "GetGuiInset") => get_gui_inset,
        _ => return None,
    })
}

/// The top bar's height on this side: a client's has one, a server's
/// doesn't (measured: a server's `ScreenPointToRay` and
/// `ViewportPointToRay` agree; conformance case `physics/queries`).
pub(crate) fn gui_inset(lua: &Lua) -> f32 {
    if with_world(lua, |w| w.is_client) {
        rook_gui::GUI_INSET
    } else {
        0.0
    }
}

/// Measured on a client: (0, 58) and (0, 0), the top bar above
/// ScreenGuis.
fn get_gui_inset(lua: &Lua, _: InstanceId, _: &Args) -> ApiResult<Outcome> {
    let top_left = LVector2(Vector2::new(0.0, gui_inset(lua)));
    Outcome::ret(lua, (top_left, LVector2(Vector2::new(0.0, 0.0))))
}
