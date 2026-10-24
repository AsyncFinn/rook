//! Characters: their rigs, where they spawn, teams, respawning, and the
//! client simulating its own (docs/behavior/characters/).

use super::*;

/// A server whose place `setup` builds (a Script run at start), with
/// `players` joined and `seconds` run. Characters are R6 unless
/// `options` give content.
fn session_with(setup: &str, players: &[&str], seconds: f64, options: &Options) -> Session {
    let mut server = Server::new(options).unwrap();
    // Rigs are R6 unless the rig file is given: tests can't rely on the
    // machine's Roblox content (scripts can't set this).
    if options.content.is_none() {
        let sp = service(&server, "StarterPlayer");
        let r6 = rook_types::Value::Enum(0);
        server
            .runtime
            .world_mut()
            .dom
            .set_property_by_name(sp, "GameSettingsAvatar", r6)
            .unwrap();
    }
    add_script(&server, "ServerScriptService", "Setup", setup);
    server.start();
    let mut session = Session::new(server, options.clone());
    for (i, name) in players.iter().enumerate() {
        session.join(name, i as i64 + 1).unwrap();
    }
    session.run_for(seconds);
    session
}

/// The character of the server's player `index` (its model and root part).
fn character(dm: &DataModel, name: &str) -> (InstanceId, InstanceId) {
    let w = dm.runtime.world();
    let players = w.dom.find_service("Players").unwrap();
    let player = w.dom.find_first_child(players, name, false).unwrap();
    let model = w.dom.ref_property(player, "Character").unwrap();
    let root = w
        .dom
        .find_first_child(model, "HumanoidRootPart", false)
        .unwrap();
    (model, root)
}

fn position(dm: &DataModel, part: InstanceId) -> glam::Vec3 {
    dm.runtime
        .world()
        .dom
        .cframe_property(part, "CFrame")
        .unwrap()
        .position
}

/// R6 is built as Studio 0.744 builds it, standing on the spawn: the root
/// part 3.01 studs above its top (measured).
#[test]
fn r6_characters_stand_on_their_spawn() {
    let setup = r#"local s = Instance.new("SpawnLocation")
        s.Anchored = true
        s.Size = Vector3.new(12, 1, 12)
        s.Position = Vector3.new(10, 0.5, 20)
        s.Duration = 0
        s.Parent = workspace
        "#;
    let session = session_with(setup, &["A"], 1.0 / 60.0, &Options::default());
    let server = session.server();
    let (model, root) = character(server, "A");
    let w = server.runtime.world();
    let dom = &w.dom;
    let names: Vec<&str> = dom.children(model).iter().map(|&c| dom.name(c)).collect();
    assert_eq!(
        names,
        [
            "Head",
            "Torso",
            "Left Arm",
            "Right Arm",
            "Left Leg",
            "Right Leg",
            "Humanoid",
            "HumanoidRootPart",
            "Body Colors"
        ]
    );
    let torso = dom.find_first_child(model, "Torso", false).unwrap();
    let shoulder = dom
        .find_first_child(torso, "Right Shoulder", false)
        .unwrap();
    let c0 = dom.cframe_property(shoulder, "C0").unwrap();
    assert_eq!(
        c0.components(),
        [1.0, 0.5, 0.0, 0.0, 0.0, 1.0, 0.0, 1.0, 0.0, -1.0, 0.0, 0.0]
    );
    let at = position(server, root);
    // One frame of the client's falling has come back already.
    assert!((at.y - 4.01).abs() < 0.05, "{at}");
    assert!(
        (5.0..=15.0).contains(&at.x) && (15.0..=25.0).contains(&at.z),
        "{at}"
    );
    assert!(dom.find_first_child_of_class(model, "ForceField").is_none());
    let head = dom.find_first_child(model, "Head", false).unwrap();
    let yellow = rook_types::Color3::from(rook_types::BrickColor::from_number(24).unwrap());
    assert_eq!(
        dom.color3_property(head, "Color"),
        Some(yellow),
        "the default look"
    );
}

