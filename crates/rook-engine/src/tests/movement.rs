//! The Humanoid controller against Studio's recordings
//! (docs/behavior/characters/movement.md): characters are driven by a
//! LocalScript on the client that simulates them, standing on a
//! baseplate whose top is at 0.

use super::*;

/// A baseplate, then `setup`'s parts (a server Script's source).
const FLOOR: &str = r#"local b = Instance.new("Part")
    b.Anchored = true
    b.Size = Vector3.new(400, 2, 400)
    b.Position = Vector3.new(0, -1, 0)
    b.Parent = workspace
    "#;

/// What the client printed while `local` (a LocalScript body, run once
/// the character `c`, its Humanoid `h` and root `r` have settled at
/// `start`) ran for `seconds`.
fn drive(setup: &str, start: &str, local: &str, seconds: f64) -> Vec<String> {
    drive_at(setup, start, local, seconds, None)
}

/// [`drive`], with the client stepped `client_hz` times a second, as a
/// window drawing at that rate steps it, and the server at 60.
fn drive_at(
    setup: &str,
    start: &str,
    local: &str,
    seconds: f64,
    client_hz: Option<f64>,
) -> Vec<String> {
    let server = Server::new(&Options::default()).unwrap();
    use_r6(&server);
    add_script(
        &server,
        "ServerScriptService",
        "Setup",
        &format!("{FLOOR}\n{setup}"),
    );
    let source = format!(
        r#"local RunService = game:GetService("RunService")
        local p = game.Players.LocalPlayer
        local c = p.Character or p.CharacterAdded:Wait()
        local h = c:WaitForChild("Humanoid")
        local r = c:WaitForChild("HumanoidRootPart")
        task.wait(0.2)
        c:PivotTo({start})
        task.wait(0.5)
        local function frames(n) for _ = 1, n do RunService.Heartbeat:Wait() end end
        {local}"#
    );
    add_local_script(&server, "Drive", &source);
    let mut server = server;
    server.start();
    let mut session = Session::new(server, Options::default());
    session.join("A", 1).unwrap();
    match client_hz {
        None => session.run_for(seconds + 1.0),
        Some(hz) => {
            let (mut server, mut client) = (0.0, 0.0);
            while client < seconds + 1.0 {
                client += 1.0 / hz;
                while server < client {
                    server += 1.0 / 60.0;
                    session.step_server();
                }
                session.step_clients_by(1.0 / hz);
            }
        }
    }
    let client = session.client(0).unwrap();
    lines(client)
}

/// The numbers in the client's line starting with `label`.
fn numbers(out: &[String], label: &str) -> Vec<f32> {
    let line = out
        .iter()
        .find(|l| l.starts_with(label))
        .unwrap_or_else(|| panic!("no {label} line in {out:?}"));
    line.split_whitespace()
        .filter_map(|w| w.parse().ok())
        .collect()
}

fn near(got: f32, want: f32, within: f32) -> bool {
    (got - want).abs() <= within
}

/// Measured: walking reaches WalkSpeed within two frames and keeps it,
/// with the root at standing height (R6: 3 above the floor).
#[test]
fn walking_reaches_walk_speed() {
    let local = r#"local z0 = r.Position.Z
        h:Move(Vector3.new(0, 0, -1))
        frames(30)
        print(("walk %.3f %.3f %.3f"):format(z0 - r.Position.Z, -r.AssemblyLinearVelocity.Z, r.Position.Y))
        h:Move(Vector3.zero)
        frames(3)
        print(("stop %.3f"):format(r.AssemblyLinearVelocity.Magnitude))"#;
    let out = drive("", "CFrame.new(0, 3, 0)", local, 1.5);
    let walk = numbers(&out, "walk");
    assert!(near(walk[0], 7.96, 0.1), "{out:?}");
    assert!(near(walk[1], 16.0, 0.01), "{out:?}");
    assert!(near(walk[2], 3.0, 0.02), "{out:?}");
    assert!(numbers(&out, "stop")[0] < 0.01, "{out:?}");
}

