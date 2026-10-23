use super::*;

/// StyleSheets style what scripts change after the frame's restyle (a tag
/// added on Heartbeat, an object made after `task.wait`), though the
/// journal is trimmed at each frame's end.
#[test]
fn styles_reach_changes_made_later_in_the_frame() {
    let mut server = Server::new(&Options::default()).unwrap();
    let starter = service(&server, "StarterGui");
    let screen = add_under(&server, starter, "ScreenGui", "Hud", "");
    add_under(
        &server,
        screen,
        "LocalScript",
        "Style",
        r#"
        local gui = script.Parent
        local sheet = Instance.new("StyleSheet")
        sheet.Parent = gui
        local link = Instance.new("StyleLink")
        link.StyleSheet = sheet
        link.Parent = gui
        local rule = Instance.new("StyleRule")
        rule.Selector = ".dim"
        rule:SetProperties({ BackgroundTransparency = 0.5 })
        rule.Parent = sheet
        task.wait(0.1)
        local made = Instance.new("Frame")
        made:AddTag("dim")
        made.Parent = gui
        local tagged = Instance.new("Frame")
        tagged.Parent = gui
        game:GetService("RunService").Heartbeat:Once(function()
            tagged:AddTag("dim")
        end)
        task.wait(0.2)
        print(made.BackgroundTransparency, tagged.BackgroundTransparency)
        "#,
    );
    server.start();
    let mut session = Session::new(server, Options::default());
    let index = session.join("Styler", 1).unwrap();
    session.run_for(0.5);
    let client = session.client_mut(index).unwrap();
    assert_eq!(lines(client), ["0.5 0.5"]);
}

/// A window resize reaches scripts and GUI layout the frame it happens,
/// and a camera a script swaps in gets the window's size.
#[test]
fn resizing_the_window_lays_gui_out_again() {
    let mut server = Server::new(&Options::default()).unwrap();
    let starter = service(&server, "StarterGui");
    let screen = add_under(&server, starter, "ScreenGui", "Hud", "");
    add_under(
        &server,
        screen,
        "LocalScript",
        "Watch",
        r#"
        local camera = workspace.CurrentCamera
        camera:GetPropertyChangedSignal("ViewportSize"):Connect(function()
            print("viewport", camera.ViewportSize)
        end)
        local hud = script.Parent
        hud:GetPropertyChangedSignal("AbsoluteSize"):Connect(function()
            print("hud", hud.AbsoluteSize)
        end)
        print("ready", hud.AbsoluteSize)
        "#,
    );
    server.start();
    let mut session = Session::new(server, Options::default());
    let index = session.join("Resizer", 1).unwrap();
    session.client_mut(index).unwrap().set_viewport(800, 600);
    session.run_for(0.2);
    let client = session.client_mut(index).unwrap();
    assert_eq!(lines(client), ["ready 800, 542"]);

    client.set_viewport(1000, 700);
    session.step();
    let client = session.client_mut(index).unwrap();
    assert_eq!(lines(client), ["viewport 1000, 700", "hud 1000, 642"]);

    client
        .runtime()
        .run_source(
            "Swap",
            r#"
            local camera = Instance.new("Camera")
            camera.Parent = workspace
            workspace.CurrentCamera = camera
            "#,
            None,
        )
        .unwrap();
    session.step();
    let client = session.client_mut(index).unwrap();
    let camera = client.current_camera().unwrap();
    let world = client.runtime().world();
    assert_eq!(
        world.dom.vector2_property(camera, "ViewportSize"),
        Some(rook_types::Vector2::new(1000.0, 700.0))
    );
    let hud = world
        .dom
        .local_player()
        .and_then(|p| world.dom.find_first_child(p, "PlayerGui", false))
        .and_then(|g| world.dom.find_first_child(g, "Hud", false))
        .unwrap();
    assert_eq!(
        world.dom.vector2_property(hud, "AbsoluteSize"),
        Some(rook_types::Vector2::new(1000.0, 642.0))
    );
}
