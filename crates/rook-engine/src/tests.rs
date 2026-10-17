use super::*;

fn add_script(server: &Server, location: &str, name: &str, source: &str) {
    let parent = service(server, location);
    add_under(server, parent, "Script", name, source);
}

fn lines(server: &Server) -> Vec<String> {
    server.take_log().into_iter().map(|e| e.message).collect()
}

#[test]
fn scripts_run_in_server_locations_only() {
    let mut s = Server::new(&Options::default()).unwrap();
    add_script(
        &s,
        "Workspace",
        "Script",
        "print('ws', script:GetFullName())",
    );
    add_script(&s, "ServerStorage", "Stored", "print('should not run')");
    s.start();
    assert_eq!(lines(&s), ["ws Workspace.Script"]);
}

#[test]
fn heartbeat_fires_every_frame() {
    let mut s = Server::new(&Options::default()).unwrap();
    add_script(
        &s,
        "ServerScriptService",
        "Script",
        r#"
        local n = 0
        game:GetService("RunService").Heartbeat:Connect(function() n += 1 end)
        task.delay(0.5, function() print(n) end)
        "#,
    );
    s.start();
    s.run_for(1.0);
    assert_eq!(lines(&s), ["30"]);
}

fn add_under(
    server: &Server,
    parent: InstanceId,
    class: &str,
    name: &str,
    source: &str,
) -> InstanceId {
    let mut w = server.runtime.world_mut();
    let id = w.dom.create_by_name(class).unwrap();
    w.dom.set_name(id, name.into()).unwrap();
    if !source.is_empty() {
        w.dom
            .set_property_by_name(id, "Source", Value::String(source.into()))
            .unwrap();
    }
    w.dom.set_parent(id, Some(parent)).unwrap();
    id
}

fn service(server: &Server, name: &str) -> InstanceId {
    server.runtime.world_mut().dom.get_service(name).unwrap()
}

/// A script changing a part's mesh (its type or Scale, or adding or
/// removing one) changes how the part looks, so the part is redrawn.
#[test]
fn mesh_and_decal_changes_mark_their_part() {
    let mut s = Server::new(&Options::default()).unwrap();
    let ws = service(&s, "Workspace");
    let part = add_under(&s, ws, "Part", "Part", "");
    add_under(&s, part, "SpecialMesh", "Mesh", "");
    let other = add_under(&s, ws, "Part", "Other", "");
    s.start();
    s.take_render_changes();
    let changed = |s: &mut Server, source: &str| {
        s.run_extra_script(source).unwrap();
        let changes = s.take_render_changes();
        assert!(!changes.full, "{source}");
        changes.parts
    };
    let only_part = std::collections::HashSet::from([part]);
    for source in [
        "workspace.Part.Mesh.MeshType = Enum.MeshType.Sphere",
        "workspace.Part.Mesh.Scale = Vector3.new(1, 2, 1)",
        "Instance.new('CylinderMesh', workspace.Part)",
        "workspace.Part:FindFirstChildOfClass('CylinderMesh').Parent = nil",
        "workspace.Part.Mesh:Destroy()",
        "Instance.new('Decal', workspace.Part)",
        "workspace.Part.Decal.Transparency = 0.5",
        "Instance.new('Texture', workspace.Part).StudsPerTileU = 3",
        "workspace.Part.Decal:Destroy()",
    ] {
        assert_eq!(changed(&mut s, source), only_part, "{source}");
    }
    // Moving a mesh between parts redraws both; other children don't
    // count.
    let block = add_under(&s, other, "BlockMesh", "Block", "");
    s.take_render_changes();
    s.runtime
        .world_mut()
        .dom
        .set_parent(block, Some(part))
        .unwrap();
    assert_eq!(
        s.take_render_changes().parts,
        std::collections::HashSet::from([part, other])
    );
    assert!(changed(&mut s, "Instance.new('Folder', workspace.Part)").is_empty());
}

/// A 1 x 1 red PNG.
const PNG: [u8; 70] = [
    137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 1, 0, 0, 0, 1, 8, 6, 0,
    0, 0, 31, 21, 196, 137, 0, 0, 0, 13, 73, 68, 65, 84, 120, 156, 99, 248, 207, 192, 240, 31, 0,
    5, 0, 1, 255, 137, 153, 61, 29, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130,
];