/// Measured: a jump of JumpHeight 7.2 rises 7.52 (Studio's stepping),
/// through Jumping, Freefall and Landed, and clears Jump.
#[test]
fn jumping_reaches_its_height() {
    let local = r#"h.UseJumpPower = false
        h.JumpHeight = 7.2
        local y0, top, states = r.Position.Y, r.Position.Y, {}
        h.StateChanged:Connect(function(_, n) table.insert(states, n.Name) end)
        h.Jump = true
        for _ = 1, 50 do RunService.Heartbeat:Wait(); top = math.max(top, r.Position.Y) end
        print(("jump %.3f %s %s"):format(top - y0, tostring(h.Jump), table.concat(states, ",")))"#;
    let out = drive("", "CFrame.new(0, 3, 0)", local, 1.5);
    let rise = numbers(&out, "jump")[0];
    assert!(near(rise, 7.52, 0.1), "{out:?}");
    assert!(
        out.iter()
            .any(|l| l.ends_with("false Jumping,Freefall,Landed,Running")),
        "{out:?}"
    );
}

/// A client drawn at 144 Hz walks at WalkSpeed and jumps as high as at
/// 60 (Inference: Roblox simulates in 240 Hz steps whatever the frame
/// rate; Studio's playtest client ran at 60, so only that is measured).
#[test]
fn walking_and_jumping_keep_pace_at_any_frame_rate() {
    let local = r#"local z0, t0 = r.Position.Z, time()
        h:Move(Vector3.new(0, 0, -1))
        task.wait(0.5)
        print(("walk %.3f %.3f"):format((z0 - r.Position.Z) / (time() - t0), -r.AssemblyLinearVelocity.Z))
        h:Move(Vector3.zero)
        task.wait(0.3)
        h.UseJumpPower = false
        h.JumpHeight = 7.2
        local y0, top = r.Position.Y, r.Position.Y
        h.Jump = true
        local t1 = time()
        while time() - t1 < 0.8 do RunService.Heartbeat:Wait(); top = math.max(top, r.Position.Y) end
        print(("jump %.3f"):format(top - y0))"#;
    let out = drive_at("", "CFrame.new(0, 3, 0)", local, 2.5, Some(144.0));
    let walk = numbers(&out, "walk");
    assert!(near(walk[0], 16.0, 0.4), "{out:?}");
    assert!(near(walk[1], 16.0, 0.01), "{out:?}");
    assert!(near(numbers(&out, "jump")[0], 7.52, 0.3), "{out:?}");
}

/// Measured: walking off a ledge stays Running for 0.1 s with no floor,
/// then falls.
#[test]
fn walking_off_a_ledge_waits_before_freefall() {
    let setup = r#"local l = Instance.new("Part")
        l.Anchored = true
        l.Size = Vector3.new(10, 10, 10)
        l.Position = Vector3.new(0, 5, 0)
        l.Parent = workspace"#;
    let local = r#"local left, fell
        h:Move(Vector3.new(1, 0, 0))
        for _ = 1, 60 do
            RunService.Heartbeat:Wait()
            if not left and h.FloorMaterial == Enum.Material.Air then left = time() end
            if not fell and h:GetState() == Enum.HumanoidStateType.Freefall then fell = time() end
        end
        print(("ledge %.3f"):format(fell - left))"#;
    let out = drive(setup, "CFrame.new(0, 13, 0)", local, 1.5);
    let wait = numbers(&out, "ledge")[0];
    assert!(near(wait, 0.1, 0.02), "{out:?}");
}

/// Measured (R6 and R15): steps up to the root part's bottom, 2 studs
/// up, are walked up; taller ones stop the body.
#[test]
fn low_steps_are_walked_up_and_tall_ones_block() {
    let setup = r#"for i, hgt in {1.5, 2.5} do
            local s = Instance.new("Part")
            s.Anchored = true
            s.Size = Vector3.new(6, hgt, 6)
            s.Position = Vector3.new(i * 20, hgt / 2, -8)
            s.Parent = workspace
        end"#;
    for (x, want_y, blocked) in [(20.0, 4.5, false), (40.0, 3.0, true)] {
        let local = r#"h:Move(Vector3.new(0, 0, -1))
            local top = 0
            for _ = 1, 40 do RunService.Heartbeat:Wait(); top = math.max(top, r.Position.Y) end
            print(("step %.3f %.3f"):format(top, r.Position.Z))"#;
        let out = drive(setup, &format!("CFrame.new({x}, 3, 0)"), local, 1.5);
        let step = numbers(&out, "step");
        assert!(near(step[0], want_y, 0.1), "{out:?}");
        assert_eq!(step[1] > -5.0, blocked, "{out:?}");
    }
}

