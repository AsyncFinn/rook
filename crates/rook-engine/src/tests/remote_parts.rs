//! Parts the server simulates, as a client shows them
//! (docs/behavior/replication.md#physics).

use super::*;

/// Measured in Studio: a client shows a part the server moves at a steady
/// 10 studs/s moving every frame by its speed times the frame, about
/// 0.146 s behind the server's newest update, and scripts read it there
/// (moved by whole steps, as they read the client's own parts). Shown as
/// updates came, a 144 Hz client saw it jump at the server's 60 Hz and
/// stand still in the frames between.
#[test]
fn a_client_plays_server_parts_back_smoothly() {
    let server = Server::new(&Options::default()).unwrap();
    add_script(
        &server,
        "ServerScriptService",
        "Mover",
        r#"local p = Instance.new("Part")
        p.Name = "Mover"
        p.Size = Vector3.new(4, 1, 4)
        p.Position = Vector3.new(0, 50, 0)
        p.CanCollide = false
        local v = Instance.new("BodyVelocity")
        v.Velocity = Vector3.new(10, 0, 0)
        v.MaxForce = Vector3.new(1e9, 1e9, 1e9)
        v.Parent = p
        p.Parent = workspace"#,
    );
    let mut server = server;
    server.start();
    let mut session = Session::new(server, Options::default());
    session.join("A", 1).unwrap();
    let hz = 144.0;
    let (mut server_time, mut client_time) = (0.0, 0.0);
    let mut shown = Vec::new();
    while client_time < 2.0 {
        client_time += 1.0 / hz;
        while server_time < client_time {
            server_time += 1.0 / 60.0;
            session.step_server();
        }
        session.step_clients_by(1.0 / hz);
        if client_time < 1.0 {
            continue;
        }
        let x = |dm: &DataModel| {
            let w = dm.runtime().world();
            let ws = w.dom.find_service("Workspace").unwrap();
            let p = w.dom.find_first_child(ws, "Mover", false).unwrap();
            let read = w.dom.cframe_property(p, "CFrame").unwrap().position.x;
            (read, w.dom.drawn_cframe(p).unwrap().position.x)
        };
        let (read, drawn) = x(session.client(0).unwrap());
        let (at_server, _) = x(session.server());
        shown.push((read, drawn, at_server));
    }
    for pair in shown.windows(2) {
        let steps = (pair[1].0 - pair[0].0) / (10.0 / 240.0);
        assert!(
            (steps - 1.0).abs() < 0.05 || (steps - 2.0).abs() < 0.05,
            "read {steps} steps on in a frame: {shown:?}"
        );
        let drawn = pair[1].1 - pair[0].1;
        assert!(
            (drawn - 10.0 / 144.0).abs() < 0.002,
            "drawn {drawn} in a frame"
        );
    }
    let behind: f32 = shown
        .iter()
        .map(|(read, _, server)| (server - read) / 10.0)
        .sum::<f32>()
        / shown.len() as f32;
    assert!(
        (behind - 0.146).abs() < 0.02,
        "{behind} s behind the server"
    );
}

