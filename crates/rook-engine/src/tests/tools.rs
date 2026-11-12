use super::*;

/// A Tool with a Handle in StarterPack, which every player's Backpack
/// gets, with a server Script printing its events.
fn starter_tool(server: &Server, name: &str) -> InstanceId {
    let pack = service(server, "StarterPack");
    let tool = add_under(server, pack, "Tool", name, "");
    add_under(server, tool, "Part", "Handle", "");
    add_under(
        server,
        tool,
        "Script",
        "Watch",
        r#"
        local tool = script.Parent
        tool.Equipped:Connect(function(mouse) print("equipped", tool.Parent.ClassName, mouse.ClassName, mouse.Name) end)
        tool.Unequipped:Connect(function() print("unequipped", tool.Parent.ClassName) end)
        tool.Activated:Connect(function() print("activated") end)
        tool.Deactivated:Connect(function() print("deactivated") end)
        "#,
    );
    tool
}

/// The number of `RightGrip` welds in the character's right arm.
fn grips(dm: &DataModel, character: InstanceId) -> usize {
    let w = dm.runtime.world();
    let arm = w
        .dom
        .find_first_child(character, "Right Arm", false)
        .unwrap();
    w.dom
        .children(arm)
        .iter()
        .filter(|c| w.dom.name(**c) == "RightGrip")
        .count()
}

fn local_character(dm: &DataModel) -> InstanceId {
    let w = dm.runtime.world();
    let player = w.dom.local_player().unwrap();
    w.dom.ref_property(player, "Character").unwrap()
}

/// The Backpack's Tools equip and unequip from the client (setting the
/// Parent, as the backpack CoreScript does, and the Humanoid's methods);
/// the server repeats each move, so a server Script in the Tool hears
/// `Equipped` (with an inactive Mouse) and `Unequipped`. The client makes
/// the grip and the server's copy of it takes the client's place, so the
/// arm holds one grip on each side.
#[test]
fn client_equips_reach_the_server() {
    let mut server = Server::new(&Options::default()).unwrap();
    use_r6(&server);
    starter_tool(&server, "Sword");
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
    session.run_for(0.6);
    assert_eq!(
        lines(session.server()),
        [
            "equipped Model Mouse Instance",
            "unequipped Backpack",
            "equipped Model Mouse Instance"
        ]
    );
    let (character, _) = character(session.server(), "Holder");
    assert_eq!(grips(session.server(), character), 1);
    let client = session.client_mut(index).unwrap();
    assert_eq!(
        lines(client),
        ["client equipped Mouse", "client equipped Mouse"]
    );
    let character = local_character(client);
    assert_eq!(grips(client, character), 1);
}

/// A left click over the world activates the held Tool on press and
/// deactivates it on release, on the client and then the server;
/// `ManualActivationOnly` leaves only the release, and a disabled Tool
/// fires nothing. The Mouse `Equipped` gave reads while the Tool is held
/// and errors once it's let go.
#[test]
fn clicks_activate_the_held_tool() {
    let mut server = Server::new(&Options::default()).unwrap();
    use_r6(&server);
    starter_tool(&server, "Wand");
    add_local_script(
        &server,
        "Use",
        r#"
        local player = game.Players.LocalPlayer
        local character = player.Character or player.CharacterAdded:Wait()
        local tool = player.Backpack:WaitForChild("Wand")
        local held
        tool.Equipped:Connect(function(mouse) held = mouse end)
        tool.Activated:Connect(function() print("client activated") end)
        tool.Deactivated:Connect(function() print("client deactivated") end)
        tool.Parent = character
        task.wait(0.3)
        print("hit", (pcall(function() return held.Hit end)))
        tool.ManualActivationOnly = true
        task.wait(0.2)
        tool.Enabled = false
        task.wait(0.2)
        tool.Parent = player.Backpack
        print(pcall(function() return held.Hit end))
        "#,
    );
    server.start();
    let mut session = Session::new(server, Options::default());
    let index = session.join("Clicker", 1).unwrap();
    session.run_for(0.2);
    for _ in 0..3 {
        for down in [true, false] {
            session.input(index, InputEvent::MouseButton { button: 1, down });
        }
        session.run_for(0.2);
    }
    session.run_for(0.2);
    assert_eq!(
        lines(session.server()),
        [
            "equipped Model Mouse Instance",
            "activated",
            "deactivated",
            "deactivated",
            "unequipped Backpack"
        ]
    );
    let client = session.client_mut(index).unwrap();
    assert_eq!(
        lines(client),
        [
            "client activated",
            "client deactivated",
            "hit true",
            "client deactivated",
            "false This Mouse is no longer active"
        ]
    );
}