/// Measured: up a 30° slope at WalkSpeed along it, 13.9 across.
#[test]
fn slopes_are_walked_along() {
    let setup = r#"local w = Instance.new("WedgePart")
        w.Anchored = true
        w.Size = Vector3.new(6, 20 * math.tan(math.rad(30)), 20)
        w.CFrame = CFrame.new(0, w.Size.Y / 2, -15) * CFrame.Angles(0, math.pi, 0)
        w.Parent = workspace"#;
    let local = r#"h:Move(Vector3.new(0, 0, -1))
        frames(45)
        local v = r.AssemblyLinearVelocity
        print(("slope %.3f %.3f"):format(-v.Z, v.Y))"#;
    let out = drive(setup, "CFrame.new(0, 3, 0)", local, 1.5);
    let v = numbers(&out, "slope");
    assert!(near(v[0], 13.86, 0.2), "{out:?}");
    assert!(near(v[1], 8.0, 0.3), "{out:?}");
}

/// Measured: a conveyor (an anchored part with a velocity) carries a
/// standing character at its speed.
#[test]
fn conveyors_carry_characters() {
    let setup = r#"local c = Instance.new("Part")
        c.Anchored = true
        c.Size = Vector3.new(40, 1, 10)
        c.Position = Vector3.new(0, 0.5, 0)
        c.AssemblyLinearVelocity = Vector3.new(10, 0, 0)
        c.Parent = workspace"#;
    let local = r#"frames(20)
        print(("carried %.3f"):format(r.AssemblyLinearVelocity.X))"#;
    let out = drive(setup, "CFrame.new(-10, 4, 0)", local, 1.0);
    assert!(near(numbers(&out, "carried")[0], 10.0, 0.05), "{out:?}");
}

/// Measured: a truss is climbed at 0.7 × WalkSpeed, and holds when the
/// input stops.
#[test]
fn trusses_are_climbed() {
    let setup = r#"local t = Instance.new("TrussPart")
        t.Anchored = true
        t.Size = Vector3.new(2, 20, 2)
        t.Position = Vector3.new(0, 10, -6)
        t.Parent = workspace"#;
    let local = r#"h:Move(Vector3.new(0, 0, -1))
        frames(40)
        print(("climb %.3f %s"):format(r.AssemblyLinearVelocity.Y, h:GetState().Name))
        h:Move(Vector3.zero)
        frames(10)
        local y = r.Position.Y
        frames(10)
        print(("hold %.3f"):format(r.Position.Y - y))"#;
    let out = drive(setup, "CFrame.new(0, 3, 0)", local, 1.5);
    assert!(out.iter().any(|l| l.ends_with("Climbing")), "{out:?}");
    assert!(near(numbers(&out, "climb")[0], 11.2, 0.1), "{out:?}");
    assert!(near(numbers(&out, "hold")[0], 0.0, 0.01), "{out:?}");
}

/// Measured: in terrain water a character swims, floating with its root
/// about half a stud under the surface.
#[test]
fn water_is_swum() {
    let setup = r#"workspace.Terrain:FillBlock(CFrame.new(0, 5, 0), Vector3.new(40, 10, 40), Enum.Material.Water)"#;
    let local = r#"frames(120)
        print(("swim %.3f %s"):format(r.Position.Y, h:GetState().Name))"#;
    let out = drive(setup, "CFrame.new(0, 5, 0)", local, 2.5);
    assert!(out.iter().any(|l| l.ends_with("Swimming")), "{out:?}");
    assert!(near(numbers(&out, "swim")[0], 9.55, 0.3), "{out:?}");
}

/// Measured: MoveTo stops within a stud and fires MoveToFinished(true);
/// against a wall it gives up after 8 s with false.
#[test]
fn move_to_arrives_or_gives_up() {
    let setup = r#"local w = Instance.new("Part")
        w.Anchored = true
        w.Size = Vector3.new(20, 10, 1)
        w.Position = Vector3.new(40, 5, -5)
        w.Parent = workspace"#;
    let local = r#"local t0 = time()
        h:MoveTo(Vector3.new(0, 0, -20))
        local ok = h.MoveToFinished:Wait()
        print(("arrive %.3f %.3f %s"):format(time() - t0, -20 - r.Position.Z, tostring(ok)))
        c:PivotTo(CFrame.new(40, 3, 0))
        task.wait(0.3)
        t0 = time()
        h:MoveTo(Vector3.new(40, 0, -20))
        ok = h.MoveToFinished:Wait()
        print(("blocked %.3f %s"):format(time() - t0, tostring(ok)))"#;
    let out = drive(setup, "CFrame.new(0, 3, 0)", local, 10.5);
    let arrive = numbers(&out, "arrive");
    assert!(near(arrive[0], 1.22, 0.1), "{out:?}");
    assert!(arrive[1].abs() < 1.0, "{out:?}");
    assert!(
        out.iter()
            .any(|l| l.starts_with("arrive") && l.ends_with("true")),
        "{out:?}"
    );
    assert!(near(numbers(&out, "blocked")[0], 8.0, 0.05), "{out:?}");
    assert!(
        out.iter()
            .any(|l| l.starts_with("blocked") && l.ends_with("false")),
        "{out:?}"
    );
}

