//! Lighting's clock as scripts see it: `ClockTime` and `TimeOfDay` are
//! two views of one time, stored as the `TimeOfDay` string files save.
//! Both read it back through [`rook_types::Clock`], which holds the rules
//! for each way of setting it. docs/behavior/lighting/clock.md.

use rook_dom::{Dom, DomError, InstanceId};
use rook_reflection::PropertyId;
use rook_types::{Clock, Value};

use crate::runtime::world::World;

pub(super) fn get(w: &World, id: InstanceId, name: &str) -> Option<Value> {
    Some(match name {
        "ClockTime" => Value::Float32(w.dom.clock(id).hours() as f32),
        // Normalized however the string was saved (Confirmed: what a
        // script sets reads back as `HH:MM:SS`).
        "TimeOfDay" => Value::String(w.dom.clock(id).to_string()),
        _ => return None,
    })
}

pub(super) fn set(
    w: &mut World,
    id: InstanceId,
    name: &str,
    value: &Value,
) -> Option<Result<(), DomError>> {
    let clock = match (name, value) {
        ("ClockTime", Value::Float32(hours)) => Clock::from_hours(*hours),
        // `rejected` has refused one that doesn't read.
        ("TimeOfDay", Value::String(s)) => s.parse().ok()?,
        _ => return None,
    };
    Some(set_clock(w, id, clock))
}

/// Set the time. Under a second's change stores the same string, so
/// nothing reports a change (Confirmed).
pub(crate) fn set_clock(w: &mut World, id: InstanceId, clock: Clock) -> Result<(), DomError> {
    super::set_named(w, id, "TimeOfDay", Value::String(clock.to_string()))
}

/// Studio's error for a `TimeOfDay` that doesn't read; the old time stays.
pub(super) fn rejected(name: &str, value: &Value) -> Option<String> {
    match (name, value) {
        ("TimeOfDay", Value::String(s)) => s.parse::<Clock>().err().map(|e| e.to_string()),
        _ => None,
    }
}

/// The view that reports a change of `property` of `id` too: `ClockTime`
/// with Lighting's `TimeOfDay`. Setting either, or
/// `SetMinutesAfterMidnight`, fires both (Confirmed).
pub(crate) fn changes_with(dom: &Dom, id: InstanceId, property: PropertyId) -> Option<PropertyId> {
    (dom.database().property(property).name == "TimeOfDay" && dom.is_a(id, "Lighting"))
        .then(|| dom.property_id(id, "ClockTime"))
        .flatten()
}
