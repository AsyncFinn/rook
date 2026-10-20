use super::*;

/// A stereo file with `samples` on both sides (a mono one plays 3 dB
/// down; see `mono_is_centred`).
fn audio(rate: u32, samples: &[i16]) -> Arc<Audio> {
    Arc::new(Audio {
        format: None,
        sample_rate: rate,
        channels: 2,
        samples: samples.iter().flat_map(|&s| [s, s]).collect(),
    })
}

fn play(audio: &Arc<Audio>, position: f64) -> Play<u32> {
    Play {
        key: 1,
        audio: audio.clone(),
        position,
        gains: [1.0, 1.0],
        speed: 1.0,
        looped: false,
        generation: 0,
        effects: Vec::new(),
        group: None,
    }
}

fn only(sounds: Vec<Play<u32>>) -> Mix<u32> {
    Mix {
        sounds,
        ..Mix::default()
    }
}

fn group(key: u32, parent: Option<u32>, volume: f32) -> Group<u32> {
    Group {
        key,
        parent,
        volume,
        effects: Vec::new(),
    }
}

#[test]
fn resamples_and_ends() {
    // 4 frames at 2 Hz, mixed at 4 Hz: every source frame twice, with
    // the half-way frames interpolated, then silence.
    let a = audio(2, &[0, 16384, 0, -16384]);
    let mut m = Mixer::new(4);
    m.sync(&only(vec![play(&a, 0.0)]));
    let mut out = [9.0; 20];
    m.mix(&mut out, 2);
    let left: Vec<f32> = out.chunks(2).map(|f| f[0]).collect();
    assert_eq!(
        left,
        [0.0, 0.25, 0.5, 0.25, 0.0, -0.25, -0.5, -0.5, 0.0, 0.0]
    );
    assert!(out.chunks(2).all(|f| f[0] == f[1]));
    assert!(m.is_empty());
}

/// Studio centres a mono sound at 1/√2 on each side; a stereo one keeps
/// its level.
#[test]
fn mono_is_centred() {
    let mono = Arc::new(Audio {
        format: None,
        sample_rate: 4,
        channels: 1,
        samples: vec![16384; 4],
    });
    let mut m = Mixer::new(4);
    m.sync(&only(vec![play(&mono, 0.0)]));
    let mut out = [0.0; 4];
    m.mix(&mut out, 2);
    assert!(
        out.iter().all(|s| (s - 0.5 * MONO_LEVEL).abs() < 1e-6),
        "{out:?}"
    );
    let mut m = Mixer::new(4);
    m.sync(&only(vec![play(&audio(4, &[16384; 4]), 0.0)]));
    m.mix(&mut out, 2);
    assert_eq!(out, [0.5; 4]);
}

#[test]
fn follows_the_engine() {
    let a = audio(10, &[1000; 100]);
    let mut m = Mixer::new(10);
    m.sync(&only(vec![play(&a, 0.0)]));
    let mut out = [0.0; 10];
    m.mix(&mut out, 1);
    // The engine says 0.4 s where the mixer is at 1.0 (its frames were
    // slow): the voice keeps its own clock.
    m.sync(&only(vec![play(&a, 0.4)]));
    assert!((m.voices[&1].seconds() - 1.0).abs() < 1e-9);
    // Played again or seeked (a new generation): it starts there.
    m.sync(&only(vec![Play {
        generation: 1,
        ..play(&a, 5.1)
    }]));
    assert!((m.voices[&1].seconds() - 5.1).abs() < 1e-9);
    // Played to its end, it stays silent while the engine's clock catches
    // up, rather than playing the end again.
    let mut rest = [0.0; 60];
    m.mix(&mut rest, 1);
    assert!(m.is_empty());
    m.sync(&only(vec![Play {
        generation: 1,
        ..play(&a, 9.5)
    }]));
    m.mix(&mut out, 1);
    assert_eq!(out, [0.0; 10]);
    m.sync(&Mix::default());
    assert!(m.is_empty());
}

#[test]
fn loops() {
    let a = audio(4, &[8192, 0, 0, 0]);
    let mut m = Mixer::new(4);
    m.sync(&only(vec![Play {
        looped: true,
        ..play(&a, 0.0)
    }]));
    let mut out = [0.0; 9];
    m.mix(&mut out, 1);
    assert_eq!((out[0], out[4], out[8]), (0.25, 0.25, 0.25));
    assert_eq!(m.len(), 1);
}