/// Backspace drops the held Tool into Workspace when `CanBeDropped` is on,
/// and does nothing when it's off.
#[test]
fn backspace_drops_the_held_tool() {
    let mut server = Server::new(&Options::default()).unwrap();
    use_r6(&server);
    starter_tool(&server, "Loose");
    starter_tool(&server, "Kept");
    add_local_script(
        &server,
        "Hold",
        r#"
        local player = game.Players.LocalPlayer
        local character = player.Character or player.CharacterAdded:Wait()
        local humanoid = character:WaitForChild("Humanoid")
        local loose = player.Backpack:WaitForChild("Loose")
        local kept = player.Backpack:WaitForChild("Kept")
        humanoid:EquipTool(loose)
        task.wait(0.4)
        kept.CanBeDropped = false
        humanoid:EquipTool(kept)
        "#,
    );
    server.start();
    let mut session = Session::new(server, Options::default());
    let index = session.join("Dropper", 1).unwrap();
    let backspace = |session: &mut Session| {
        session.run_for(0.2);
        for down in [true, false] {
            let code = "Backspace".into();
            session.input(index, InputEvent::Key { code, down });
        }
        session.run_for(0.2);
    };
    backspace(&mut session);
    backspace(&mut session);
    let (character, _) = character(session.server(), "Dropper");
    let w = session.server().runtime.world();
    let workspace = w.dom.find_service("Workspace").unwrap();
    let parent_of = |name: &str| {
        let tool = w.dom.find_first_child(workspace, name, true).unwrap();
        w.dom.parent(tool).unwrap()
    };
    assert_eq!(parent_of("Loose"), workspace);
    assert_eq!(parent_of("Kept"), character);
}

/// A player's character touching a lying Tool's Handle picks it up; the
/// server's `Tool:Activate()` then fires on the server and the client.
#[test]
fn touching_a_tool_picks_it_up() {
    let mut server = Server::new(&Options::default()).unwrap();
    use_r6(&server);
    add_script(
        &server,
        "ServerScriptService",
        "Drop",
        r#"
        game.Players.PlayerAdded:Connect(function(player)
            player.CharacterAdded:Connect(function(character)
                local root = character:WaitForChild("HumanoidRootPart")
                task.wait(0.2)
                local tool = Instance.new("Tool")
                tool.Name = "Lying"
                local handle = Instance.new("Part")
                handle.Name = "Handle"
                handle.Size = Vector3.new(1, 1, 1)
                handle.CFrame = root.CFrame
                handle.Parent = tool
                tool.Activated:Connect(function() print("activated") end)
                tool.Parent = workspace
                tool.Equipped:Wait()
                print("picked up", tool.Parent == character)
                task.wait(0.2)
                tool:Activate()
            end)
        end)
        "#,
    );
    add_local_script(
        &server,
        "Hear",
        r#"
        local character = game.Players.LocalPlayer.CharacterAdded:Wait()
        local tool = character:WaitForChild("Lying")
        tool.Activated:Connect(function() print("client activated") end)
        "#,
    );
    server.start();
    let mut session = Session::new(server, Options::default());
    let index = session.join("Finder", 1).unwrap();
    session.run_for(1.0);
    assert_eq!(lines(session.server()), ["picked up true", "activated"]);
    let client = session.client_mut(index).unwrap();
    assert_eq!(lines(client), ["client activated"]);
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
