//! The ranges the audio API's properties keep to: a value set outside one
//! reads back at its end (all observed in Studio 0.742; setting -1e9 and
//! 1e9 on each). Unlike the legacy SoundEffects, which read back anything.

use rbx_types::NumberRange;
use rook_dom::{DomError, InstanceId};
use rook_types::Value;

use crate::world::World;

/// (class, property, lowest, highest).
const RANGES: &[(&str, &str, f64, f64)] = &[
    ("AudioPlayer", "Volume", 0.0, 10.0),
    ("AudioPlayer", "PlaybackSpeed", 0.0, 20.0),
    ("AudioFader", "Volume", 0.0, 3.0),
    ("AudioEqualizer", "LowGain", -80.0, 10.0),
    ("AudioEqualizer", "MidGain", -80.0, 10.0),
    ("AudioEqualizer", "HighGain", -80.0, 10.0),
    ("AudioDistortion", "Level", 0.0, 1.0),
    ("AudioPitchShifter", "Pitch", 0.5, 2.0),
    ("AudioReverb", "DecayRatio", 0.1, 1.0),
    ("AudioReverb", "DecayTime", 0.1, 20.0),
    ("AudioReverb", "Density", 0.1, 1.0),
    ("AudioReverb", "Diffusion", 0.1, 1.0),
    ("AudioReverb", "DryLevel", -80.0, 20.0),
    ("AudioReverb", "EarlyDelayTime", 0.0, 0.3),
    ("AudioReverb", "HighCutFrequency", 20.0, 20_000.0),
    ("AudioReverb", "LateDelayTime", 0.0, 0.1),
    ("AudioReverb", "LowShelfFrequency", 20.0, 20_000.0),
    ("AudioReverb", "LowShelfGain", -36.0, 12.0),
    ("AudioReverb", "ReferenceFrequency", 20.0, 20_000.0),
    ("AudioReverb", "WetLevel", -80.0, 20.0),
    ("AudioEcho", "DelayTime", 0.001, 5.0),
    ("AudioEcho", "DryLevel", -80.0, 10.0),
    ("AudioEcho", "Feedback", 0.0, 1.0),
    ("AudioEcho", "RampTime", 0.0, 60_000.0),
    ("AudioEcho", "WetLevel", -80.0, 10.0),
    ("AudioChorus", "Depth", 0.0, 1.0),
    ("AudioChorus", "Mix", 0.0, 1.0),
    ("AudioChorus", "Rate", 0.0, 20.0),
    ("AudioFlanger", "Depth", 0.01, 1.0),
    ("AudioFlanger", "Mix", 0.0, 1.0),
    ("AudioFlanger", "Rate", 0.0, 20.0),
    ("AudioCompressor", "Attack", 0.0001, 0.5),
    ("AudioCompressor", "MakeupGain", -30.0, 30.0),
    ("AudioCompressor", "Ratio", 1.0, 50.0),
    ("AudioCompressor", "Release", 0.01, 5.0),
    ("AudioCompressor", "Threshold", -60.0, 0.0),
    ("AudioFilter", "Frequency", 20.0, 22_000.0),
    ("AudioFilter", "Gain", -30.0, 30.0),
    ("AudioFilter", "Q", 0.1, 10.0),
    ("AudioTremolo", "Depth", 0.0, 1.0),
    ("AudioTremolo", "Duty", 0.0, 1.0),
    ("AudioTremolo", "Frequency", 0.0, 20.0),
    ("AudioTremolo", "Shape", 0.0, 1.0),
    ("AudioTremolo", "Skew", -1.0, 1.0),
    ("AudioTremolo", "Square", 0.0, 1.0),
    ("AudioLimiter", "MaxLevel", -12.0, 0.0),
    ("AudioLimiter", "Release", 0.001, 1.0),
];

/// AudioEqualizer's `MidRange` ends each keep within 200 to 20,000 Hz
/// (10 to 30,000 read back as 200 to 20,000).
const MID_RANGE: (f32, f32) = (200.0, 20_000.0);

/// Store `value` within its range, if `name` has one on `id`'s class.
/// NaN reads back as the highest value (observed for AudioPlayer's
/// `Volume`; Inference for the rest).
pub(crate) fn clamp(
    w: &mut World,
    id: InstanceId,
    name: &str,
    value: &Value,
) -> Option<Result<(), DomError>> {
    let class = w.dom.class_name(id);
    if name == "MidRange"
        && class == "AudioEqualizer"
        && let Value::NumberRange(r) = value
    {
        let (lo, hi) = MID_RANGE;
        let range = NumberRange::new(r.min.clamp(lo, hi), r.max.clamp(lo, hi));
        return Some(
            w.dom
                .set_property_by_name(id, name, Value::NumberRange(range)),
        );
    }
    let &(_, _, lo, hi) = RANGES
        .iter()
        .find(|(c, p, _, _)| *c == class && *p == name)?;
    let v = value.as_number()?;
    let v = if v.is_nan() { hi } else { v.clamp(lo, hi) };
    let stored = match w.dom.property_by_name(id, name) {
        Some(Value::Float64(_)) => Value::Float64(v),
        _ => Value::Float32(v as f32),
    };
    Some(w.dom.set_property_by_name(id, name, stored))
}