/// Measured: a disabled Jumping state refuses the jump and clears Jump;
/// PlatformStand and Sit move to their states and back to Running, and a
/// jump from sitting goes straight to Running.
#[test]
fn state_requests_follow_studio() {
    let local = r#"local log = {}
        h.StateChanged:Connect(function(o, n) table.insert(log, o.Name .. ">" .. n.Name) end)
        h:SetStateEnabled(Enum.HumanoidStateType.Jumping, false)
        h.Jump = true
        frames(10)
        print("disabled " .. tostring(h.Jump) .. " " .. table.concat(log, ","))
        table.clear(log)
        h:SetStateEnabled(Enum.HumanoidStateType.Jumping, true)
        h.PlatformStand = true
        frames(5)
        h.PlatformStand = false
        frames(5)
        h.Sit = true
        frames(5)
        h.Jump = true
        frames(5)
        print("states " .. tostring(h.Sit) .. " " .. table.concat(log, ","))"#;
    let out = drive("", "CFrame.new(0, 3, 0)", local, 1.5);
    assert!(out.iter().any(|l| l == "disabled false "), "{out:?}");
    assert!(
        out.iter().any(|l| l
            == "states false Running>PlatformStanding,PlatformStanding>Running,Running>Seated,Seated>Running"),
        "{out:?}"
    );
}

/// A Humanoid the server simulates (a copy of a character) walks to
/// MoveTo too, and spawned characters take StarterPlayer's settings.
#[test]
fn server_humanoids_walk_and_characters_take_starter_settings() {
    let setup = r#"game.StarterPlayer.CharacterWalkSpeed = 24
        local p = game.Players.PlayerAdded:Wait()
        local c = p.Character or p.CharacterAdded:Wait()
        task.wait(0.5)
        c.Archivable = true
        local npc = c:Clone()
        npc.Parent = workspace
        npc:PivotTo(CFrame.new(30, 3, 0))
        task.wait(0.3)
        npc.Humanoid:MoveTo(Vector3.new(30, 0, -20))
        local ok = npc.Humanoid.MoveToFinished:Wait()
        print(("npc %s %.1f"):format(tostring(ok), c.Humanoid.WalkSpeed))"#;
    let server = Server::new(&Options::default()).unwrap();
    use_r6(&server);
    add_script(
        &server,
        "ServerScriptService",
        "Setup",
        &format!("{FLOOR}\n{setup}"),
    );
    let mut server = server;
    server.start();
    let mut session = Session::new(server, Options::default());
    session.join("A", 1).unwrap();
    session.run_for(3.0);
    let out = lines(session.server());
    assert!(out.iter().any(|l| l == "npc true 24.0"), "{out:?}");
}

/// A camera following the character from the render step, as Roblox's
/// does, is drawn exactly where the character is drawn: the render step
/// comes after the frame's physics, so both show the same pose.
#[test]
fn a_render_step_camera_keeps_up_with_its_character() {
    let mut server = Server::new(&Options::default()).unwrap();
    use_r6(&server);
    add_script(&server, "ServerScriptService", "Setup", FLOOR);
    add_local_script(
        &server,
        "Follow",
        r#"local p = game.Players.LocalPlayer
        local c = p.Character or p.CharacterAdded:Wait()
        local h = c:WaitForChild("Humanoid")
        local r = c:WaitForChild("HumanoidRootPart")
        local camera = workspace.CurrentCamera
        camera.CameraType = Enum.CameraType.Scriptable
        game:GetService("RunService"):BindToRenderStep("follow", 200, function()
            camera.CFrame = CFrame.new(r.Position + Vector3.new(0, 5, 10))
        end)
        task.wait(0.5)
        h:Move(Vector3.new(0, 0, -1))"#,
    );
    server.start();
    let mut session = Session::new(server, Options::default());
    session.join("A", 1).unwrap();
    session.run_for(1.5);
    let client = session.client(0).unwrap();
    let w = client.runtime().world();
    let camera = client.current_camera().unwrap();
    let local = w.dom.local_player().unwrap();
    let character = w.dom.ref_property(local, "Character").unwrap();
    let root = w
        .dom
        .find_first_child(character, "HumanoidRootPart", false)
        .unwrap();
    let speed = w
        .dom
        .vector3_property(root, "AssemblyLinearVelocity")
        .unwrap()
        .length();
    let gap = w.dom.cframe_property(camera, "CFrame").unwrap().position
        - w.dom.cframe_property(root, "CFrame").unwrap().position;
    assert!(speed > 15.0, "walking at {speed}");
    assert!(
        (gap - glam::Vec3::new(0.0, 5.0, 10.0)).length() < 1e-3,
        "{gap}"
    );
}

