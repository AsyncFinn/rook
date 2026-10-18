//! Tweens a LocalScript plays on a client, as places that animate doors
//! on their clients do: the server fires a RemoteEvent and a LocalScript
//! the player's character got from StarterCharacterScripts tweens the door.

use super::*;

/// The door's x position on a DataModel.
fn door_x(dm: &DataModel) -> f32 {
    let world = dm.runtime().world();
    let dom = &world.dom;
    let ws = dom
        .find_first_child(dom.root(), "Workspace", false)
        .unwrap();
    let door = dom.find_first_child(ws, "Door", false).unwrap();
    let panel = dom.find_first_child(door, "Panel", false).unwrap();
    match dom.property_by_name(panel, "CFrame") {
        Some(Value::CFrame(cf)) => cf.position.x,
        other => panic!("CFrame {other:?}"),
    }
}

/// The character spawns with StarterCharacterScripts' contents, and a
/// tween its LocalScript plays moves the client's door frame by frame. The
/// door is pivoted every frame about a saved pivot that, as in places, is
/// very slightly skewed.
#[test]
fn client_tween_moves_a_door() {
    let mut server = Server::new(&Options::default()).unwrap();
    let rs = service(&server, "ReplicatedStorage");
    add_under(&server, rs, "RemoteEvent", "TweenDoor", "");
    let ws = service(&server, "Workspace");
    let door = add_under(&server, ws, "Model", "Door", "");
    let panel = add_under(&server, door, "Part", "Panel", "");
    {
        let mut w = server.runtime.world_mut();
        w.dom
            .set_property_by_name(panel, "Anchored", Value::Bool(true))
            .unwrap();
        let mut pivot = rook_types::CFrame::IDENTITY;
        pivot.rotation *= 1.0001;
        w.dom
            .set_property_by_name(door, "WorldPivotData", Value::OptionalCFrame(Some(pivot)))
            .unwrap();
    }
    add_script(
        &server,
        "ServerScriptService",
        "Doors",
        r#"
        game.Players.PlayerAdded:Wait()
        task.wait(0.2)
        game.ReplicatedStorage.TweenDoor:FireAllClients(workspace.Door, {2, Enum.EasingStyle.Linear}, CFrame.new(10, 0, 0))
        "#,
    );
    let sp = service(&server, "StarterPlayer");
    let scs = add_under(
        &server,
        sp,
        "StarterCharacterScripts",
        "StarterCharacterScripts",
        "",
    );
    add_under(
        &server,
        scs,
        "LocalScript",
        "Tweener",
        r#"
        print("tweener in", script.Parent.Name)
        local TweenService = game:GetService("TweenService")
        game.ReplicatedStorage.TweenDoor.OnClientEvent:Connect(function(model, info, to)
            local value = Instance.new("CFrameValue", model)
            value.Value = model:GetPivot()
            value:GetPropertyChangedSignal("Value"):Connect(function()
                model:PivotTo(value.Value)
            end)
            TweenService:Create(value, TweenInfo.new(table.unpack(info)), { Value = to }):Play()
        end)
        "#,
    );
    server.start();
    let mut session = Session::new(server, Options::default());
    let index = session.join("Player1", 1).unwrap();
    session.run_for(1.2);
    let x = door_x(session.client(index).unwrap());
    let log: Vec<String> = session
        .client(index)
        .unwrap()
        .take_log()
        .into_iter()
        .map(|e| e.message)
        .collect();
    assert_eq!(log, ["tweener in Player1"], "runs in the character");
    // Partway through the 2 s tween, not snapped to either end.
    assert!((4.0..6.0).contains(&x), "client door at x = {x}");
    assert_eq!(door_x(session.server()), 0.0, "the server's door stays");
}
