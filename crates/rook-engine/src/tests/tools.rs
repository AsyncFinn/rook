use super::*;

/// The Backpack's Tools equip and unequip from the client (setting the
/// Parent, as the backpack CoreScript does, and the Humanoid's methods);
/// the server repeats each move, so a server Script in the Backpack hears
/// `Equipped` and `Unequipped`, and the client's own handlers get the Mouse.
#[test]
fn client_equips_reach_the_server() {
    let mut server = Server::new(&Options::default()).unwrap();
    let pack = service(&server, "StarterPack");
    let sword = add_under(&server, pack, "Tool", "Sword", "");
    add_under(
        &server,
        sword,
        "Script",
        "Watch",
        r#"
        local tool = script.Parent
        tool.Equipped:Connect(function(mouse) print("equipped", tool.Parent.ClassName, mouse) end)
        tool.Unequipped:Connect(function() print("unequipped", tool.Parent.ClassName) end)
        "#,
    );
    add_local_script(
        &server,
        "Equip",
        r#"
        local player = game.Players.LocalPlayer
        local character = player.Character or player.CharacterAdded:Wait()
        local humanoid = character:WaitForChild("Humanoid")
        local tool = player.Backpack:WaitForChild("Sword")
        tool.Equipped:Connect(function(mouse) print("client equipped", mouse.ClassName) end)
        tool.Parent = character
        task.wait(0.1)
        humanoid:UnequipTools()
        task.wait(0.1)
        humanoid:EquipTool(tool)
        "#,
    );
    server.start();
    let mut session = Session::new(server, Options::default());
    let index = session.join("Holder", 1).unwrap();
    session.run_for(0.5);
    assert_eq!(
        lines(session.server()),
        [
            "equipped Model nil",
            "unequipped Backpack",
            "equipped Model nil"
        ]
    );
    let client = session.client_mut(index).unwrap();
    assert_eq!(
        lines(client),
        ["client equipped PlayerMouse", "client equipped PlayerMouse"]
    );
}

/// Damage reaches the client as `HealthChanged` with the new health, and
/// through `Health`'s changed signals, though Rook stores it as
/// `Health_XML`.
#[test]
fn health_changes_fire_on_the_client() {
    let mut server = Server::new(&Options::default()).unwrap();
    add_script(
        &server,
        "ServerScriptService",
        "Hurt",
        r#"
        game.Players.PlayerAdded:Connect(function(player)
            player.CharacterAdded:Connect(function(character)
                task.wait(0.1)
                character:WaitForChild("Humanoid"):TakeDamage(25)
            end)
        end)
        "#,
    );
    add_local_script(
        &server,
        "Health",
        r#"
        local player = game.Players.LocalPlayer
        local character = player.Character or player.CharacterAdded:Wait()
        local humanoid = character:WaitForChild("Humanoid")
        humanoid.HealthChanged:Connect(function(health) print("health", health) end)
        humanoid:GetPropertyChangedSignal("Health"):Connect(function() print("signal", humanoid.Health) end)
        humanoid.Changed:Connect(function(name) print("changed", name) end)
        "#,
    );
    server.start();
    let mut session = Session::new(server, Options::default());
    let index = session.join("Hurt", 1).unwrap();
    session.run_for(0.5);
    // Which of the three fires first in Roblox isn't measured.
    let mut heard = lines(session.client_mut(index).unwrap());
    heard.sort();
    assert_eq!(heard, ["changed Health", "health 75", "signal 75"]);
}
