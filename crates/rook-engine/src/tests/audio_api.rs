//! The audio API on a client: its default listener, scheduled `Play`,
//! analyzers measuring what reaches them, and listeners hearing emitters
//! by distance.

use super::sounds::tone_options;
use super::*;

#[test]
fn client_graph_listeners_and_analyzers() {
    let options = tone_options("audio-api");
    let mut server = DataModel::new(&options).unwrap();
    {
        let mut w = server.runtime.world_mut();
        let ss = w.dom.find_service("SoundService").unwrap();
        // ListenerLocation Camera, as Studio's Baseplate has it.
        w.dom
            .set_property_by_name(ss, "DefaultListenerLocation", Value::Enum(3))
            .unwrap();
    }
    let first = service(&server, "ReplicatedFirst");
    add_under(
        &server,
        first,
        "LocalScript",
        "Graph",
        r#"
        local SS = game:GetService('SoundService')
        local function wire(a, b)
            local w = Instance.new('Wire') w.SourceInstance = a w.TargetInstance = b w.Parent = workspace
        end
        local function part(z)
            local p = Instance.new('Part') p.Anchored = true p.CFrame = CFrame.new(0, 500, z)
            p.Parent = workspace return p
        end
        local p = Instance.new('AudioPlayer') p.Asset = 'rbxasset://sounds/tone.wav'
        p.Looping = true p.Volume = 0.5 p.Parent = workspace
        local direct = Instance.new('AudioAnalyzer') direct.Parent = workspace
        wire(p, direct)
        local em = Instance.new('AudioEmitter') em.AudioInteractionGroup = 'test' em.Parent = part(-20)
        local li = Instance.new('AudioListener') li.AudioInteractionGroup = 'test' li.Parent = part(0)
        local heard = Instance.new('AudioAnalyzer') heard.Parent = workspace
        wire(p, em) wire(li, heard)
        while not p.IsReady do task.wait() end
        local id = p:Play(SS:GetMixerTime() + 0.2)
        print('scheduled', typeof(id), p.IsPlaying)
        task.wait(0.4)
        print('playing', p.IsPlaying)
        task.wait(0.3)
        print('direct', direct.PeakLevel > 0.29 and direct.PeakLevel < 0.3, direct.RmsLevel > 0.1)
        print('heard', math.floor(heard.PeakLevel / direct.PeakLevel * 1000 + 0.5) / 1000,
            em:GetAudibilityFor(li), #direct:GetSpectrum())
        "#,
    );
    server.start();
    let mut session = Session::new(server, options);
    let index = session.join("Player1", 1).unwrap();
    session.run_for(1.2);
    let client = session.client(index).unwrap();
    let log = lines(client);
    for line in [
        "scheduled number false",
        "playing true",
        "direct true true",
        "heard 0.2 0.20000000298023224 512",
    ] {
        assert!(log.contains(&line.to_owned()), "{line}: {log:?}");
    }

    // The default listener: under the camera, wired to an output under
    // SoundService, which the mixer hears.
    let camera = client.current_camera().unwrap();
    let dom = &client.runtime.world().dom;
    assert!(
        dom.find_first_child(camera, "AudioListener", false)
            .is_some()
    );
    let ss = dom.find_service("SoundService").unwrap();
    let output = dom
        .find_first_child(ss, "AudioDeviceOutput", false)
        .unwrap();
    let graph = client.audio_graph();
    assert!(graph.has_output());
    assert!(graph.wires.iter().any(|w| w.to == output));
}
