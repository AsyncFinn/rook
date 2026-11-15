//! Sound properties with rules of their own.

use rook_dom::{DomError, InstanceId};
use rook_types::Value;

use super::{SAME_AS, set_named, view};
use crate::runtime::world::World;
/// Sound properties with rules of their own (observed in Studio 0.743):
/// `Volume` is kept within 0 to 10 and `PlaybackSpeed` (and `Pitch`) within
/// 0 to 20, the roll-off distances at 0 or more; a `TimePosition` a script
/// sets is where the next `Play` starts.
pub(super) fn set_sound(
    w: &mut World,
    id: InstanceId,
    name: &str,
    value: &Value,
) -> Option<Result<(), DomError>> {
    let n = value.as_number();
    match (name, n) {
        ("Volume", Some(v)) => Some(set_named(
            w,
            id,
            "Volume",
            Value::Float32(v.clamp(0.0, 10.0) as f32),
        )),
        ("PlaybackSpeed" | "Pitch", Some(v)) => Some(set_named(
            w,
            id,
            "PlaybackSpeed",
            Value::Float32(v.clamp(0.0, 20.0) as f32),
        )),
        // Observed: a negative RollOffMinDistance reads 0; a minimum past
        // the maximum is kept as it is. A negative maximum is taken to be
        // 0 the same way (Inference).
        ("RollOffMinDistance" | "EmitterSize" | "RollOffMaxDistance" | "MaxDistance", Some(v)) => {
            let target = if name.contains("Max") {
                "RollOffMaxDistance"
            } else {
                "RollOffMinDistance"
            };
            Some(set_named(w, id, target, Value::Float32(v.max(0.0) as f32)))
        }
        ("TimePosition", Some(t)) => {
            w.sounds.entry(id).or_default().start = t;
            Some(set_named(w, id, "TimePosition", Value::Float64(t)))
        }
        _ => view(w, id, name, SAME_AS).map(|target| set_named(w, id, target, value.clone())),
    }
}
