//! What an in-game menu calls on the engine: `GuiService:SetMenuIsOpen`,
//! which games hear through `MenuIsOpen`, `MenuOpened` and `MenuClosed`,
//! and `DataModel:Shutdown`, its Leave button. See
//! docs/behavior/gui/menu.md.

use mlua::Lua;
use rook_dom::InstanceId;
use rook_types::Value;

use super::{RawMethod, mutate};
use crate::runtime::ctx::args::Args;
use crate::runtime::ctx::{ApiResult, Outcome, with_world};
use crate::runtime::signal::SignalKey;
use crate::runtime::threads::fire;

pub(crate) fn native(owner: &str, name: &str) -> Option<RawMethod> {
    Some(match (owner, name) {
        ("GuiService", "SetMenuIsOpen") => set_menu_is_open,
        ("DataModel", "Shutdown") => shutdown,
        _ => return None,
    })
}

/// The menu opened or closed: `MenuIsOpen` follows, and `MenuOpened` or
/// `MenuClosed` fires when it changes (creator-docs GuiService.MenuIsOpen,
/// MenuOpened, MenuClosed). The menu's name isn't kept.
fn set_menu_is_open(lua: &Lua, this: InstanceId, a: &Args) -> ApiResult<Outcome> {
    let open = a.truthy(0);
    let was = with_world(lua, |w| w.dom.bool_property(this, "MenuIsOpen")) == Some(true);
    if open != was {
        mutate(lua, |w| {
            w.dom
                .set_property_by_name(this, "MenuIsOpen", Value::Bool(open))
        })?;
        let event = if open { "MenuOpened" } else { "MenuClosed" };
        fire(lua, &SignalKey::event(this, event), &[]);
    }
    Outcome::ret(lua, ())
}

/// A client's Leave: the engine's host closes it once the step is over
/// (`World::shutdown`); a server ignores it (Inference: Roblox's
/// servers are shut down from its website).
fn shutdown(lua: &Lua, _: InstanceId, _: &Args) -> ApiResult<Outcome> {
    with_world(lua, |w| {
        if w.is_client {
            w.shutdown = true;
        }
    });
    Outcome::ret(lua, ())
}