/// Gains move to new values over one buffer, per side.
#[test]
fn ramps_and_pans() {
    let a = audio(4, &[16384; 40]);
    let mut m = Mixer::new(4);
    m.sync(&only(vec![play(&a, 0.0)]));
    m.sync(&only(vec![Play {
        gains: [0.0, 1.0],
        ..play(&a, 0.0)
    }]));
    let mut out = [0.0; 8];
    m.mix(&mut out, 2);
    assert_eq!(out, [0.5, 0.5, 0.375, 0.5, 0.25, 0.5, 0.125, 0.5]);
    m.mix(&mut out, 2);
    assert_eq!(out, [0.0, 0.5, 0.0, 0.5, 0.0, 0.5, 0.0, 0.5]);
}

/// A sound's effects run before its Volume (Studio's `PlaybackLoudness`
/// is taken after effects and before Volume): a distortion makes a quiet
/// sound as loud as a loud one, and Volume then scales the result.
#[test]
fn effects_before_volume() {
    let a = audio(4, &[328; 8]);
    let mut m = Mixer::new(4);
    m.sync(&only(vec![Play {
        gains: [0.5, 0.5],
        effects: vec![Effect::Distortion { level: 1.0 }],
        ..play(&a, 0.0)
    }]));
    let mut out = [0.0; 4];
    m.mix(&mut out, 1);
    assert!(out.iter().all(|s| (s - 0.5).abs() < 0.01), "{out:?}");
}

/// A group's Volume multiplies its sounds', and a group inside another is
/// multiplied by that one's too (creator-docs; nesting by Inference).
#[test]
fn groups_multiply() {
    let a = audio(4, &[16384; 40]);
    let mut m = Mixer::new(4);
    let mix = Mix {
        sounds: vec![Play {
            gains: [0.5, 0.5],
            group: Some(10),
            ..play(&a, 0.0)
        }],
        groups: vec![group(10, Some(20), 0.5), group(20, None, 2.0)],
        ambient: None,
        graph: Graph::default(),
    };
    m.sync(&mix);
    let mut out = [0.0; 4];
    m.mix(&mut out, 1);
    // 0.5 (the sample) × 0.5 (Volume) × 0.5 × 2 (the groups).
    assert_eq!(out, [0.25; 4]);
    // A sound whose group isn't listed goes straight out.
    m.sync(&only(vec![Play {
        gains: [0.5, 0.5],
        group: Some(99),
        ..play(&a, 0.0)
    }]));
    m.mix(&mut out, 1);
    assert_eq!(out, [0.25; 4]);
}

/// A group's effects run over the sum of its sounds, and a reverb's tail
/// goes on after the sound stops.
#[test]
fn group_effects_and_tails() {
    let a = audio(100, &[16384; 10]);
    let mut m = Mixer::new(100);
    let reverb = Effect::Reverb(Reverb {
        dry: -80.0,
        ..Reverb::default()
    });
    let mix = Mix {
        sounds: vec![Play {
            group: Some(10),
            ..play(&a, 0.0)
        }],
        groups: vec![Group {
            effects: vec![reverb],
            ..group(10, None, 1.0)
        }],
        ambient: None,
        graph: Graph::default(),
    };
    m.sync(&mix);
    let mut out = vec![0.0; 100];
    m.mix(&mut out, 1);
    // The sound has ended and the engine lists nothing any more, yet the
    // group's reverb still rings.
    m.sync(&Mix::default());
    m.mix(&mut out, 1);
    assert!(out.iter().any(|s| s.abs() > 1e-4));
    // Once its tail has passed, the bus is gone.
    for _ in 0..3 {
        m.mix(&mut out, 1);
    }
    m.sync(&Mix::default());
    assert!(m.buses.is_empty());
}

#[test]
fn master_volume_scales_what_is_heard() {
    let a = audio(4, &[16384; 4]);
    let mut m = Mixer::new(4);
    m.set_master_volume(0.25);
    m.sync(&only(vec![play(&a, 0.0)]));
    let mut out = [0.0; 2];
    m.mix(&mut out, 2);
    assert_eq!(out, [0.125, 0.125]);
    m.set_master_volume(3.0);
    m.mix(&mut out, 2);
    assert_eq!(out, [0.5, 0.5], "clamped to 1");
}
