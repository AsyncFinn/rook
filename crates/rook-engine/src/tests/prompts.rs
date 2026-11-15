use super::*;

/// The client's character stands anchored at the origin and the camera
/// looks down +Z from it, so prompts' screen points are known.
const STAGE: &str = r#"
    local player = game:GetService("Players").LocalPlayer
    local character = player.Character or player.CharacterAdded:Wait()
    local root = character:WaitForChild("HumanoidRootPart")
    root.Anchored = true
    root.CFrame = CFrame.new(0, 50, 0)
    local camera = workspace.CurrentCamera
    camera.CameraType = Enum.CameraType.Scriptable
    game:GetService("RunService").RenderStepped:Connect(function()
        camera.CFrame = CFrame.lookAt(Vector3.new(0, 50, 0), Vector3.new(0, 50, 10))
    end)
    local svc = game:GetService("ProximityPromptService")
    local function part(name, at)
        local p = Instance.new("Part")
        p.Name = name
        p.Anchored = true
        p.Size = Vector3.new(0.2, 0.2, 0.2)
        p.Position = Vector3.new(0, 50, 0) + at
        p.Parent = workspace
        return p
    end
    local function prompt(name, at, props)
        local p = Instance.new("ProximityPrompt")
        p.Name = name
        for k, v in props or {} do
            p[k] = v
        end
        p.Parent = part(name, at)
        return p
    end
"#;

fn session_with(local: &str, server_script: Option<&str>) -> (Session, usize) {
    let mut server = Server::new(&Options::default()).unwrap();
    use_r6(&server);
    add_local_script(&server, "Stage", &format!("{STAGE}\n{local}"));
    if let Some(source) = server_script {
        add_script(&server, "ServerScriptService", "Watch", source);
    }
    server.start();
    let mut session = Session::new(server, Options::default());
    let index = session.join("Tester", 1).unwrap();
    session.client_mut(index).unwrap().set_viewport(800, 600);
    session.run_for(0.4);
    (session, index)
}

fn key(session: &mut Session, index: usize, code: &str, down: bool) {
    session.input(
        index,
        InputEvent::Key {
            code: code.into(),
            down,
        },
    );
}

/// A prompt in reach shows with the keyboard's input type; its key
/// triggers it before UserInputService hears the key, as processed, and
/// ContextActionService's binding doesn't; the release ends it. Both
/// events fire on the server too with the player, the prompt's event
/// before the service's (measured in Studio 0.744).
#[test]
fn a_key_triggers_a_shown_prompt_on_both_sides() {
    let local = r#"
        svc.PromptShown:Connect(function(p, t) print("shown", p.Name, t.Name) end)
        svc.PromptTriggered:Connect(function(p, who) print("svc triggered", p.Name, who.Name) end)
        game:GetService("UserInputService").InputBegan:Connect(function(i, gp) print("began", i.KeyCode.Name, gp) end)
        game:GetService("UserInputService").InputEnded:Connect(function(i, gp) print("ended", i.KeyCode.Name, gp) end)
        game:GetService("ContextActionService"):BindAction("Use", function() print("action") end, false, Enum.KeyCode.E)
        local door = workspace:WaitForChild("Door"):WaitForChild("Door")
        door.Triggered:Connect(function(who) print("triggered", who.Name) end)
        door.TriggerEnded:Connect(function(who) print("trigger ended", who.Name) end)
    "#;
    let server_script = r#"
        local part = Instance.new("Part")
        part.Name = "Door"
        part.Anchored = true
        part.Position = Vector3.new(0, 50, 6)
        local door = Instance.new("ProximityPrompt")
        door.Name = "Door"
        door.Parent = part
        part.Parent = workspace
        local svc = game:GetService("ProximityPromptService")
        door.Triggered:Connect(function(who) print("triggered", who.Name) end)
        svc.PromptTriggered:Connect(function(p, who) print("svc triggered", p.Name, who.Name) end)
        door.TriggerEnded:Connect(function(who) print("trigger ended", who.Name) end)
    "#;
    let (mut session, index) = session_with(local, Some(server_script));
    key(&mut session, index, "E", true);
    session.run_for(0.1);
    key(&mut session, index, "E", false);
    session.run_for(0.1);
    assert_eq!(
        lines(session.client(index).unwrap()),
        [
            "shown Door Keyboard",
            "triggered Tester",
            "svc triggered Door Tester",
            "began E true",
            "trigger ended Tester",
            "ended E true",
        ]
    );
    assert_eq!(
        lines(session.server()),
        [
            "triggered Tester",
            "svc triggered Door Tester",
            "trigger ended Tester"
        ]
    );
}

