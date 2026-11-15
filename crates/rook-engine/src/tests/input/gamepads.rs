use crate::tests::*;
use crate::{GamepadAxis, GamepadKind};
use rook_types::GamepadKey;

/// A client with a LocalScript that logs what `source` prints.
fn client_with(source: &str) -> (Session, usize) {
    let mut server = Server::new(&Options::default()).unwrap();
    let starter = service(&server, "StarterGui");
    let screen = add_under(&server, starter, "ScreenGui", "Hud", "");
    add_under(&server, screen, "LocalScript", "Log", source);
    server.start();
    let mut session = Session::new(server, Options::default());
    let index = session.join("Player", 1).unwrap();
    session.run_for(0.1);
    (session, index)
}

fn connect(session: &mut Session, index: usize, pad: usize) {
    session.input(
        index,
        InputEvent::GamepadConnected {
            pad,
            kind: GamepadKind::PlayStation5,
            supported: GamepadKey::ITEMS.iter().map(|&(_, k)| k).collect(),
        },
    );
}

/// Connecting turns GamepadEnabled and PreferredInput before
/// GamepadConnected; disconnecting ends an off-center stick first, then
/// turns them back once no pad is left (measured order).
#[test]
fn pads_connect_and_disconnect_in_studio_order() {
    let (mut session, index) = client_with(
        r#"
        local uis = game:GetService("UserInputService")
        uis:GetPropertyChangedSignal("GamepadEnabled"):Connect(function()
            print("GamepadEnabled", uis.GamepadEnabled)
        end)
        uis:GetPropertyChangedSignal("PreferredInput"):Connect(function()
            print("PreferredInput", uis.PreferredInput.Name)
        end)
        uis.LastInputTypeChanged:Connect(function(t) print("LastInputType", t.Name) end)
        uis.GamepadConnected:Connect(function(t) print("Connected", t.Name) end)
        uis.GamepadDisconnected:Connect(function(t) print("Disconnected", t.Name) end)
        uis.InputEnded:Connect(function(i) print("Ended", i.KeyCode.Name, i.Position) end)
        "#,
    );
    connect(&mut session, index, 0);
    connect(&mut session, index, 1);
    let axis = GamepadAxis::StickX(0);
    session.input(
        index,
        InputEvent::GamepadAxis {
            pad: 0,
            axis,
            value: 0.1,
        },
    );
    session.input(index, InputEvent::GamepadDisconnected { pad: 1 });
    session.input(index, InputEvent::GamepadDisconnected { pad: 0 });
    session.run_for(0.05);
    assert_eq!(
        lines(session.client(index).unwrap()),
        [
            "GamepadEnabled true",
            "PreferredInput Gamepad",
            "Connected Gamepad1",
            "Connected Gamepad2",
            "Disconnected Gamepad2",
            "LastInputType Gamepad1",
            "Ended Thumbstick1 0, 0, 0",
            "GamepadEnabled false",
            "PreferredInput KeyboardAndMouse",
            "Disconnected Gamepad1",
        ]
    );
}

