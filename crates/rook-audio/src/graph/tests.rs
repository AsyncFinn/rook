use super::*;

fn audio(samples: Vec<i16>) -> Arc<Audio> {
    Arc::new(Audio {
        format: None,
        sample_rate: 100,
        channels: 1,
        samples,
    })
}

fn node(key: u32, kind: NodeKind<u32>) -> Node<u32> {
    Node { key, kind }
}

fn wire(from: u32, to: u32) -> Wire<u32> {
    Wire {
        from,
        to,
        sidechain: false,
    }
}

fn source(volume: f32) -> Source {
    Source {
        audio: audio(vec![16384; 1000]),
        position: 0.0,
        volume,
        speed: 1.0,
        looped: false,
        generation: 1,
    }
}

fn render(runner: &mut Runner<u32>, frames: usize) -> Vec<Frame> {
    let mut out = vec![[0.0; 2]; frames];
    runner.render(&mut out);
    out
}

/// A player through a fader into an output and an analyzer: heard at the
/// player's volume times the fader's, and measured the same.
#[test]
fn player_effect_output_and_analyzer() {
    let mut r = Runner::new(100);
    r.sync(&Graph {
        nodes: vec![
            node(1, NodeKind::Player(Some(source(0.5)))),
            node(2, NodeKind::Effect(Some(Effect::Fader { volume: 2.0 }))),
            node(3, NodeKind::Output),
            node(4, NodeKind::Analyzer),
        ],
        wires: vec![wire(1, 2), wire(2, 3), wire(2, 4), wire(2, 4)],
    });
    let out = render(&mut r, 10);
    assert!(
        out.iter()
            .all(|f| (f[0] - 0.5).abs() < 1e-6 && f[0] == f[1])
    );
    let (_, peak) = r.meter(4).unwrap().levels();
    assert!((peak - 0.5).abs() < 1e-6, "a doubled wire counts once");
}

/// Two wires into one input add up; a node with no wire is silent; a
/// bypassed effect passes its input as it is.
#[test]
fn inputs_add_up() {
    let mut r = Runner::new(100);
    r.sync(&Graph {
        nodes: vec![
            node(1, NodeKind::Player(Some(source(0.25)))),
            node(2, NodeKind::Effect(None)),
            node(3, NodeKind::Effect(None)),
            node(4, NodeKind::Output),
            node(5, NodeKind::Player(None)),
        ],
        wires: vec![wire(1, 2), wire(1, 3), wire(2, 4), wire(3, 4), wire(5, 4)],
    });
    let out = render(&mut r, 5);
    assert!(out.iter().all(|f| (f[0] - 0.25).abs() < 1e-6));
}

/// A listener hears each emitter with that emitter's gains, and nothing
/// reaches an output except through a wire.
#[test]
fn listeners_hear_emitters() {
    let mut r = Runner::new(100);
    r.sync(&Graph {
        nodes: vec![
            node(5, NodeKind::Listener(vec![(2, [0.0, 0.4])])),
            node(1, NodeKind::Player(Some(source(1.0)))),
            node(2, NodeKind::Emitter),
            node(3, NodeKind::Output),
        ],
        wires: vec![wire(1, 2), wire(5, 3)],
    });
    let out = render(&mut r, 4);
    assert!(out.iter().all(|f| f[0] == 0.0 && (f[1] - 0.2).abs() < 1e-6));
}

/// A compressor's sidechain decides how much it turns its input down.
#[test]
fn sidechain_keys_the_compressor() {
    let compressor = Effect::Compressor {
        threshold: -40.0,
        ratio: 50.0,
        attack: 0.0001,
        release: 0.01,
        makeup: 0.0,
    };
    let run = |keyed: bool| {
        let mut r = Runner::new(1000);
        let mut wires = vec![wire(1, 3), wire(3, 4)];
        if keyed {
            wires.push(Wire {
                from: 2,
                to: 3,
                sidechain: true,
            });
        }
        r.sync(&Graph {
            nodes: vec![
                node(1, NodeKind::Player(Some(source(0.001)))),
                node(2, NodeKind::Player(Some(source(1.0)))),
                node(3, NodeKind::Effect(Some(compressor))),
                node(4, NodeKind::Output),
            ],
            wires,
        });
        render(&mut r, 200)[199][0]
    };
    // Quiet input under the threshold passes; a loud key turns it down.
    assert!((run(false) - 0.0005).abs() < 1e-5);
    assert!(run(true) < 0.0001);
}

#[test]
fn order_puts_sources_first() {
    assert_eq!(order(3, &[(2, 1), (1, 0)]), [2, 1, 0]);
    // A loop (a listener hearing an emitter it feeds) goes last.
    assert_eq!(order(3, &[(1, 2), (2, 1)]), [0, 1, 2]);
}
