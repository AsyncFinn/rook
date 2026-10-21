//! AudioPlayers playing: `TimePosition` moving on, ends and loops
//! (observed in Studio 0.742 on a playtest server; `docs/behavior/audio/audio-api.md`).
//!
//! Unlike a Sound's, an AudioPlayer's end is a change scripts see:
//! `IsPlaying` turns false with `Changed`, then `Ended` fires, and
//! `TimePosition` goes back to 0. `Stop` keeps the position and `Play`
//! goes on from it (`rook-lua`'s `methods/audio.rs`).

use super::*;

/// Move a playing, loaded player on by `dt` times `PlaybackSpeed`. One
/// past its end, by time or because a script put `TimePosition` there,
/// ends; a `Looping` one wraps and fires `Looped`. A negative position
/// plays from 0. A player still loading, or whose file failed, stays where
/// it is.
pub(super) fn advance(
    dom: &mut Dom,
    controls: &mut HashMap<InstanceId, rook_lua::SoundControl>,
    id: InstanceId,
    audio: &Audio,
    dt: f64,
    events: &mut Vec<SoundEvent>,
) {
    if !dom.bool_property(id, "IsPlaying").unwrap_or(false) {
        return;
    }
    let length = audio.seconds();
    let speed = dom
        .number_property(id, "PlaybackSpeed")
        .unwrap_or(1.0)
        .max(0.0);
    let before = dom.number_property(id, "TimePosition").unwrap_or(0.0);
    let mut t = if before >= length {
        before
    } else {
        before.max(0.0) + dt * speed
    };
    if t >= length {
        if dom.bool_property(id, "Looping").unwrap_or(false) && length > 0.0 {
            t %= length;
            controls.entry(id).or_default().loops += 1;
            events.push((id, "Looped", None, None));
        } else {
            rook_dom::ignore_stale(dom.set_property_by_name(id, "IsPlaying", Value::Bool(false)));
            t = 0.0;
            events.push((id, "Ended", None, None));
        }
    }
    // Time passing isn't a change scripts hear about (no Changed each
    // frame), so it skips the journal.
    if let Some(p) = dom.property_id(id, "TimePosition") {
        dom.load_property(id, p, Value::Float64(t));
    }
}