/// Measured: with `RotationType` CameraRelative (what the default camera
/// sets in first person and shift lock), the character turns to face the
/// camera's heading within a frame or two, standing still.
#[test]
fn camera_relative_rotation_faces_the_camera() {
    let local = r#"UserSettings():GetService("UserGameSettings").RotationType = Enum.RotationType.CameraRelative
        local camera = workspace.CurrentCamera
        camera.CameraType = Enum.CameraType.Scriptable
        RunService:BindToRenderStep("pin", 200, function()
            camera.CFrame = CFrame.new(r.Position) * CFrame.Angles(0, math.rad(90), 0)
        end)
        frames(3)
        local _, yaw = r.CFrame:ToEulerAnglesYXZ()
        print(("yaw %.2f"):format(math.deg(yaw)))"#;
    let out = drive("", "CFrame.new(0, 3, 0)", local, 1.0);
    assert!(near(numbers(&out, "yaw")[0], 90.0, 1.0), "{out:?}");
}

/// A client drawn at 144 Hz draws its walking character the same
/// distance every frame, between its 240 Hz steps, with the camera
/// following what's drawn; scripts still see whole steps (measured in
/// Studio: a part moved 3, 4 or 5 steps a frame, never part of one).
#[test]
fn a_144_hz_client_draws_its_character_moving_evenly() {
    let mut server = Server::new(&Options::default()).unwrap();
    use_r6(&server);
    add_script(&server, "ServerScriptService", "Setup", FLOOR);
    add_local_script(
        &server,
        "Walk",
        r#"local p = game.Players.LocalPlayer
        local c = p.Character or p.CharacterAdded:Wait()
        local h = c:WaitForChild("Humanoid")
        local r = c:WaitForChild("HumanoidRootPart")
        local camera = workspace.CurrentCamera
        camera.CameraType = Enum.CameraType.Scriptable
        game:GetService("RunService"):BindToRenderStep("follow", 200, function()
            camera.CFrame = CFrame.new(r.Position + Vector3.new(0, 5, 10))
        end)
        task.wait(0.5)
        h:Move(Vector3.new(0, 0, -1))"#,
    );
    server.start();
    let mut session = Session::new(server, Options::default());
    session.join("A", 1).unwrap();
    let hz = 144.0;
    let (mut server_time, mut client_time) = (0.0, 0.0);
    let mut drawn = Vec::new();
    let mut seen = Vec::new();
    while client_time < 1.5 {
        client_time += 1.0 / hz;
        while server_time < client_time {
            server_time += 1.0 / 60.0;
            session.step_server();
        }
        session.step_clients_by(1.0 / hz);
        let client = session.client(0).unwrap();
        let w = client.runtime().world();
        let root = rook_lua::character_root(&w.dom, w.dom.local_player().unwrap()).unwrap();
        let camera = client.current_camera().unwrap();
        let at = w.dom.drawn_cframe(root).unwrap().position;
        let gap = w.dom.drawn_cframe(camera).unwrap().position - at;
        drawn.push((at.z, gap));
        seen.push(w.dom.cframe_property(root, "CFrame").unwrap().position.z);
    }
    // The last half second, walking at 16 studs a second.
    let last = &drawn[drawn.len() - 72..];
    for pair in last.windows(2) {
        let moved = pair[0].0 - pair[1].0;
        assert!((moved - 16.0 / hz as f32).abs() < 0.005, "{moved}");
        assert!((pair[1].1 - glam::Vec3::new(0.0, 5.0, 10.0)).length() < 1e-3);
    }
    let step = 16.0 / 240.0;
    for pair in seen[seen.len() - 72..].windows(2) {
        let steps = (pair[0] - pair[1]) / step;
        assert!((steps - steps.round()).abs() < 0.01, "{steps} steps");
    }
}
