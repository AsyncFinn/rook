//! The audio API's graph (`docs/behavior/audio.md`, step 5): what the
//! player hears of it, a client's default AudioListener, scheduled `Play`
//! and `Stop` calls, and AudioAnalyzers' measurements.
//!
//! Which wires connect and how loud a listener hears an emitter are
//! `rook_lua::audio_api`'s rules, so scripts and the mixer agree.

use rook_audio::SAMPLE_RATE;
use rook_audio::effects::{Effect, Reverb};
use rook_audio::graph::{Graph, Node, NodeKind, Runner, Source, Wire};

use super::*;

/// A client's own rendering of the graph, to measure its analyzers.
#[derive(Debug)]
pub(crate) struct Metering {
    runner: Runner<InstanceId>,
    /// Frames owed from earlier frames' fractions.
    carry: f64,
}

/// The effect an effect instance plays, with Studio's defaults; `None`
/// passes sound through as it is: a bypassed effect (creator-docs), and
/// AudioFilter, AudioFlanger, AudioTremolo, AudioLimiter, AudioGate and
/// AudioChannelSplitter, which Rook doesn't play yet (Approximation).
fn effect(dom: &Dom, id: InstanceId) -> Option<Effect> {
    if dom.bool_property(id, "Bypass").unwrap_or(false) {
        return None;
    }
    let f = |name: &str, default: f32| dom.float_property(id, name).unwrap_or(default);
    Some(match dom.class_name(id) {
        "AudioFader" => Effect::Fader {
            volume: f("Volume", 1.0),
        },
        "AudioEqualizer" => Effect::Equalizer {
            low: f("LowGain", 0.0),
            mid: f("MidGain", 0.0),
            high: f("HighGain", 0.0),
            range: dom
                .number_range_property(id, "MidRange")
                .map_or(rook_audio::effects::CROSSOVERS, |r| [r.min, r.max]),
        },
        // Confirmed one effect with ReverbSoundEffect at the same
        // settings; the properties it lacks (DecayRatio, the delays, the
        // low shelf, ReferenceFrequency) aren't played (Approximation).
        "AudioReverb" => Effect::Reverb(Reverb {
            decay: f("DecayTime", 1.5),
            density: f("Density", 1.0),
            diffusion: f("Diffusion", 1.0),
            dry: f("DryLevel", 0.0),
            wet: f("WetLevel", -6.0),
            high_cut: f("HighCutFrequency", 20_000.0),
        }),
        "AudioDistortion" => Effect::Distortion {
            level: f("Level", 0.5),
        },
        "AudioPitchShifter" => Effect::PitchShift {
            octave: f("Pitch", 1.25),
        },
        "AudioChorus" => Effect::Chorus {
            depth: f("Depth", 0.45),
            mix: f("Mix", 0.85),
            rate: f("Rate", 5.0),
        },
        "AudioEcho" => Effect::Echo {
            delay: f("DelayTime", 1.0),
            dry: f("DryLevel", 0.0),
            wet: f("WetLevel", 0.0),
            feedback: f("Feedback", 0.5),
        },
        "AudioCompressor" => Effect::Compressor {
            threshold: f("Threshold", -40.0),
            ratio: f("Ratio", 40.0),
            attack: f("Attack", 0.1),
            release: f("Release", 0.1),
            makeup: f("MakeupGain", 0.0),
        },
        _ => return None,
    })
}

