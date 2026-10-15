//! AudioPlayers playing: `TimePosition` moving on, ends and loops
//! (observed in Studio 0.742 on a playtest server; `docs/behavior/audio.md`,
//! step 5).
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
    if !flag(dom, id, "IsPlaying") {
        return;
    }
    let length = audio.seconds();
    let speed = number(dom, id, "PlaybackSpeed", 1.0).max(0.0);
    let before = number(dom, id, "TimePosition", 0.0);
    let mut t = if before >= length {
        before
    } else {
        before.max(0.0) + dt * speed
    };
    if t >= length {
        if flag(dom, id, "Looping") && length > 0.0 {
            t %= length;
            controls.entry(id).or_default().loops += 1;
            events.push((id, "Looped", None, None));
        } else {
            let _ = dom.set_property_by_name(id, "IsPlaying", Value::Bool(false));
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