/// Which prompts show (measured): on one key, the one whose screen point
/// is nearest a point the GUI inset below the viewport's center, even
/// when farther; one per key; none a part hides from the camera unless
/// it doesn't need line of sight; `AlwaysShow` ones up to
/// `MaxPromptsVisible`; out of reach but within `MaxIndicatorDistance`, an
/// indicator.
#[test]
fn prompts_show_by_screen_point_key_and_sight() {
    // The viewport is 800 by 600 with a 70 degree field of view, so 1 stud
    // across at depth d is 300 / tan(35 degrees) / d pixels. `Low` sits
    // 58 pixels below the center at depth 9, `Center` on it at depth 8.
    let local = r#"
        local px = 300 / math.tan(math.rad(35))
        local shown, indicated = {}, {}
        svc.PromptShown:Connect(function(p) shown[p.Name] = true end)
        svc.PromptHidden:Connect(function(p) shown[p.Name] = nil end)
        svc.IndicatorShown:Connect(function(p) indicated[p.Name] = true end)
        prompt("Center", Vector3.new(0, 0, 8))
        prompt("Low", Vector3.new(0.1, -58 / px * 9, 9))
        local wall = part("Wall", Vector3.new(-2, 0, 5))
        wall.Size = Vector3.new(2, 2, 0.2)
        prompt("Hidden", Vector3.new(-3, 0, 8), {KeyboardKeyCode = Enum.KeyCode.F})
        prompt("Seen", Vector3.new(-3, 0.5, 8), {KeyboardKeyCode = Enum.KeyCode.R, RequiresLineOfSight = false})
        svc.MaxPromptsVisible = 1
        local always = {Exclusivity = Enum.ProximityPromptExclusivity.AlwaysShow}
        prompt("AlwaysNear", Vector3.new(1, -1, 8), always)
        prompt("AlwaysFar", Vector3.new(3, -1, 8), always)
        prompt("Indicated", Vector3.new(0, 0, 14), {KeyboardKeyCode = Enum.KeyCode.G, MaxIndicatorDistance = 20})
        task.wait(0.2)
        local function sorted(set)
            local list = {}
            for name in set do table.insert(list, name) end
            table.sort(list)
            return table.concat(list, " ")
        end
        print("shown", sorted(shown))
        print("indicated", sorted(indicated))
    "#;
    let (mut session, index) = session_with(local, None);
    session.run_for(0.3);
    assert_eq!(
        lines(session.client(index).unwrap()),
        ["shown AlwaysNear Low Seen", "indicated Indicated"]
    );
}

/// Holding (measured): a press fires `PromptButtonHoldBegan`; letting go
/// early ends the hold without a trigger; once `HoldDuration` has passed
/// the hold ends and the prompt triggers, and the release ends the
/// trigger. `InputHoldBegin` holds as a key does, and a prompt hidden
/// while triggered ends the trigger.
#[test]
fn holds_trigger_after_their_duration() {
    let local = r#"
        local door = prompt("Door", Vector3.new(0, 0, 6), {HoldDuration = 0.5})
        for _, event in {"PromptButtonHoldBegan", "PromptButtonHoldEnded", "Triggered", "TriggerEnded", "PromptHidden"} do
            door[event]:Connect(function() print(event) end)
        end
    "#;
    let (mut session, index) = session_with(local, None);
    key(&mut session, index, "E", true);
    session.run_for(0.2);
    key(&mut session, index, "E", false);
    session.run_for(0.1);
    key(&mut session, index, "E", true);
    session.run_for(0.7);
    key(&mut session, index, "E", false);
    session.run_for(0.1);
    let client = session.client(index).unwrap();
    let source = "local door = workspace.Door.Door         door.HoldDuration = 0 door:InputHoldBegin() door:InputHoldBegin() door.Enabled = false";
    client.runtime.run_source("hold", source, None).unwrap();
    session.run_for(0.1);
    assert_eq!(
        lines(session.client(index).unwrap()),
        [
            "PromptButtonHoldBegan",
            "PromptButtonHoldEnded",
            "PromptButtonHoldBegan",
            "PromptButtonHoldEnded",
            "Triggered",
            "TriggerEnded",
            "Triggered",
            "TriggerEnded",
            "PromptHidden",
        ]
    );
}
