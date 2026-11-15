//! The weld holding a Tool's Handle in the right hand, and what equipping
//! and unequipping do to the Handle (measured in Studio 0.747,
//! docs/behavior/characters/tools.md#the-grip).
//!
//! The grip is a `Weld` named `RightGrip` in the hand, from the hand to
//! the Handle: `C0` from the rig, `C1` the Tool's `Grip`. A player's own
//! client makes its character's grip in Roblox; here both sides make it,
//! and the server's replicates in place of the client's (see
//! `rook_net::Source::adopt`). Equipping changes nothing on the arm: the
//! raised arm is the Animate script's `toolnone` animation.
// TODO: the raised arm once M9's animation step plays Animate's `toolnone`.

use glam::{Mat3, Vec3};
use mlua::Lua;
use rook_dom::{Dom, DomError, InstanceId};
use rook_types::{CFrame, Value, Vector3};

use super::{handle, holder};
use crate::api::methods::characters::root_part;
use crate::api::methods::mutate;
use crate::api::properties;
use crate::ctx::with_world;
use crate::world::World;

const NAME: &str = "RightGrip";

/// Turned -90 degrees about X: the Handle's length along the arm.
const ALONG_ARM: Mat3 = Mat3::from_cols(
    Vec3::new(1.0, 0.0, 0.0),
    Vec3::new(0.0, 0.0, -1.0),
    Vec3::new(0.0, 1.0, 0.0),
);

/// Where a dropped or unequipped Handle goes, in the root part's space
/// (measured).
const LET_GO: Vec3 = Vec3::new(0.0, 4.0, -8.0);

/// The hand holding a character's tools: `RightHand` on an R15 Humanoid,
/// `Right Arm` otherwise, by name (measured: a renamed hand gets no grip),
/// with the grip's `C0`.
fn hand(dom: &Dom, character: InstanceId) -> Option<(InstanceId, CFrame)> {
    let humanoid = dom.find_first_child_of_class(character, "Humanoid")?;
    let r15 = dom.enum_item_property(humanoid, "RigType") == Some("R15");
    let name = if r15 { "RightHand" } else { "Right Arm" };
    let hand = dom
        .find_first_child(character, name, false)
        .filter(|h| dom.is_a(*h, "BasePart"))?;
    // Measured: R15 takes the hand's RightGripAttachment, else 0.2 below
    // its center; R6 always 1 below, whatever the arm's size or its
    // attachment.
    let attachment = r15
        .then(|| dom.find_first_child(hand, "RightGripAttachment", false))
        .flatten()
        .filter(|a| dom.is_a(*a, "Attachment"))
        .and_then(|a| dom.cframe_property(a, "CFrame"));
    let below = if r15 { 0.2 } else { 1.0 };
    let c0 = attachment.unwrap_or(CFrame {
        position: Vec3::new(0.0, -below, 0.0),
        rotation: ALONG_ARM,
    });
    Some((hand, c0))
}

/// The grip holding `tool`'s Handle, if one does.
pub(crate) fn find(dom: &Dom, tool: InstanceId) -> Option<InstanceId> {
    let handle = handle(dom, tool)?;
    let db = dom.database();
    dom.referrers(handle)
        .iter()
        .find(|(weld, p)| {
            db.property(*p).name == "Part1"
                && !dom.is_destroyed(*weld)
                && dom.class_name(*weld) == "Weld"
                && dom.name(*weld) == NAME
        })
        .map(|(weld, _)| *weld)
}

fn set(w: &mut World, id: InstanceId, name: &str, value: Value) -> Result<(), DomError> {
    w.dom.set_property_by_name(id, name, value)
}

/// `character` took hold of `tool`: the Handle stops colliding, then the
/// grip welds it to the hand (and snaps it there, as any new weld does).
pub(super) fn hold(lua: &Lua, tool: InstanceId, character: InstanceId) {
    let result = mutate(lua, |w| {
        let Some(handle) = handle(&w.dom, tool) else {
            return Ok(());
        };
        set(w, handle, "CanCollide", Value::Bool(false))?;
        let Some((hand, c0)) = hand(&w.dom, character) else {
            return Ok(());
        };
        let c1 = w
            .dom
            .cframe_property(tool, "Grip")
            .unwrap_or(CFrame::IDENTITY);
        let weld = w.dom.create_by_name("Weld")?;
        w.dom.set_name(weld, NAME.to_owned())?;
        set(w, weld, "Part0", Value::Ref(Some(hand.to_ref())))?;
        set(w, weld, "Part1", Value::Ref(Some(handle.to_ref())))?;
        set(w, weld, "C0", Value::CFrame(c0))?;
        set(w, weld, "C1", Value::CFrame(c1))?;
        w.dom.set_parent(weld, Some(hand))
    });
    if let Err(e) = result {
        tracing::debug!("making a tool's grip failed: {e:?}");
    }
}

/// `character` let go of `tool`: the grip goes, the Handle collides again
/// (whatever it did before, measured) and moves to 4 studs up and 8 ahead
/// of the root part, keeping its rotation.
pub(super) fn release(lua: &Lua, tool: InstanceId, character: InstanceId) {
    if let Some(weld) = with_world(lua, |w| find(&w.dom, tool))
        && let Err(e) = crate::removal::destroy(lua, weld)
    {
        tracing::debug!("removing a tool's grip failed: {e}");
    }
    let result = mutate(lua, |w| {
        let Some(handle) = handle(&w.dom, tool) else {
            return Ok(());
        };
        set(w, handle, "CanCollide", Value::Bool(true))?;
        let Some(root) = root_part(&w.dom, character) else {
            return Ok(());
        };
        let at = properties::part_cframe(w, root) * Vector3(LET_GO);
        let c = CFrame {
            position: at.0,
            ..properties::part_cframe(w, handle)
        };
        properties::set_cframe(w, handle, c)
    });
    if let Err(e) = result {
        tracing::debug!("letting go of a tool failed: {e:?}");
    }
}

/// A held Tool's `Grip` changed: its grip's `C1` follows (measured).
pub(crate) fn follow_grip(w: &mut World, tool: InstanceId) -> Result<(), DomError> {
    if holder(&w.dom, tool).is_none() {
        return Ok(());
    }
    let Some(weld) = find(&w.dom, tool) else {
        return Ok(());
    };
    let grip = w
        .dom
        .cframe_property(tool, "Grip")
        .unwrap_or(CFrame::IDENTITY);
    set(w, weld, "C1", Value::CFrame(grip))
}