/// A pad plugged in before the client starts is there for its scripts'
/// first line, with Gamepad preferred and its own key names (measured).
#[test]
fn pads_present_at_start_count_from_the_first_line() {
    let mut server = Server::new(&Options::default()).unwrap();
    let starter = service(&server, "StarterGui");
    let screen = add_under(&server, starter, "ScreenGui", "Hud", "");
    let source = r#"
        local uis = game:GetService("UserInputService")
        print(#uis:GetConnectedGamepads(), uis.GamepadEnabled, uis.PreferredInput.Name)
        print(uis:GetImageForKeyCode(Enum.KeyCode.ButtonB))
    "#;
    add_under(&server, screen, "LocalScript", "Log", source);
    server.start();
    let mut session = Session::new(server, Options::default());
    let present = [InputEvent::GamepadConnected {
        pad: 0,
        kind: GamepadKind::PlayStation5,
        supported: GamepadKey::ITEMS.iter().map(|&(_, k)| k).collect(),
    }];
    let index = session.join_with("Player", 1, &present).unwrap();
    session.run_for(0.1);
    assert_eq!(
        lines(session.client(index).unwrap()),
        [
            "1 true Gamepad",
            "rbxasset://textures/ui/Controls/PlayStationController/ButtonCircle@2x.png",
        ]
    );
}

/// Buttons begin and end with Position.Z 1 and 0; a stick counts as the
/// pad in use from 0.2 out; a trigger changes with its pull, begins at a
/// full pull and ends at rest. Every event passes the slot's own object,
/// the one GetGamepadState returns (measured).
#[test]
fn pad_input_reaches_scripts_as_studio_sends_it() {
    let (mut session, index) = client_with(
        r#"
        local uis = game:GetService("UserInputService")
        local state = {}
        for _, o in uis:GetGamepadState(Enum.UserInputType.Gamepad1) do
            state[o.KeyCode] = o
        end
        uis.LastInputTypeChanged:Connect(function(t) print("LastInputType", t.Name) end)
        for _, name in { "InputBegan", "InputChanged", "InputEnded" } do
            uis[name]:Connect(function(i, processed)
                if i.UserInputType ~= Enum.UserInputType.Gamepad1 then
                    return
                end
                print(name, i.KeyCode.Name, i.Position, i.Delta, state[i.KeyCode] == i,
                    uis:IsGamepadButtonDown(Enum.UserInputType.Gamepad1, i.KeyCode))
            end)
        end
        "#,
    );
    connect(&mut session, index, 0);
    let button = |down| InputEvent::GamepadButton {
        pad: 0,
        key: GamepadKey::ButtonA,
        down,
    };
    session.input(index, button(true));
    session.input(index, button(false));
    for value in [0.125, 0.25] {
        let axis = GamepadAxis::StickY(0);
        session.input(
            index,
            InputEvent::GamepadAxis {
                pad: 0,
                axis,
                value,
            },
        );
    }
    for value in [0.5, 1.0, 0.0] {
        let axis = GamepadAxis::Trigger(1);
        session.input(
            index,
            InputEvent::GamepadAxis {
                pad: 0,
                axis,
                value,
            },
        );
    }
    session.run_for(0.05);
    assert_eq!(
        lines(session.client(index).unwrap()),
        [
            "LastInputType Gamepad1",
            "InputBegan ButtonA 0, 0, 1 0, 0, 1 true true",
            "InputEnded ButtonA 0, 0, 0 0, 0, -1 true false",
            "InputChanged Thumbstick1 0, 0.125, 0 0, 0.125, 0 true false",
            "InputChanged Thumbstick1 0, 0.25, 0 0, 0.125, 0 true false",
            "InputChanged ButtonR2 0, 0, 0.5 0, 0, 0.5 true false",
            "InputChanged ButtonR2 0, 0, 1 0, 0, 0.5 true false",
            "InputBegan ButtonR2 0, 0, 1 0, 0, 0 true true",
            "InputChanged ButtonR2 0, 0, 0 0, 0, -1 true true",
            "InputEnded ButtonR2 0, 0, 0 0, 0, 0 true false",
        ]
    );
}

/// ContextActionService hears input before UserInputService, and a
/// binding that keeps it makes the event processed; BindActivate keeps
/// nothing (measured).
#[test]
fn bindings_hear_input_first_and_sink_it() {
    let (mut session, index) = client_with(
        r#"
        local cas = game:GetService("ContextActionService")
        local uis = game:GetService("UserInputService")
        cas:BindAction("jump", function(_, state, i)
            print("bound", i.KeyCode.Name, state.Name)
        end, false, Enum.KeyCode.ButtonA, Enum.KeyCode.E)
        cas:BindActivate(Enum.UserInputType.Gamepad1, Enum.KeyCode.ButtonR2)
        uis.InputBegan:Connect(function(i, processed)
            print("uis", i.KeyCode.Name, processed)
        end)
        "#,
    );
    connect(&mut session, index, 0);
    session.input(
        index,
        InputEvent::GamepadButton {
            pad: 0,
            key: GamepadKey::ButtonA,
            down: true,
        },
    );
    session.input(
        index,
        InputEvent::Key {
            code: "E".into(),
            down: true,
        },
    );
    let axis = GamepadAxis::Trigger(1);
    session.input(
        index,
        InputEvent::GamepadAxis {
            pad: 0,
            axis,
            value: 1.0,
        },
    );
    session.run_for(0.05);
    assert_eq!(
        lines(session.client(index).unwrap()),
        [
            "bound ButtonA Begin",
            "uis ButtonA true",
            "bound E Begin",
            "uis E true",
            "uis ButtonR2 false",
        ]
    );
}

/// Focus is an input of its own: LastInputType turns to Focus, the one
/// Focus object begins or ends, then the window signal fires (measured).
#[test]
fn window_focus_is_an_input() {
    let (mut session, index) = client_with(
        r#"
        local uis = game:GetService("UserInputService")
        local seen
        uis.LastInputTypeChanged:Connect(function(t) print("LastInputType", t.Name) end)
        for _, name in { "InputBegan", "InputEnded" } do
            uis[name]:Connect(function(i)
                print(name, i.UserInputType.Name, seen == nil or seen == i)
                seen = i
            end)
        end
        uis.WindowFocusReleased:Connect(function() print("released") end)
        uis.WindowFocused:Connect(function() print("focused") end)
        "#,
    );
    session.input(index, InputEvent::MouseMove { x: 5.0, y: 5.0 });
    for focused in [false, true] {
        session.input(index, InputEvent::Focus { focused });
    }
    session.run_for(0.05);
    assert_eq!(
        lines(session.client(index).unwrap()),
        [
            "LastInputType MouseMovement",
            "LastInputType Focus",
            "InputEnded Focus true",
            "released",
            "InputBegan Focus true",
            "focused",
        ]
    );
}
