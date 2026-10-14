//! Sounds across a play session: what the client (the DataModel a player
//! window shows and hears) loads and plays.

use super::*;

/// Options whose install holds a real 1 s WAV at `sounds/tone.wav`.
fn tone_options(name: &str) -> Options {
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
