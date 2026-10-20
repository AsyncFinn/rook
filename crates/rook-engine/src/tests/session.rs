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
