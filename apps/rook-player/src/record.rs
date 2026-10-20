//! `--record-audio out.wav`: hear a place without a device. The game runs
//! in lockstep for `--seconds` and each frame's sounds go through the same
//! mixer the window uses, into a WAV file instead of the speakers. It says
//! how loud the mix got and where it would clip, so loudness can be
//! checked silently.

use std::io::Write;
use std::path::Path;

use rook_audio::effects::Frame;
use rook_audio::{Mixer, SAMPLE_RATE};
use rook_engine::console;
use rook_lua::MessageType;

use crate::STEP;
use crate::game::Game;

/// The loudest moment: when it was, and the sounds playing then, each
/// with its left and right gain.
struct Loudest {
    peak: f32,
    at: f64,
    sounds: Vec<(String, [f32; 2])>,
}

pub fn record(game: &mut Game, seconds: f64, out: &Path) -> anyhow::Result<()> {
    let mut mixer = Mixer::new(SAMPLE_RATE);
    let frames = (seconds / STEP).ceil() as usize;
    let mut samples: Vec<Frame> = Vec::with_capacity((seconds * f64::from(SAMPLE_RATE)) as usize);
    let mut buffer = Vec::new();
    let mut loudest = Loudest {
        peak: 0.0,
        at: 0.0,
        sounds: Vec::new(),
    };
    // Seconds (from the start) in which the mix went past full scale.
    let mut clipped_seconds = Vec::new();
    for frame in 0..frames {
        game.step();
        let mix = game.sounds();
        mixer.sync(&mix);
        // Whole frames per step, carrying the remainder so the file keeps
        // to game time.
        let end = ((frame + 1) as f64 * STEP * f64::from(SAMPLE_RATE)).round() as usize;
        let len = end - samples.len();
        buffer.resize(len * 2, 0.0);
        mixer.mix(&mut buffer, 2);
        let mixed = mixer.mixed();
        let peak = mixed
            .iter()
            .flat_map(|f| f.iter())
            .fold(0.0f32, |m, s| m.max(s.abs()));
        let at = samples.len() as f64 / f64::from(SAMPLE_RATE);
        if peak > 1.0 && clipped_seconds.last() != Some(&(at as u32)) {
            clipped_seconds.push(at as u32);
        }
        if peak > loudest.peak {
            let dom = &game.view().runtime().world().dom;
            loudest = Loudest {
                peak,
                at,
                sounds: mix
                    .sounds
                    .iter()
                    .map(|p| (dom.full_name(p.key), p.gains))
                    .collect(),
            };
        }
        samples.extend_from_slice(mixed);
    }
    write_wav(out, &samples)?;
    report(out, &samples, &loudest, &clipped_seconds);
    Ok(())
}

fn report(out: &Path, samples: &[Frame], loudest: &Loudest, clipped_seconds: &[u32]) {
    let all = samples.iter().flat_map(|f| f.iter());
    let clipped = all.clone().filter(|s| s.abs() > 1.0).count();
    let rms = (all.map(|s| s * s).sum::<f32>() / (samples.len() * 2).max(1) as f32).sqrt();
    let db = |v: f32| 20.0 * v.max(1e-9).log10();
    let mut text = format!(
        "Saved {:.1} s of audio to {}\n  peak {:.3} ({:+.1} dBFS) at {:.2} s, RMS {:.4} ({:+.1} dBFS)\n  {clipped} samples past full scale",
        samples.len() as f64 / f64::from(SAMPLE_RATE),
        out.display(),
        loudest.peak,
        db(loudest.peak),
        loudest.at,
        rms,
        db(rms),
    );
    if !clipped_seconds.is_empty() {
        let list: Vec<String> = clipped_seconds.iter().map(u32::to_string).collect();
        text += &format!(", in seconds {}", list.join(", "));
    }
    if !loudest.sounds.is_empty() {
        text += "\n  playing at the peak (left, right gain):";
        let mut sounds: Vec<_> = loudest.sounds.iter().collect();
        sounds.sort_by(|a, b| b.1[0].max(b.1[1]).total_cmp(&a.1[0].max(a.1[1])));
        for (name, [l, r]) in sounds.iter().take(10) {
            text += &format!("\n    {l:.2}, {r:.2}  {name}");
        }
        if sounds.len() > 10 {
            text += &format!("\n    and {} more", sounds.len() - 10);
        }
    }
    console::print_message(MessageType::Info, &text);
}

/// 32-bit float stereo WAV, as the device gets it: past full scale where
/// the mix is.
fn write_wav(out: &Path, samples: &[Frame]) -> anyhow::Result<()> {
    let data = (samples.len() * 8) as u32;
    let mut bytes = Vec::with_capacity(44 + data as usize);
    bytes.extend_from_slice(b"RIFF");
    bytes.extend_from_slice(&(36 + data).to_le_bytes());
    bytes.extend_from_slice(b"WAVEfmt ");
    bytes.extend_from_slice(&16u32.to_le_bytes());
    bytes.extend_from_slice(&3u16.to_le_bytes()); // IEEE float
    bytes.extend_from_slice(&2u16.to_le_bytes());
    bytes.extend_from_slice(&SAMPLE_RATE.to_le_bytes());
    bytes.extend_from_slice(&(SAMPLE_RATE * 8).to_le_bytes());
    bytes.extend_from_slice(&8u16.to_le_bytes());
    bytes.extend_from_slice(&32u16.to_le_bytes());
    bytes.extend_from_slice(b"data");
    bytes.extend_from_slice(&data.to_le_bytes());
    for s in samples.iter().flat_map(|f| f.iter()) {
        bytes.extend_from_slice(&s.to_le_bytes());
    }
    fs_err::File::create(out)?.write_all(&bytes)?;
    Ok(())
}