impl DataModel {
    /// The audio API's graph as it is now, for the mixer: every player,
    /// effect, emitter, listener, output and analyzer in the DataModel,
    /// and the connected wires. An AudioDeviceOutput is heard when its
    /// `Player` is nil or this client's player (creator-docs).
    pub fn audio_graph(&self) -> Graph<InstanceId> {
        if self.sounds.graph_nodes.is_empty() {
            return Graph::default();
        }
        let w = self.runtime.world();
        let dom = &w.dom;
        let mut ids: Vec<InstanceId> = self.sounds.graph_nodes.iter().copied().collect();
        ids.sort();
        let local = dom
            .find_service("Players")
            .and_then(|p| dom.ref_property(p, "LocalPlayer"));
        let of = |class: &'static str| {
            ids.iter()
                .copied()
                .filter(move |&i| dom.class_name(i) == class)
        };
        let emitters: Vec<InstanceId> = of("AudioEmitter").collect();
        let wires: Vec<InstanceId> = of("Wire").collect();
        let mut nodes = Vec::new();
        for &id in &ids {
            let kind = match dom.class_name(id) {
                "Wire" => continue,
                "AudioPlayer" => NodeKind::Player(
                    self.sounds
                        .audio(id)
                        .filter(|_| dom.bool_property(id, "IsPlaying").unwrap_or(false))
                        .map(|audio| Source {
                            audio: audio.clone(),
                            position: dom
                                .number_property(id, "TimePosition")
                                .unwrap_or(0.0)
                                .max(0.0),
                            volume: dom.float_property(id, "Volume").unwrap_or(1.0),
                            speed: dom.float_property(id, "PlaybackSpeed").unwrap_or(1.0),
                            looped: dom.bool_property(id, "Looping").unwrap_or(false),
                            generation: self.sounds.generation(id),
                        }),
                ),
                "AudioEmitter" => NodeKind::Emitter,
                "AudioListener" => NodeKind::Listener(
                    emitters
                        .iter()
                        .filter_map(|&e| Some((e, rook_lua::audio_api::heard(&w, e, id)?)))
                        .collect(),
                ),
                "AudioDeviceOutput" => {
                    let player = dom.ref_property(id, "Player");
                    if player.is_none() || player == local {
                        NodeKind::Output
                    } else {
                        NodeKind::Effect(None)
                    }
                }
                "AudioAnalyzer" => NodeKind::Analyzer,
                _ => NodeKind::Effect(effect(dom, id)),
            };
            nodes.push(Node { key: id, kind });
        }
        let wires = rook_lua::audio_api::connections(dom, &wires)
            .into_iter()
            .map(|c| Wire {
                from: c.from,
                to: c.to,
                sidechain: c.sidechain,
            })
            .collect();
        Graph { nodes, wires }
    }

    /// Once a frame, after sounds move on: a client's scheduled `Play` and
    /// `Stop` calls that are due, its default listener, and its analyzers.
    pub(crate) fn update_audio(&mut self, dt: f64) {
        if self.role != Role::Client {
            return;
        }
        self.run_scheduled();
        if !self.sounds.default_listener {
            self.make_default_listener();
        }
        self.measure_analyzers(dt);
    }

    fn run_scheduled(&self) {
        let start = {
            let mut w = self.runtime.world_mut();
            if w.audio.scheduled.is_empty() {
                return;
            }
            let start = w.dom.journal().end();
            let now = w.scheduler.now();
            let (due, waiting) = std::mem::take(&mut w.audio.scheduled)
                .into_iter()
                .partition(|s| s.at <= now);
            w.audio.scheduled = waiting;
            for s in due {
                rook_dom::ignore_stale(rook_lua::audio_api::set_playing(&mut w, s.player, s.play));
            }
            start
        };
        self.runtime.dispatch_since(start);
    }

    /// A client's default listener, as Studio makes one: with
    /// `SoundService.DefaultListenerLocation` Camera, an AudioListener under
    /// the current camera, wired by a Wire inside it to an
    /// AudioDeviceOutput under SoundService (observed); with Character, the
    /// same under the player's character once it has one (creator-docs).
    /// Default makes one only in voice chat experiences (creator-docs), so
    /// Rook makes none, nor for None.
    fn make_default_listener(&mut self) {
        let camera = self.current_camera();
        let start = {
            let mut w = self.runtime.world_mut();
            let dom = &mut w.dom;
            let Some(service) = dom.find_service("SoundService") else {
                return;
            };
            let location = dom.enum_item_property(service, "DefaultListenerLocation");
            let parent = match location {
                Some("Camera") => camera,
                Some("Character") => dom
                    .find_service("Players")
                    .and_then(|p| dom.ref_property(p, "LocalPlayer"))
                    .and_then(|p| dom.ref_property(p, "Character")),
                _ => {
                    self.sounds.default_listener = true;
                    return;
                }
            };
            let Some(parent) = parent else {
                return;
            };
            let start = dom.journal().end();
            let (Ok(listener), Ok(output), Ok(wire)) = (
                dom.create_by_name("AudioListener"),
                dom.create_by_name("AudioDeviceOutput"),
                dom.create_by_name("Wire"),
            ) else {
                return;
            };
            // Fresh instances under ones just found: none of this can fail.
            let source = Value::Ref(Some(listener.to_ref()));
            let target = Value::Ref(Some(output.to_ref()));
            rook_dom::ignore_stale(dom.set_property_by_name(wire, "SourceInstance", source));
            rook_dom::ignore_stale(dom.set_property_by_name(wire, "TargetInstance", target));
            let _ = dom.set_parent(wire, Some(listener));
            let _ = dom.set_parent(listener, Some(parent));
            let _ = dom.set_parent(output, Some(service));
            self.sounds.default_listener = true;
            start
        };
        self.runtime.dispatch_since(start);
    }

    /// Render the graph for `dt` and show each analyzer what reached it:
    /// `RmsLevel` and `PeakLevel` (no `Changed`, creator-docs) and, with
    /// `SpectrumEnabled`, its spectrum (kept as it was while disabled,
    /// observed). Only clients measure: on a server both levels stay 0 and
    /// the spectrum empty (creator-docs).
    fn measure_analyzers(&mut self, dt: f64) {
        let analyzers: Vec<InstanceId> = {
            let w = self.runtime.world();
            let mut a: Vec<InstanceId> = self
                .sounds
                .graph_nodes
                .iter()
                .copied()
                .filter(|&i| w.dom.class_name(i) == "AudioAnalyzer")
                .collect();
            a.sort();
            a
        };
        if analyzers.is_empty() {
            self.sounds.meter = None;
            return;
        }
        let graph = self.audio_graph();
        let m = self.sounds.meter.get_or_insert_with(|| Metering {
            // Studio's analyzer spectra reach 24 kHz.
            runner: Runner::new(SAMPLE_RATE),
            carry: 0.0,
        });
        m.runner.sync(&graph);
        let owed = dt * f64::from(SAMPLE_RATE) + m.carry;
        let mut frames = owed.floor() as usize;
        m.carry = owed - frames as f64;
        let mut block = Vec::new();
        while frames > 0 {
            let n = frames.min(rook_audio::graph::meter::BLOCK);
            block.clear();
            block.resize(n, [0.0; 2]);
            m.runner.render(&mut block);
            frames -= n;
        }
        let mut w = self.runtime.world_mut();
        let w = &mut *w;
        for id in analyzers {
            let Some(meter) = m.runner.meter(id) else {
                continue;
            };
            let (rms, peak) = meter.levels();
            for (name, v) in [("RmsLevel", rms), ("PeakLevel", peak)] {
                if let Some(p) = w.dom.property_id(id, name) {
                    w.dom.load_property(id, p, Value::Float32(v));
                }
            }
            if w.dom.bool_property(id, "SpectrumEnabled").unwrap_or(false) {
                let bins = match w.dom.enum_item_property(id, "WindowSize") {
                    Some("Small") => 128,
                    Some("Large") => 2048,
                    _ => 512,
                };
                w.audio.spectra.insert(id, meter.spectrum(bins));
            }
        }
    }
}