/// A character standing on a platform the server moves rides it, as on
/// Roblox: the client's controller takes the platform's velocity from its
/// played-back motion. Shown as updates came, the platform stood still
/// between them with no velocity, and slid out from under the character.
#[test]
fn a_character_rides_a_server_platform() {
    let server = Server::new(&Options::default()).unwrap();
    use_r6(&server);
    add_script(
        &server,
        "ServerScriptService",
        "Platform",
        r#"local p = Instance.new("Part")
        p.Name = "Platform"
        p.Size = Vector3.new(12, 1, 12)
        p.Position = Vector3.new(0, 49.5, 0)
        local v = Instance.new("BodyVelocity")
        v.Velocity = Vector3.zero
        v.MaxForce = Vector3.new(1e9, 1e9, 1e9)
        v.Parent = p
        p.Parent = workspace
        game.Players.PlayerAdded:Connect(function(player)
            player.CharacterAdded:Connect(function(c)
                c:WaitForChild("HumanoidRootPart")
                c:PivotTo(CFrame.new(0, 53, 0))
                task.wait(1)
                v.Velocity = Vector3.new(6, 0, 0)
            end)
        end)"#,
    );
    let mut server = server;
    server.start();
    let mut session = Session::new(server, Options::default());
    session.join("A", 1).unwrap();
    let place = |session: &Session| {
        let w = session.client(0).unwrap().runtime().world();
        let ws = w.dom.find_service("Workspace").unwrap();
        let platform = w.dom.find_first_child(ws, "Platform", false).unwrap();
        let root = rook_lua::character_root(&w.dom, w.dom.local_player().unwrap()).unwrap();
        let x = |id| w.dom.cframe_property(id, "CFrame").unwrap().position.x;
        (x(platform), x(root))
    };
    session.run_for(2.0);
    let (p0, r0) = place(&session);
    session.run_for(1.0);
    let (p1, r1) = place(&session);
    assert!((p1 - p0 - 6.0).abs() < 0.2, "platform moved {}", p1 - p0);
    assert!(
        (r1 - r0 - (p1 - p0)).abs() < 0.2,
        "rode {} of {}",
        r1 - r0,
        p1 - p0
    );
}

/// Physics journals only the root of an assembly that moves whole, and
/// replication sends only that: the rest of a welded assembly still moves
/// on the client, read and drawn where the weld holds it.
#[test]
fn a_clients_copy_of_a_server_assembly_moves_whole() {
    let server = Server::new(&Options::default()).unwrap();
    add_script(
        &server,
        "ServerScriptService",
        "Mover",
        r#"local function part(name, x)
            local p = Instance.new("Part")
            p.Name = name
            p.Size = Vector3.new(4, 1, 4)
            p.Position = Vector3.new(x, 50, 0)
            p.CanCollide = false
            p.Parent = workspace
            return p
        end
        local base, arm = part("Base", 0), part("Arm", 6)
        local weld = Instance.new("WeldConstraint")
        weld.Part0, weld.Part1 = base, arm
        weld.Parent = base
        local v = Instance.new("BodyVelocity")
        v.Velocity = Vector3.new(10, 0, 0)
        v.MaxForce = Vector3.new(1e9, 1e9, 1e9)
        v.Parent = base"#,
    );
    let mut server = server;
    server.start();
    let mut session = Session::new(server, Options::default());
    session.join("A", 1).unwrap();
    let at = |dm: &DataModel| {
        let w = dm.runtime().world();
        let ws = w.dom.find_service("Workspace").unwrap();
        let x = |name| {
            let p = w.dom.find_first_child(ws, name, false).unwrap();
            let read = w.dom.cframe_property(p, "CFrame").unwrap().position.x;
            (read, w.dom.drawn_cframe(p).unwrap().position.x)
        };
        (x("Base"), x("Arm"))
    };
    for _ in 0..30 {
        session.step();
    }
    let ((base_before, _), (arm_before, _)) = at(session.client(0).unwrap());
    for _ in 0..60 {
        session.step();
    }
    let client = session.client_mut(0).unwrap();
    let ((base, base_drawn), (arm, arm_drawn)) = at(client);
    assert!(
        (arm - base - 6.0).abs() < 1e-3,
        "read: base {base}, arm {arm}"
    );
    assert!(
        (arm_drawn - base_drawn - 6.0).abs() < 1e-3,
        "drawn: base {base_drawn}, arm {arm_drawn}"
    );
    assert!(
        arm - arm_before > 5.0,
        "the arm moved from {arm_before} to {arm}"
    );
    assert!(base - base_before > 5.0);
    // Its move reaches a renderer, though only the root's was journaled.
    let ws = client
        .runtime()
        .world()
        .dom
        .find_service("Workspace")
        .unwrap();
    let arm_id = (client.runtime().world().dom)
        .find_first_child(ws, "Arm", false)
        .unwrap();
    let changes = client.take_render_changes();
    assert!(changes.moved.contains(&arm_id) || changes.parts.contains(&arm_id) || changes.full);
}
