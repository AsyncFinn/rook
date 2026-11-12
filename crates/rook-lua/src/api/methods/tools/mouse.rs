//! The Mouse `Equipped` passes (measured in Studio 0.747,
//! docs/behavior/characters/tools.md#equipped): a new one each equip, of
//! class `Mouse`, not the PlayerMouse. On the client it acts as the
//! player's mouse while the local character holds the Tool; the input
//! bridge writes and fires it with the PlayerMouse (`InputState::tool_mice`).
//! Once the Tool is let go, and always on the server, it's inactive:
//! reading its properties errors, writing them doesn't.

use mlua::Lua;
use rook_dom::{Dom, InstanceId};

use super::holder;
use crate::ctx::with_world;
use crate::world::World;

/// What the PlayerMouse knows of the cursor, which a new tool Mouse starts
/// with.
const LOOK: [&str; 8] = [
    "Hit",
    "Origin",
    "Target",
    "X",
    "Y",
    "ViewSizeX",
    "ViewSizeY",
    "UnitRay",
];

/// Whether the local character holds `tool`.
fn held_here(dom: &Dom, tool: InstanceId) -> bool {
    let character = dom
        .local_player()
        .and_then(|p| dom.ref_property(p, "Character"));
    character.is_some() && holder(dom, tool) == character
}

/// A new Mouse for `tool`'s `Equipped`: active on the client whose
/// character holds it.
pub(crate) fn equipped_mouse(lua: &Lua, tool: InstanceId) -> Option<InstanceId> {
    with_world(lua, |w| {
        let mouse = w.dom.create_by_name("Mouse").ok()?;
        // Measured: Roblox names it so.
        if let Err(e) = w.dom.set_name(mouse, "Instance".to_owned()) {
            tracing::warn!("naming a tool's Mouse failed: {e}");
        }
        if w.is_client && held_here(&w.dom, tool) {
            if let Some(player_mouse) = w.input.mouse_object {
                // Mouse's own properties, which the PlayerMouse inherits.
                for name in LOOK {
                    if let Some(p) = w.dom.property_id(mouse, name)
                        && let Some(v) = w.dom.property(player_mouse, p)
                        && let Err(e) = w.dom.set_property(mouse, p, v)
                    {
                        tracing::warn!("copying Mouse.{name} failed: {e}");
                    }
                }
            }
            w.input.tool_mice.push((tool, mouse));
        }
        Some(mouse)
    })
}

/// A Tool moved: its Mice stop when the local character no longer holds
/// it.
pub(crate) fn retire_mice(w: &mut World, tool: InstanceId) {
    if w.input.tool_mice.is_empty() || held_here(&w.dom, tool) {
        return;
    }
    w.input.tool_mice.retain(|(t, _)| *t != tool);
}

/// Whether `id` is a tool's Mouse, whose reads depend on whether it's
/// active, so never take the fast path for plain stored values.
pub(crate) fn is_tool_mouse(dom: &Dom, id: InstanceId) -> bool {
    dom.class_name(id) == "Mouse"
}

/// The error reading `property` of an inactive tool Mouse gives: one of
/// the Mouse's own (measured: `Hit`, `Icon`, `X`; `Name` and `ClassName`
/// still read).
pub(crate) fn inactive_mouse_error(
    w: &World,
    id: InstanceId,
    property: Option<rook_reflection::PropertyId>,
) -> Option<&'static str> {
    if !is_tool_mouse(&w.dom, id) || w.input.tool_mice.iter().any(|(_, m)| *m == id) {
        return None;
    }
    let db = w.dom.database();
    let own = property.is_some_and(|p| db.class(db.property(p).owner).name == "Mouse");
    own.then_some("This Mouse is no longer active")
}
