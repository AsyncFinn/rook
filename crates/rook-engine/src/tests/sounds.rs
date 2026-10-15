//! Sounds across a play session: what the client (the DataModel a player
//! window shows and hears) loads and plays.

use super::*;

/// Options whose install holds a real 1 s WAV at `sounds/tone.wav`.
pub(super) fn tone_options(name: &str) -> Options {
    let dir = std::env::temp_dir()
        .join("rook-engine-tests")
        .join(format!("{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("content/sounds")).unwrap();
    let (rate, frames) = (8000u32, 8000u32);
    let mut wav = Vec::new();
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36 + frames * 2).to_le_bytes());
    wav.extend_from_slice(b"WAVEfmt ");
    wav.extend_from_slice(&16u32.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&rate.to_le_bytes());
    wav.extend_from_slice(&(rate * 2).to_le_bytes());
    wav.extend_from_slice(&2u16.to_le_bytes());
    wav.extend_from_slice(&16u16.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&(frames * 2).to_le_bytes());
    for i in 0..frames {
        wav.extend_from_slice(&((i % 40) as i16 * 500).to_le_bytes());
    }
    std::fs::write(dir.join("content/sounds/tone.wav"), wav).unwrap();
    let mut config = rook_content::Config::empty();
    config.install = rook_content::Install::at(&dir, rook_content::InstallKind::Custom);
    Options {
        content: Some(Arc::new(rook_content::Resolver::new(config))),
        ..Options::default()
    }
}

/// A sound the place starts with, and one a server script plays later,
/// both reach the client loaded and playing.
#[test]
fn client_hears_server_sounds() {
    let options = tone_options("client-sounds");
    let mut server = DataModel::new(&options).unwrap();
    add_script(
        &server,
        "ServerScriptService",
        "Play",
        "local function new(name) local s = Instance.new('Sound') s.Name = name \
         s.SoundId = 'rbxasset://sounds/tone.wav' s.Looped = true s.Parent = workspace return s end \
         new('AtStart'):Play() task.wait(0.5) new('Later'):Play()",
    );
    server.start();
    let mut session = Session::new(server, options);
    let index = session.join("Player1", 1).unwrap();
    session.run_for(1.0);
    let server_playing = session.server().playing_sounds();
    let client = session.client(index).unwrap();
    let client_playing = client.playing_sounds();
    // Playing and TimePosition are NotReplicated in the API dump, yet a
    // server's sounds are heard on clients.
    assert_eq!(server_playing.len(), 2, "server");
    assert_eq!(client_playing.len(), 2, "client");
}

/// Every Sound property the client plays by reaches it, however the API
/// dump marks it.
#[test]
fn sound_properties_replicate() {
    let options = tone_options("sound-props");
    let mut server = DataModel::new(&options).unwrap();
    add_script(
        &server,
        "ServerScriptService",
        "Set",
        "local s = Instance.new('Sound') s.SoundId = 'rbxasset://sounds/tone.wav' \
         s.PlaybackSpeed = 0.5 s.Volume = 2 s.Looped = true s.RollOffMode = Enum.RollOffMode.Linear \
         s.RollOffMinDistance = 3 s.RollOffMaxDistance = 40 s.Parent = workspace s:Play() \
         task.wait(0.5) s.Pitch = 0.75",
    );
    server.start();
    let mut session = Session::new(server, options);
    let index = session.join("Player1", 1).unwrap();
    session.run_for(0.3);
    let client = session.client(index).unwrap();
    let sound = client.playing_sounds();
    assert_eq!(sound.len(), 1);
    assert_eq!(
        (sound[0].speed, sound[0].volume, sound[0].looped),
        (0.5, 2.0, true)
    );
    session.run_for(0.5);
    let sound = session.client(index).unwrap().playing_sounds();
    assert_eq!(sound[0].speed, 0.75, "Pitch set later");
}

