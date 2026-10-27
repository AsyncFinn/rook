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
