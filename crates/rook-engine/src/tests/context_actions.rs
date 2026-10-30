use super::*;

/// ContextActionService: the highest priority binding hears a key first,
/// then the newest among equals; Pass hands it on, anything else keeps
/// it. Unbinding uncovers the binding beneath.
#[test]
fn bindings_take_keys_by_priority_then_newest() {
    let mut server = Server::new(&Options::default()).unwrap();
    let starter = service(&server, "StarterGui");
    let screen = add_under(&server, starter, "ScreenGui", "Hud", "");
    add_under(
        &server,
        screen,
        "LocalScript",
        "Bind",
        r#"
        local cas = game:GetService("ContextActionService")
        local function say(name, state)
            print(name, state.Name)
        end
        cas:BindActionAtPriority("high", function(name, state)
            say(name, state)
            return Enum.ContextActionResult.Pass
        end, false, 3000, Enum.KeyCode.E)
        cas:BindAction("old", say, false, Enum.KeyCode.E)
        cas:BindAction("new", say, false, "e")
        task.wait(0.2)
        cas:UnbindAction("new")
        "#,
    );
    server.start();
    let mut session = Session::new(server, Options::default());
    let index = session.join("Binder", 1).unwrap();
    session.run_for(0.1);
    let press = |session: &mut Session| {
        for down in [true, false] {
            session.input(
                index,
                InputEvent::Key {
                    code: "E".into(),
                    down,
                },
            );
        }
    };
    press(&mut session);
    session.run_for(0.3);
    press(&mut session);
    session.run_for(0.1);
    assert_eq!(
        lines(session.client(index).unwrap()),
        [
            "high Begin",
            "new Begin",
            "high End",
            "new End",
            "high Begin",
            "old Begin",
            "high End",
            "old End",
        ]
    );
}

/// A script locking the mouse holds it, and the held mouse's movement
/// reaches InputChanged as a Delta (what the default camera turns by).
#[test]
fn a_locked_mouse_moves_by_deltas() {
    let mut server = Server::new(&Options::default()).unwrap();
    let starter = service(&server, "StarterGui");
    let screen = add_under(&server, starter, "ScreenGui", "Hud", "");
    add_under(
        &server,
        screen,
        "LocalScript",
        "Lock",
        r#"
        local uis = game:GetService("UserInputService")
        uis.MouseBehavior = Enum.MouseBehavior.LockCenter
        uis.InputChanged:Connect(function(input)
            print(input.UserInputType.Name, input.Delta.X, input.Delta.Y)
        end)
        "#,
    );
    server.start();
    let mut session = Session::new(server, Options::default());
    let index = session.join("Looker", 1).unwrap();
    session.run_for(0.1);
    assert_eq!(
        session.client(index).unwrap().mouse_lock(),
        crate::MouseLock::Center
    );
    session.input(index, InputEvent::MouseMotion { dx: 3.0, dy: -2.0 });
    session.run_for(0.05);
    assert_eq!(
        lines(session.client(index).unwrap()),
        ["MouseMovement 3 -2"]
    );
}

/// A mouse locked to the center is at the center from the frame the lock
/// comes, before it moves: the engine holds it there, not a window's
/// warp, so a host that only reports its cursor (or none) still gives
/// scripts the center.
#[test]
fn a_mouse_locked_to_the_center_is_there_at_once() {
    let mut server = Server::new(&Options::default()).unwrap();
    let starter = service(&server, "StarterGui");
    let screen = add_under(&server, starter, "ScreenGui", "Hud", "");
    add_under(
        &server,
        screen,
        "LocalScript",
        "Lock",
        r#"
        local uis = game:GetService("UserInputService")
        task.wait(0.1)
        uis.MouseBehavior = Enum.MouseBehavior.LockCenter
        task.wait(0.05)
        print(uis:GetMouseLocation())
        "#,
    );
    server.start();
    let mut session = Session::new(server, Options::default());
    let index = session.join("Looker", 1).unwrap();
    session.client_mut(index).unwrap().set_viewport(800, 600);
    session.input(index, InputEvent::MouseMove { x: 10.0, y: 20.0 });
    session.run_for(0.2);
    let client = session.client(index).unwrap();
    assert_eq!(client.mouse_position(), (400.0, 300.0));
    assert_eq!(lines(client), ["400, 300"]);
}

/// A shown Modal button frees a locked mouse; hiding it, or turning Modal
/// off, locks it again. Asking costs nothing for the rest of the GUI: a
/// thousand asks over 5,000 other objects stay well under a frame.
#[test]
fn a_shown_modal_button_frees_the_mouse() {
    let mut server = Server::new(&Options::default()).unwrap();
    let starter = service(&server, "StarterGui");
    let screen = add_under(&server, starter, "ScreenGui", "Hud", "");
    add_under(
        &server,
        screen,
        "LocalScript",
        "Lock",
        r#"
        game:GetService("UserInputService").MouseBehavior = Enum.MouseBehavior.LockCenter
        local hud = script.Parent
        for i = 1, 5000 do
            Instance.new("Frame", hud)
        end
        local menu = Instance.new("Frame")
        menu.Name = "Menu"
        local button = Instance.new("TextButton")
        button.Name = "Close"
        button.Modal = true
        button.Parent = menu
        menu.Parent = hud
        "#,
    );
    server.start();
    let mut session = Session::new(server, Options::default());
    let index = session.join("Looker", 1).unwrap();
    session.run_for(0.1);
    let lock = |session: &Session| session.client(index).unwrap().mouse_lock();
    assert_eq!(lock(&session), crate::MouseLock::Free);

    let client = session.client(index).unwrap();
    let start = std::time::Instant::now();
    for _ in 0..1000 {
        assert_eq!(client.mouse_lock(), crate::MouseLock::Free);
    }
    let took = start.elapsed();
    assert!(took.as_millis() < 100, "a thousand asks took {took:?}");

    let (menu, close) = {
        let w = client.runtime().world();
        let dom = &w.dom;
        let gui = dom.find_first_child_of_class(dom.local_player().unwrap(), "PlayerGui");
        let hud = dom.find_first_child(gui.unwrap(), "Hud", false).unwrap();
        let menu = dom.find_first_child(hud, "Menu", false).unwrap();
        (menu, dom.find_first_child(menu, "Close", false).unwrap())
    };
    let client = session.client_mut(index).unwrap();
    client.set_property(menu, "Visible", Value::Bool(false));
    session.run_for(0.05);
    assert_eq!(lock(&session), crate::MouseLock::Center);
    let client = session.client_mut(index).unwrap();
    client.set_property(menu, "Visible", Value::Bool(true));
    client.set_property(close, "Modal", Value::Bool(false));
    session.run_for(0.05);
    assert_eq!(lock(&session), crate::MouseLock::Center);
    let client = session.client_mut(index).unwrap();
    client.set_property(close, "Modal", Value::Bool(true));
    session.run_for(0.05);
    assert_eq!(lock(&session), crate::MouseLock::Free);
}