/// What a client hears a sound through: its enabled effects directly
/// under it, in priority order; its SoundGroup and the groups that one is
/// in; `AmbientReverb`. `PlaybackLoudness` is taken after the sound's
/// effects, and `PlayLocalSound` plays a sound once without it playing.
#[test]
fn effects_groups_and_local_sounds() {
    use rook_audio::Effect;
    let options = tone_options("sound-effects");
    let mut server = DataModel::new(&options).unwrap();
    let first = service(&server, "ReplicatedFirst");
    add_under(
        &server,
        first,
        "LocalScript",
        "Sounds",
        r#"
        local SS = game:GetService('SoundService')
        local outer = Instance.new('SoundGroup') outer.Name = 'Outer' outer.Volume = 2 outer.Parent = SS
        local inner = Instance.new('SoundGroup') inner.Name = 'Inner' inner.Volume = 0.25 inner.Parent = outer
        Instance.new('ReverbSoundEffect').Parent = inner
        local function new(name)
            local s = Instance.new('Sound') s.Name = name s.SoundId = 'rbxasset://sounds/tone.wav'
            s.Looped = true s.Parent = SS return s
        end
        local a = new('A') a.SoundGroup = inner
        Instance.new('DistortionSoundEffect').Parent = a
        Instance.new('EqualizerSoundEffect').Parent = a
        local off = Instance.new('PitchShiftSoundEffect') off.Enabled = false off.Parent = a
        local folder = Instance.new('Folder') folder.Parent = a
        Instance.new('EqualizerSoundEffect').Parent = folder
        a:Play()
        local quiet = new('Quiet')
        local cut = Instance.new('EqualizerSoundEffect')
        cut.LowGain = -80 cut.MidGain = -80 cut.HighGain = -80 cut.Parent = quiet
        quiet:Play()
        local plain = new('Plain') plain:Play()
        SS.AmbientReverb = Enum.ReverbType.Cave
        task.wait(0.3)
        print('loudness', plain.PlaybackLoudness > 100, quiet.PlaybackLoudness < 1)
        local once = new('Once') once.Looped = false
        SS:PlayLocalSound(once)
        task.wait(0.1)
        print('once', once.Playing, once.TimePosition)
        "#,
    );
    server.start();
    let mut session = Session::new(server, options);
    let index = session.join("Player1", 1).unwrap();
    session.run_for(0.6);
    let client = session.client(index).unwrap();
    let log: Vec<String> = client.take_log().into_iter().map(|e| e.message).collect();
    assert!(log.contains(&"loudness true true".to_owned()), "{log:?}");
    assert!(log.contains(&"once false 0".to_owned()), "{log:?}");

    let dom = &client.runtime.world().dom;
    let ss = dom.find_service("SoundService").unwrap();
    let named = |parent, name| dom.find_first_child(parent, name, false).unwrap();
    let (a, once) = (named(ss, "A"), named(ss, "Once"));
    let outer = named(ss, "Outer");
    let inner = named(outer, "Inner");
    let playing = client.playing_sounds();
    let sound = playing.iter().find(|s| s.id == a).unwrap();
    assert_eq!(
        sound.effects,
        [
            Effect::Equalizer {
                low: -20.0,
                mid: -10.0,
                high: 0.0,
                range: rook_audio::effects::CROSSOVERS,
            },
            Effect::Distortion { level: 0.75 }
        ]
    );
    assert_eq!(sound.group, Some(inner));
    assert!(
        playing.iter().any(|s| s.id == once),
        "the local sound plays once"
    );
    let groups = client.sound_groups(&playing);
    let summary: Vec<_> = groups
        .iter()
        .map(|g| (g.key, g.parent, g.volume, g.effects.len()))
        .collect();
    assert_eq!(
        summary,
        [(inner, Some(outer), 0.25, 1), (outer, None, 2.0, 0)]
    );
    let ambient = client.sound_listener().ambient_reverb.unwrap();
    assert_eq!(ambient.decay, 2.9);
}
