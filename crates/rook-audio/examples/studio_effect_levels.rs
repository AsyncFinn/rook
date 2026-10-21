//! Rook's sound effects against the levels Studio gave for them
//! (`docs/behavior/audio/soundgroups-and-effects.md`): `PlaybackLoudness` of the
//! install's `sounds/impact_explosion_03.mp3`, looped, averaged over two
//! loops, at Volume 0 so the measurement was silent. The audio API's
//! effects were measured with an AudioAnalyzer instead, which reads the
//! same; their levels here are Studio's ratio to the plain sound times
//! the plain level.
//!
//! `cargo run -p rook-audio --example studio_effect_levels -- <file>`

#![expect(
    clippy::disallowed_methods,
    reason = "an example run on a desktop, reading the install's file"
)]

use rook_audio::effects::{CROSSOVERS, Chain, Effect, Frame, Reverb, ordered};
use rook_content::decode::audio::Audio;

/// A setting's name, its effects with their `Priority`, and Studio's level.
type Case = (&'static str, Vec<(Effect, i32)>, f32);

/// Studio's mean `PlaybackLoudness` for each setting (Studio 0.742).
fn cases() -> Vec<Case> {
    let eq = |low, mid, high| Effect::Equalizer {
        low,
        mid,
        high,
        range: CROSSOVERS,
    };
    let dist = |level| Effect::Distortion { level };
    let wet = |decay: f32, density: f32, diffusion: f32| {
        Effect::Reverb(Reverb {
            decay,
            density,
            diffusion,
            dry: -80.0,
            ..Reverb::default()
        })
    };
    let pitch = |octave| Effect::PitchShift { octave };
    let hi = eq(-80.0, -80.0, 0.0);
    let chorus = |depth, mix, rate| Effect::Chorus { depth, mix, rate };
    let echo = |delay, dry, feedback| Effect::Echo {
        delay,
        dry,
        wet: 0.0,
        feedback,
    };
    let comp = |threshold, ratio, attack, release| Effect::Compressor {
        threshold,
        ratio,
        attack,
        release,
        makeup: 0.0,
    };
    // Studio 0.742: the audio API's level over the plain sound's, on this
    // scale. AudioEcho and AudioCompressor widen a mono stream to 7.1, so
    // their analyzer readings are divided by the 0.354 that alone gives.
    let api = |ratio: f32| 180.5 * ratio;
    vec![
        ("plain", vec![], 180.5),
        ("eq low -80", vec![(eq(-80.0, 0.0, 0.0), 0)], 54.3),
        ("eq mid -80", vec![(eq(0.0, -80.0, 0.0), 0)], 167.2),
        ("eq high -80", vec![(eq(0.0, 0.0, -80.0), 0)], 180.1),
        ("eq default", vec![(eq(-20.0, -10.0, 0.0), 0)], 28.0),
        ("eq all +10", vec![(eq(10.0, 10.0, 10.0), 0)], 571.6),
        ("eq high only", vec![(hi, 0)], 9.3),
        ("distortion 0.25", vec![(dist(0.25), 0)], 236.5),
        ("distortion 0.5", vec![(dist(0.5), 0)], 307.3),
        ("distortion 0.75", vec![(dist(0.75), 0)], 416.6),
        ("distortion 1", vec![(dist(1.0), 0)], 955.3),
        (
            "reverb default",
            vec![(Effect::Reverb(Reverb::default()), 0)],
            249.4,
        ),
        ("reverb wet, 0.3 s", vec![(wet(0.3, 1.0, 1.0), 0)], 224.8),
        ("reverb wet, 1.5 s", vec![(wet(1.5, 1.0, 1.0), 0)], 246.5),
        ("reverb wet, 5 s", vec![(wet(5.0, 1.0, 1.0), 0)], 270.5),
        ("reverb wet, sparse", vec![(wet(1.5, 0.0, 0.0), 0)], 218.4),
        ("pitch 0.5", vec![(pitch(0.5), 0)], 188.5),
        ("pitch 2", vec![(pitch(2.0), 0)], 142.1),
        (
            "pitch 2, then eq high",
            vec![(pitch(2.0), 10), (hi, 5)],
            15.9,
        ),
        ("eq high, then pitch 2", vec![(pitch(2.0), 0), (hi, 0)], 6.4),
        (
            "chorus defaults",
            vec![(chorus(0.45, 0.85, 5.0), 0)],
            api(1.042),
        ),
        (
            "chorus mix 1 depth 1",
            vec![(chorus(1.0, 1.0, 5.0), 0)],
            api(1.005),
        ),
        ("echo defaults", vec![(echo(1.0, 0.0, 0.5), 0)], api(1.451)),
        (
            "echo wet, 0.2 s",
            vec![(echo(0.2, -80.0, 0.0), 0)],
            api(0.974),
        ),
        (
            "echo wet, 0.2 s, fb 0.7",
            vec![(echo(0.2, -80.0, 0.7), 0)],
            api(1.315),
        ),
        (
            "comp defaults",
            vec![(comp(-40.0, 40.0, 0.1, 0.1), 0)],
            api(0.082),
        ),
        (
            "comp -20, 4:1",
            vec![(comp(-20.0, 4.0, 0.1, 0.1), 0)],
            api(0.436),
        ),
        (
            "comp -20, 4:1, fast",
            vec![(comp(-20.0, 4.0, 0.001, 0.1), 0)],
            api(0.246),
        ),
        (
            "comp -20, 50:1",
            vec![(comp(-20.0, 50.0, 0.1, 0.1), 0)],
            api(0.342),
        ),
        (
            "comp -20, 50:1, fastest",
            vec![(comp(-20.0, 50.0, 0.0001, 0.01), 0)],
            api(0.196),
        ),
        (
            "comp -10, 2:1, fastest",
            vec![(comp(-10.0, 2.0, 0.0001, 0.1), 0)],
            api(0.622),
        ),
    ]
}

fn main() {
    let path = std::env::args()
        .nth(1)
        .expect("the path of impact_explosion_03.mp3");
    let audio = Audio::decode(&std::fs::read(path).expect("read")).expect("decode");
    let channels = usize::from(audio.channels.max(1));
    let source: Vec<Frame> = (0..audio.frames())
        .map(|i| {
            let s =
                |c: usize| f32::from(audio.samples[i * channels + c.min(channels - 1)]) / 32768.0;
            [s(0), s(1)]
        })
        .collect();
    println!("{} Hz, {:.3} s", audio.sample_rate, audio.seconds());
    println!(
        "{:<24} {:>7} {:>7} {:>6}",
        "setting", "Studio", "Rook", "ratio"
    );
    for (name, effects, studio) in cases() {
        let mut chain = Chain::new(audio.sample_rate);
        chain.sync(&ordered(effects));
        let mut frames: Vec<Frame> = source.iter().chain(&source).copied().collect();
        chain.process(&mut frames);
        // PlaybackLoudness each 1/60 s: the root mean square of what played,
        // times 1000, both sides.
        let block = (audio.sample_rate / 60) as usize;
        let levels: Vec<f32> = frames
            .chunks(block)
            .map(|b| {
                (b.iter().map(|f| f[0] * f[0] + f[1] * f[1]).sum::<f32>() / (2 * b.len()) as f32)
                    .sqrt()
                    * 1000.0
            })
            .collect();
        let rook = levels.iter().sum::<f32>() / levels.len() as f32;
        println!(
            "{name:<24} {studio:>7.1} {rook:>7.1} {:>6.2}",
            rook / studio
        );
    }
}