/// Options whose content comes from a synthetic install holding
/// `textures/face.png` and `sounds/hit.wav`, with no network or cache.
fn content_options(name: &str) -> Options {
    let dir = std::env::temp_dir()
        .join("rook-engine-tests")
        .join(format!("{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("content/textures")).unwrap();
    std::fs::create_dir_all(dir.join("content/sounds")).unwrap();
    std::fs::write(dir.join("content/textures/face.png"), PNG).unwrap();
    std::fs::write(dir.join("content/sounds/hit.wav"), b"wav").unwrap();
    let mut config = rook_content::Config::empty();
    config.install = rook_content::Install::at(&dir, rook_content::InstallKind::Custom);
    Options {
        content: Some(Arc::new(rook_content::Resolver::new(config))),
        ..Options::default()
    }
}

/// Step until `done` holds (content resolves on other threads).
fn step_until(dm: &mut DataModel, mut done: impl FnMut(&[String]) -> bool) -> Vec<String> {
    let mut out = Vec::new();
    for _ in 0..500 {
        dm.step();
        out.extend(lines(dm));
        if done(&out) {
            break;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    out
}

/// PreloadAsync on a client, as observed in Studio: instances stand for
/// their (and their descendants') content, strings load as images,
/// asset id strings fail, empty strings are skipped, an image property
/// holding a sound fails, and the call returns after the last callback.
#[test]
fn preload_async_on_a_client() {
    let mut client = DataModel::new_client(&content_options("preload")).unwrap();
    client
        .run_extra_script(
            r#"
            local CP = game:GetService("ContentProvider")
            local model = Instance.new("Model")
            Instance.new("Decal", model).Texture = "rbxasset://textures/face.png"
            local sound = Instance.new("Decal")
            sound.Texture = "rbxasset://sounds/hit.wav"
            local results = {}
            task.spawn(function()
                CP:PreloadAsync({"rbxasset://textures/face.png", "rbxasset://sounds/hit.wav",
                    "", "rbxassetid://5", "abc", model, sound}, function(id, status)
                    table.insert(results, id .. " " .. status.Name)
                end)
                table.sort(results)
                print(table.concat(results, ", "))
                print("returned", CP.RequestQueueSize)
            end)
            print("waiting", CP.RequestQueueSize)
            "#,
        )
        .unwrap();
    let out = step_until(&mut client, |out| {
        out.iter().any(|l| l.starts_with("returned"))
    });
    assert_eq!(
        out,
        [
            "waiting 6",
            "abc Failure, rbxasset://sounds/hit.wav Failure, \
             rbxasset://sounds/hit.wav Failure, \
             rbxasset://textures/face.png Success, rbxasset://textures/face.png Success, \
             rbxassetid://5 Failure",
            "returned 0",
        ]
    );
}

/// On a server PreloadAsync never returns once there is anything to
/// fetch, and returns at once when there isn't (observed).
#[test]
fn preload_async_on_a_server() {
    let mut s = Server::new(&content_options("preload-server")).unwrap();
    s.run_extra_script(
        r#"
        local CP = game:GetService("ContentProvider")
        task.spawn(function() CP:PreloadAsync({"abc"}) print("never") end)
        CP:PreloadAsync({"", Instance.new("Folder")})
        print("nothing to fetch")
        for _, bad in {"abc", 5} do
            print(pcall(CP.PreloadAsync, CP, bad))
        end
        print(pcall(CP.PreloadAsync, CP, {true}))
        print(pcall(CP.PreloadAsync, CP, {}, 5))
        "#,
    )
    .unwrap();
    s.run_for(0.5);
    assert_eq!(
        lines(&s),
        [
            "nothing to fetch",
            "false Unable to cast string to Array",
            "false Unable to cast double to Array",
            "false Unable to cast bool to ContentId",
            "false Unable to cast double to Function",
        ]
    );
}

#[test]
fn session_join_runs_local_scripts_where_roblox_does() {
    let mut server = Server::new(&Options::default()).unwrap();
    add_script(
        &server,
        "ServerScriptService",
        "Main",
        r#"
        game.Players.PlayerAdded:Connect(function(p)
            print("server: joined", p.Name, p.UserId, #p.PlayerGui:GetChildren())
        end)
        task.wait(0.2)
        local part = Instance.new("Part")
        part.Name = "Late"
        part.Parent = workspace
        "#,
    );
    let ss = service(&server, "ServerStorage");
    add_under(&server, ss, "Folder", "Secret", "");
    let gui = service(&server, "StarterGui");
    let screen = add_under(&server, gui, "ScreenGui", "Hud", "");
    add_under(
        &server,
        screen,
        "LocalScript",
        "HudScript",
        r#"
        local rs = game:GetService("RunService")
        print("gui:", game.Players.LocalPlayer.Name, rs:IsClient(), rs:IsServer(), script:GetFullName())
        print("gui: secret visible?", game:GetService("ServerStorage"):FindFirstChild("Secret") ~= nil)
        print("gui: late part", workspace:WaitForChild("Late").ClassName)
        "#,
    );
    let sp = service(&server, "StarterPlayer");
    let sps = server
        .runtime
        .world()
        .dom
        .find_first_child(sp, "StarterPlayerScripts", false);
    let sps = sps.unwrap_or_else(|| {
        add_under(
            &server,
            sp,
            "StarterPlayerScripts",
            "StarterPlayerScripts",
            "",
        )
    });
    add_under(
        &server,
        sps,
        "LocalScript",
        "Control",
        "print('player scripts:', script.Parent.ClassName)",
    );
    let rf = service(&server, "ReplicatedFirst");
    add_under(
        &server,
        rf,
        "LocalScript",
        "Loading",
        "print('replicated first')",
    );
    let ws = service(&server, "Workspace");
    add_under(
        &server,
        ws,
        "LocalScript",
        "Nope",
        "print('workspace local script must not run')",
    );
    server.start();

    let mut session = Session::new(server, Options::default());
    let index = session.join("Builderman", 156).unwrap();
    session.run_for(1.0);

    let server_log: Vec<String> = session
        .server()
        .take_log()
        .into_iter()
        .map(|e| e.message)
        .collect();
    assert_eq!(server_log, ["server: joined Builderman 156 1"]);
    let mut client_log: Vec<String> = session
        .client(index)
        .unwrap()
        .take_log()
        .into_iter()
        .map(|e| e.message)
        .collect();
    client_log.sort();
    assert_eq!(
        client_log,
        [
            "gui: Builderman true false Players.Builderman.PlayerGui.Hud.HudScript",
            "gui: late part Part",
            "gui: secret visible? false",
            "player scripts: PlayerScripts",
            "replicated first",
        ]
    );
}

#[test]
fn remote_events_and_functions_cross_the_session() {
    let mut server = Server::new(&Options::default()).unwrap();
    let rs = service(&server, "ReplicatedStorage");
    add_under(&server, rs, "RemoteEvent", "Ping", "");
    add_under(&server, rs, "RemoteFunction", "Ask", "");
    let ws = service(&server, "Workspace");
    add_under(&server, ws, "Part", "Target", "");
    add_script(
        &server,
        "ServerScriptService",
        "Main",
        r#"
        local rs = game:GetService("ReplicatedStorage")
        rs.Ping.OnServerEvent:Connect(function(player, data, part, fn)
            print("server got", player.Name, data.a, data.list[2], part == workspace.Target, fn)
            rs.Ping:FireClient(player, "pong", workspace.Target)
        end)
        rs.Ask.OnServerInvoke = function(player, n)
            if n < 0 then error("negative!") end
            task.wait(0.1)
            return n * 2, player.Name
        end
        print(pcall(function() rs.Ping:FireServer() end))
        "#,
    );
    let sp = service(&server, "StarterPlayer");
    let sps = add_under(
        &server,
        sp,
        "StarterPlayerScripts",
        "StarterPlayerScripts",
        "",
    );
    add_under(
        &server,
        sps,
        "LocalScript",
        "Client",
        r#"
        local rs = game:GetService("ReplicatedStorage")
        rs.Ping.OnClientEvent:Connect(function(word, part)
            print("client got", word, part == workspace.Target)
        end)
        rs.Ping:FireServer({ a = 1, list = { 10, 20 } }, workspace.Target, function() end)
        print("invoke", rs.Ask:InvokeServer(21))
        print("invoke error", pcall(function() return rs.Ask:InvokeServer(-1) end))
        "#,
    );
    server.start();
    let mut session = Session::new(server, Options::default());
    let index = session.join("Player1", 1).unwrap();
    session.run_for(1.0);
    let server_log: Vec<String> = session
        .server()
        .take_log()
        .into_iter()
        .map(|e| e.message)
        .collect();
    let client_log: Vec<String> = session
        .client(index)
        .unwrap()
        .take_log()
        .into_iter()
        .map(|e| e.message)
        .collect();
    assert_eq!(
        server_log,
        [
            "false FireServer can only be called from the client",
            "server got Player1 1 20 true nil",
        ]
    );
    assert_eq!(
        client_log,
        [
            "client got pong true",
            "invoke 42 Player1",
            "invoke error false ServerScriptService.Main:8: negative!",
        ]
    );
}

#[test]
fn input_mouse_and_click_detectors() {
    let mut server = Server::new(&Options::default()).unwrap();
    let ws = service(&server, "Workspace");
    let button = add_under(&server, ws, "Part", "Button", "");
    {
        let mut w = server.runtime.world_mut();
        let cf = rook_types::CFrame::from_position(glam::Vec3::new(0.0, 0.0, -10.0));
        w.dom
            .set_property_by_name(button, "CFrame", Value::CFrame(cf))
            .unwrap();
        w.dom
            .set_property_by_name(
                button,
                "Size",
                Value::Vector3(rook_types::Vector3::new(4.0, 4.0, 1.0)),
            )
            .unwrap();
    }
    add_under(&server, button, "ClickDetector", "ClickDetector", "");
    add_script(
        &server,
        "ServerScriptService",
        "Main",
        "workspace.Button.ClickDetector.MouseClick:Connect(function(p) print('server click', p.Name) end)",
    );
    let sp = service(&server, "StarterPlayer");
    let sps = add_under(
        &server,
        sp,
        "StarterPlayerScripts",
        "StarterPlayerScripts",
        "",
    );
    add_under(
        &server,
        sps,
        "LocalScript",
        "Input",
        r#"
        local uis = game:GetService("UserInputService")
        local mouse = game.Players.LocalPlayer:GetMouse()
        uis.InputBegan:Connect(function(input, processed)
            print("began", input.UserInputType.Name, input.KeyCode.Name, processed)
        end)
        uis.InputEnded:Connect(function(input)
            print("ended", input.UserInputType.Name, input.KeyCode.Name, input.UserInputState.Name)
        end)
        mouse.KeyDown:Connect(function(key) print("keydown", key, uis:IsKeyDown(Enum.KeyCode.W)) end)
        mouse.Button1Down:Connect(function() print("button1", mouse.Target and mouse.Target.Name, math.round(mouse.Hit.Position.Z)) end)
        local cd = workspace:WaitForChild("Button"):WaitForChild("ClickDetector")
        cd.MouseHoverEnter:Connect(function(p) print("hover", p.Name) end)
        cd.MouseClick:Connect(function(p) print("client click", p.Name) end)
        "#,
    );
    server.start();
    let mut session = Session::new(server, Options::default());
    let index = session.join("Clicker", 7).unwrap();
    {
        let client = session.client_mut(index).unwrap();
        client.set_viewport(800, 600);
        let camera = client.current_camera().unwrap();
        client.set_property(
            camera,
            "CFrame",
            Value::CFrame(rook_types::CFrame::IDENTITY),
        );
    }
    session.run_for(0.1);
    session.input(
        index,
        InputEvent::Key {
            code: "W".into(),
            down: true,
        },
    );
    session.input(
        index,
        InputEvent::Key {
            code: "W".into(),
            down: false,
        },
    );
    session.input(index, InputEvent::MouseMove { x: 400.0, y: 300.0 });
    session.input(
        index,
        InputEvent::MouseButton {
            button: 1,
            down: true,
        },
    );
    session.input(
        index,
        InputEvent::MouseButton {
            button: 1,
            down: false,
        },
    );
    session.run_for(0.1);
    let client: Vec<String> = session
        .client(index)
        .unwrap()
        .take_log()
        .into_iter()
        .map(|e| e.message)
        .collect();
    let server_log: Vec<String> = session
        .server()
        .take_log()
        .into_iter()
        .map(|e| e.message)
        .collect();
    assert_eq!(
        client,
        [
            "began Keyboard W false",
            "keydown w true",
            "ended Keyboard W End",
            "hover Clicker",
            "began MouseButton1 None false",
            "button1 Button -10",
            "ended MouseButton1 None End",
            "client click Clicker",
        ]
    );
    assert_eq!(server_log, ["server click Clicker"]);
}

mod audio_api;
mod scripts;
mod sounds;
mod terrain_api;
mod terrain_look;
mod unions;
