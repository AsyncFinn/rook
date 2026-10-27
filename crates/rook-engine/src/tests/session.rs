use super::*;

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

    let server_log = lines(session.server());
    // StarterGui is copied when the character spawns (creator-docs
    // StarterGui), after PlayerAdded.
    assert_eq!(server_log, ["server: joined Builderman 156 0"]);
    let mut client_log = lines(session.client(index).unwrap());
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
    add_local_script(
        &server,
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
    let server_log = lines(session.server());
    let client_log = lines(session.client(index).unwrap());
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
        // No character: reach is then measured from the camera.
        let players = w.dom.get_service("Players").unwrap();
        w.dom
            .set_property_by_name(players, "CharacterAutoLoads", Value::Bool(false))
            .unwrap();
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
    add_local_script(
        &server,
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
    let client = lines(session.client(index).unwrap());
    let server_log = lines(session.server());
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

#[test]
fn player_gui_from_the_server_reaches_only_its_player() {
    let mut server = Server::new(&Options::default()).unwrap();
    add_script(
        &server,
        "ServerScriptService",
        "Give",
        r#"
        local Players = game:GetService("Players")
        local function give(p, name)
            local gui = Instance.new("ScreenGui")
            gui.Name = name
            Instance.new("Frame", gui)
            gui.Parent = p.PlayerGui
        end
        Players.PlayerAdded:Connect(function(p) give(p, "Early") end)
        task.wait(0.3)
        for _, p in Players:GetPlayers() do give(p, "Late") end
        task.wait(0.3)
        Players.A:LoadCharacter()
        "#,
    );
    server.start();
    let mut session = Session::new(server, Options::default());
    let a = session.join("A", 1).unwrap();
    let b = session.join("B", 2).unwrap();
    // Each client's view of every player's PlayerGui.
    let guis = |session: &Session, index| {
        let w = session.client(index).unwrap().runtime.world();
        let dom = &w.dom;
        let players = dom.find_service("Players").unwrap();
        let mut seen = Vec::new();
        for &p in dom.children(players) {
            if let Some(gui) = dom.find_first_child(p, "PlayerGui", false) {
                for &g in dom.children(gui) {
                    seen.push(format!("{}.{}", dom.name(p), dom.name(g)));
                }
            }
        }
        seen
    };
    session.run_for(0.5);
    // The first spawn keeps what PlayerAdded gave.
    assert_eq!(guis(&session, a), ["A.Early", "A.Late"]);
    assert_eq!(guis(&session, b), ["B.Early", "B.Late"]);
    session.run_for(0.3);
    // A later spawn resets them.
    assert!(guis(&session, a).is_empty());
    assert_eq!(guis(&session, b), ["B.Early", "B.Late"]);
}

/// A ClickDetector's reach is measured from the character's root part,
/// whatever it is called: here a body with a Torso and no
/// HumanoidRootPart stands beside a detector the camera is too far from.
#[test]
fn click_detectors_reach_from_a_torso_root() {
    let mut server = Server::new(&Options::default()).unwrap();
    add_script(
        &server,
        "ServerScriptService",
        "Main",
        r#"
        game.Players.CharacterAutoLoads = false
        local button = Instance.new("Part")
        button.Name = "Button"
        button.Anchored = true
        button.Size = Vector3.new(4, 4, 1)
        button.CFrame = CFrame.new(0, 0, -10)
        local cd = Instance.new("ClickDetector")
        cd.MaxActivationDistance = 5
        cd.Parent = button
        button.Parent = workspace
        game.Players.PlayerAdded:Connect(function(p)
            local body = Instance.new("Model")
            body.Name = p.Name
            local torso = Instance.new("Part")
            torso.Name = "Torso"
            torso.Anchored = true
            torso.CanQuery = false
            torso.CFrame = CFrame.new(3, 0, -7)
            torso.Parent = body
            Instance.new("Humanoid").Parent = body
            body.Parent = workspace
            p.Character = body
        end)
        "#,
    );
    add_local_script(
        &server,
        "Input",
        r#"
        local cd = workspace:WaitForChild("Button"):WaitForChild("ClickDetector")
        cd.MouseHoverEnter:Connect(function(p) print("hover", p.Name) end)
        "#,
    );
    server.start();
    let mut session = Session::new(server, Options::default());
    let index = session.join("Clicker", 7).unwrap();
    session.run_for(0.2);
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
    session.input(index, InputEvent::MouseMove { x: 400.0, y: 300.0 });
    session.run_for(0.1);
    assert_eq!(lines(session.client(index).unwrap()), ["hover Clicker"]);
}

/// The mouse looks under the cursor again only when the camera, the
/// cursor or the world changed, and Mouse fires no Changed for what it
/// writes (measured in Studio: none while the cursor and camera moved).
#[test]
fn the_mouse_looks_again_only_when_something_changed() {
    let mut server = Server::new(&Options::default()).unwrap();
    add_script(
        &server,
        "ServerScriptService",
        "Main",
        r#"
        game.Players.CharacterAutoLoads = false
        local wall = Instance.new("Part")
        wall.Name = "Wall"
        wall.Anchored = true
        wall.Size = Vector3.new(8, 8, 1)
        wall.CFrame = CFrame.new(0, 0, -10)
        wall.Parent = workspace
        task.wait(0.5)
        wall.CFrame = CFrame.new(0, 0, -20)
        "#,
    );
    add_local_script(
        &server,
        "Watch",
        r#"
        local mouse = game.Players.LocalPlayer:GetMouse()
        mouse.Changed:Connect(function(name) print("changed", name) end)
        "#,
    );
    server.start();
    let mut session = Session::new(server, Options::default());
    let index = session.join("Looker", 7).unwrap();
    session.run_for(0.1);
    {
        let client = session.client_mut(index).unwrap();
        client.set_viewport(800, 600);
        let camera = client.current_camera().unwrap();
        let cf = rook_types::CFrame::IDENTITY;
        client.set_property(camera, "CFrame", Value::CFrame(cf));
    }
    session.input(index, InputEvent::MouseMove { x: 400.0, y: 300.0 });
    session.run_for(0.05);
    let looks = |session: &Session| session.client(index).unwrap().input.looks;
    let target_z = |session: &Session| {
        let client = session.client(index).unwrap();
        let w = client.runtime().world();
        let mouse = w.input.mouse_object.unwrap();
        w.dom
            .cframe_property(mouse, "Hit")
            .unwrap()
            .position
            .z
            .round()
    };
    let before = looks(&session);
    session.run_for(0.2);
    assert_eq!(looks(&session), before, "idle frames looked again");
    assert_eq!(target_z(&session), -10.0);
    session.run_for(0.4);
    assert_eq!(target_z(&session), -20.0);
    assert!(lines(session.client(index).unwrap()).is_empty());
}
