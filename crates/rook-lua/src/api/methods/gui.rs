//! GUI methods.

use mlua::Lua;
use rook_dom::InstanceId;
use rook_types::Vector2;

use super::RawMethod;
use super::ret_instances;
use crate::ctx::args::Args;
use crate::ctx::{ApiResult, Outcome, with_world};
use crate::signal::SignalKey;
use crate::threads::fire;
use crate::types::LVector2;

pub(crate) fn native(owner: &str, name: &str) -> Option<RawMethod> {
    Some(match (owner, name) {
        ("GuiService", "GetGuiInset") => get_gui_inset,
        ("GuiService", "GetInsetArea") => get_inset_area,
        ("GuiService", "GetHardwareSafeViewport") => get_hardware_safe_viewport,
        ("BasePlayerGui", "GetGuiObjectsAtPosition") => get_gui_objects_at_position,
        ("PlayerGui", "GetTopbarTransparency") => get_topbar_transparency,
        ("PlayerGui", "SetTopbarTransparency") => set_topbar_transparency,
        _ => return None,
    })
}

/// UIPageLayout's methods, written in Luau over the `std.page_layout`
/// helpers (`luau/page_layout.luau`).
pub(super) fn luau_source(owner: &str, name: &str) -> Option<&'static str> {
    Some(match (owner, name) {
        ("UIPageLayout", "JumpTo") => {
            "local p = (...).page_layout return function(self, page) p.jumpTo(self, page) end"
        }
        ("UIPageLayout", "JumpToIndex") => {
            "local p = (...).page_layout return function(self, index) p.jumpToIndex(self, index) end"
        }
        ("UIPageLayout", "Next") => {
            "local p = (...).page_layout return function(self) p.step(self, 1) end"
        }
        ("UIPageLayout", "Previous") => {
            "local p = (...).page_layout return function(self) p.step(self, -1) end"
        }
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

/// The window's size: the current camera's `ViewportSize`.
fn viewport_size(lua: &Lua) -> Vector2 {
    with_world(lua, |w| {
        let dom = &w.dom;
        dom.find_service("Workspace")
            .and_then(|ws| dom.ref_property(ws, "CurrentCamera"))
            .and_then(|c| dom.vector2_property(c, "ViewportSize"))
            .unwrap_or(Vector2::new(0.0, 0.0))
    })
}

/// The part of the screen no hardware covers (notches, rounded corners):
/// all of it on a desktop.
fn get_hardware_safe_viewport(lua: &Lua, _: InstanceId, _: &Args) -> ApiResult<Outcome> {
    Outcome::ret(lua, LVector2(viewport_size(lua)))
}

/// The screen below the top bar, for any `Enum.ScreenInsets` (Inference:
/// the top bar is the only inset a desktop has).
fn get_inset_area(lua: &Lua, _: InstanceId, _: &Args) -> ApiResult<Outcome> {
    let size = viewport_size(lua);
    let area = rbx_types::Rect::new(
        rbx_types::Vector2::new(0.0, gui_inset(lua)),
        rbx_types::Vector2::new(size.0.x, size.0.y),
    );
    Outcome::ret(lua, crate::types::LRect(area))
}

/// Measured: 0.5 until a script sets it.
fn get_topbar_transparency(lua: &Lua, this: InstanceId, _: &Args) -> ApiResult<Outcome> {
    let t = with_world(lua, |w| w.topbar_transparency.get(&this).copied());
    Outcome::ret(lua, t.unwrap_or(0.5))
}

/// Measured: deprecated but still kept, clamped to 0 to 1, and announced
/// on `TopbarTransparencyChangedSignal`. Rook draws no top bar to apply it to.
fn set_topbar_transparency(lua: &Lua, this: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let t = (a.number(0)? as f32).clamp(0.0, 1.0);
    with_world(lua, |w| w.topbar_transparency.insert(this, t));
    let key = SignalKey::event(this, "TopbarTransparencyChangedSignal");
    fire(lua, &key, &[mlua::Value::Number(f64::from(t))]);
    Outcome::ret(lua, ())
}

/// The local player's on-screen GUI objects at (x, y) in GUI coordinates,
/// top first (measured: the order they're drawn in, reversed, as of the
/// last frame's layout).
fn get_gui_objects_at_position(lua: &Lua, _: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let at = glam::Vec2::new(a.number(0)? as f32, a.number(1)? as f32);
    let found = with_world(lua, |w| w.gui.objects_at(&w.dom, at));
    ret_instances(lua, &found)
}