/// R15 bodies are Roblox's rig file, read through content.
#[test]
fn r15_characters_come_from_the_rig_file() {
    let dir = std::env::temp_dir()
        .join("rook-engine-tests")
        .join(format!("rig-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("content/avatar")).unwrap();
    // A stand-in for Roblox's file, made here: a root part and a torso.
    let rig = r#"<roblox version="4"><Item class="Model" referent="M"><Properties><string name="Name">Player</string></Properties>
        <Item class="Part" referent="R"><Properties><string name="Name">HumanoidRootPart</string>
          <CoordinateFrame name="CFrame"><X>0</X><Y>17</Y><Z>22</Z><R00>1</R00><R01>0</R01><R02>0</R02><R10>0</R10><R11>1</R11><R12>0</R12><R20>0</R20><R21>0</R21><R22>1</R22></CoordinateFrame>
          <Vector3 name="size"><X>2</X><Y>2</Y><Z>1</Z></Vector3></Properties>
          <Item class="Motor6D" referent="J"><Properties><string name="Name">Root</string><Ref name="Part0">R</Ref><Ref name="Part1">T</Ref>
            <CoordinateFrame name="C0"><X>0</X><Y>0.2</Y><Z>0</Z><R00>1</R00><R01>0</R01><R02>0</R02><R10>0</R10><R11>1</R11><R12>0</R12><R20>0</R20><R21>0</R21><R22>1</R22></CoordinateFrame></Properties></Item></Item>
        <Item class="Part" referent="T"><Properties><string name="Name">UpperTorso</string>
          <CoordinateFrame name="CFrame"><X>0</X><Y>17.2</Y><Z>22</Z><R00>1</R00><R01>0</R01><R02>0</R02><R10>0</R10><R11>1</R11><R12>0</R12><R20>0</R20><R21>0</R21><R22>1</R22></CoordinateFrame>
          <Vector3 name="size"><X>2</X><Y>1.6</Y><Z>1</Z></Vector3></Properties></Item>
        <Item class="Humanoid" referent="H"><Properties><string name="Name">Humanoid</string><token name="RigType">1</token><float name="HipHeight">1.35</float></Properties></Item>
        </Item></roblox>"#;
    std::fs::write(dir.join("content/avatar/characterR15.rbxm"), rig).unwrap();
    let session = session_with("", &["A"], 1.0 / 60.0, &install_options(&dir));
    let server = session.server();
    let (model, root) = character(server, "A");
    let w = server.runtime.world();
    let dom = &w.dom;
    let torso = dom.find_first_child(model, "UpperTorso", false).unwrap();
    let humanoid = dom.find_first_child_of_class(model, "Humanoid").unwrap();
    assert_eq!(dom.name(model), "A");
    assert!(
        dom.find_first_child_of_class(humanoid, "HumanoidDescription")
            .is_some()
    );
    // No spawn: feet 100 studs up, above the origin (measured).
    let at = position(server, root);
    assert!((at.y - (100.0 + 1.0 + 0.01)).abs() < 0.05, "{at}");
    assert!((position(server, torso).y - at.y - 0.2).abs() < 1e-3);
    drop(w);
    let _ = std::fs::remove_dir_all(&dir);
}

/// Joining players fill the auto-assignable team with the fewest
/// players, and spawn on their team's spawns.
#[test]
fn teams_assign_players_and_their_spawns() {
    let setup = r#"local function team(name, color)
            local t = Instance.new("Team")
            t.Name = name
            t.TeamColor = BrickColor.new(color)
            t.Parent = game:GetService("Teams")
        end
        team("Red", "Bright red")
        team("Blue", "Bright blue")
        local function spawn(x, color)
            local s = Instance.new("SpawnLocation")
            s.Anchored = true
            s.Size = Vector3.new(6, 1, 6)
            s.Position = Vector3.new(x, 0.5, 0)
            s.Neutral = false
            s.TeamColor = BrickColor.new(color)
            s.Parent = workspace
        end
        spawn(-30, "Bright red")
        spawn(30, "Bright blue")
        "#;
    let session = session_with(setup, &["A", "B"], 1.0 / 60.0, &Options::default());
    let server = session.server();
    let (_, a) = character(server, "A");
    let (_, b) = character(server, "B");
    // Spawns 6 studs across put players at their center (measured), with
    // a ForceField for the default 10 seconds.
    assert_eq!(position(server, a).x, -30.0);
    assert_eq!(position(server, b).x, 30.0);
    let w = server.runtime.world();
    let players = w.dom.find_service("Players").unwrap();
    let team = |name| {
        let p = w.dom.find_first_child(players, name, false).unwrap();
        w.dom
            .ref_property(p, "Team")
            .map(|t| w.dom.name(t).to_owned())
    };
    assert_eq!(
        [team("A"), team("B")],
        [Some("Red".into()), Some("Blue".into())]
    );
}

/// A dead character's player respawns `Players.RespawnTime` after Died.
#[test]
fn players_respawn_after_dying() {
    let setup = r#"game.Players.RespawnTime = 1
        game.Players.PlayerAdded:Connect(function(p)
            p.CharacterAdded:Connect(function(c)
                print("added", math.round(time() * 10) / 10)
                c:WaitForChild("Humanoid").Died:Connect(function() print("died", math.round(time() * 10) / 10) end)
            end)
        end)
        task.wait(0.5)
        game.Players.A.Character.Humanoid.Health = 0
        "#;
    let session = session_with(setup, &["A"], 2.0, &Options::default());
    assert_eq!(
        lines(session.server()),
        ["added 0", "died 0.5", "added 1.5"]
    );
}

/// The client simulates its own character and the server takes what it
/// sends: the server has the client's position after each session frame,
/// other clients see it move, the server's Touched fires for it, and the
/// server's own moves reach the client.
#[test]
fn clients_simulate_their_own_characters() {
    let setup = r#"local pad = Instance.new("Part")
        pad.Name = "Pad"
        pad.Anchored = true
        pad.Size = Vector3.new(20, 1, 20)
        pad.Position = Vector3.new(0, 50, 0)
        pad.Parent = workspace
        local touched = false
        pad.Touched:Connect(function(hit)
            if not touched then
                touched = true
                print("touched by", hit.Parent.Name)
            end
        end)
        game.Players.PlayerAdded:Connect(function(p)
            if p.Name ~= "A" then return end
            p.CharacterAdded:Connect(function(c)
                local root = c:WaitForChild("HumanoidRootPart")
                task.wait(2)
                print("owner", root:GetNetworkOwner() == p)
                root.CFrame = CFrame.new(100, 200, 0)
            end)
        end)
        "#;
    let mut session = session_with(setup, &["A", "B"], 1.5, &Options::default());
    let (_, root) = character(session.server(), "A");
    let client_root = |session: &Session, index: usize| {
        let client = session.client(index).unwrap();
        let w = client.runtime.world();
        let players = w.dom.find_service("Players").unwrap();
        let a = w.dom.find_first_child(players, "A", false).unwrap();
        let c = w.dom.ref_property(a, "Character").unwrap();
        w.dom
            .find_first_child(c, "HumanoidRootPart", false)
            .unwrap()
    };
    let (mine, theirs) = (client_root(&session, 0), client_root(&session, 1));
    // Fallen from 103.01 onto the pad on A's client.
    let fallen = position(session.client(0).unwrap(), mine);
    assert!((fallen.y - 53.5).abs() < 0.6, "{fallen}");
    assert_eq!(position(session.server(), root), fallen);
    session.step();
    assert_eq!(position(session.client(1).unwrap(), theirs), fallen);
    session.run_for(1.0);
    assert_eq!(lines(session.server()), ["touched by A", "owner true"]);
    let moved = position(session.client(0).unwrap(), mine);
    assert!(moved.x == 100.0 && moved.y > 150.0, "{moved}");
}
