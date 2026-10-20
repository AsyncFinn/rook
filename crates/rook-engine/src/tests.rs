use super::*;

fn add_script(server: &Server, location: &str, name: &str, source: &str) {
    let parent = service(server, location);
    add_under(server, parent, "Script", name, source);
}

fn lines(server: &Server) -> Vec<String> {
    server.take_log().into_iter().map(|e| e.message).collect()
}

#[test]
fn scripts_run_in_server_locations_only() {
    let mut s = Server::new(&Options::default()).unwrap();
    add_script(
        &s,
        "Workspace",
        "Script",
        "print('ws', script:GetFullName())",
    );
    add_script(&s, "ServerStorage", "Stored", "print('should not run')");
    s.start();
    assert_eq!(lines(&s), ["ws Workspace.Script"]);
}

#[test]
fn heartbeat_fires_every_frame() {
    let mut s = Server::new(&Options::default()).unwrap();
    add_script(
        &s,
        "ServerScriptService",
        "Script",
        r#"
        local n = 0
        game:GetService("RunService").Heartbeat:Connect(function() n += 1 end)
        task.delay(0.5, function() print(n) end)
        "#,
    );
    s.start();
    s.run_for(1.0);
    assert_eq!(lines(&s), ["30"]);
}

fn add_under(
    server: &Server,
    parent: InstanceId,
    class: &str,
    name: &str,
    source: &str,
) -> InstanceId {
    let mut w = server.runtime.world_mut();
    let id = w.dom.create_by_name(class).unwrap();
    w.dom.set_name(id, name.into()).unwrap();
    if !source.is_empty() {
        w.dom
            .set_property_by_name(id, "Source", Value::String(source.into()))
            .unwrap();
    }
    w.dom.set_parent(id, Some(parent)).unwrap();
    id
}

fn service(server: &Server, name: &str) -> InstanceId {
    server.runtime.world_mut().dom.get_service(name).unwrap()
}

/// A script changing a part's mesh (its type or Scale, or adding or
/// removing one) changes how the part looks, so the part is redrawn.
#[test]
fn mesh_and_decal_changes_mark_their_part() {
    let mut s = Server::new(&Options::default()).unwrap();
    let ws = service(&s, "Workspace");
    let part = add_under(&s, ws, "Part", "Part", "");
    add_under(&s, part, "SpecialMesh", "Mesh", "");
    let other = add_under(&s, ws, "Part", "Other", "");
    s.start();
    s.take_render_changes();
    let changed = |s: &mut Server, source: &str| {
        s.run_extra_script(source).unwrap();
        let changes = s.take_render_changes();
        assert!(!changes.full, "{source}");
        changes.parts
    };
    let only_part = std::collections::HashSet::from([part]);
    for source in [
        "workspace.Part.Mesh.MeshType = Enum.MeshType.Sphere",
        "workspace.Part.Mesh.Scale = Vector3.new(1, 2, 1)",
        "Instance.new('CylinderMesh', workspace.Part)",
        "workspace.Part:FindFirstChildOfClass('CylinderMesh').Parent = nil",
        "workspace.Part.Mesh:Destroy()",
        "Instance.new('Decal', workspace.Part)",
        "workspace.Part.Decal.Transparency = 0.5",
        "Instance.new('Texture', workspace.Part).StudsPerTileU = 3",
        "workspace.Part.Decal:Destroy()",
    ] {
        assert_eq!(changed(&mut s, source), only_part, "{source}");
    }
    // Moving a mesh between parts redraws both; other children don't
    // count.
    let block = add_under(&s, other, "BlockMesh", "Block", "");
    s.take_render_changes();
    s.runtime
        .world_mut()
        .dom
        .set_parent(block, Some(part))
        .unwrap();
    assert_eq!(
        s.take_render_changes().parts,
        std::collections::HashSet::from([part, other])
    );
    assert!(changed(&mut s, "Instance.new('Folder', workspace.Part)").is_empty());
}

/// A 1 x 1 red PNG.
const PNG: [u8; 70] = [
    137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 1, 0, 0, 0, 1, 8, 6, 0,
    0, 0, 31, 21, 196, 137, 0, 0, 0, 13, 73, 68, 65, 84, 120, 156, 99, 248, 207, 192, 240, 31, 0,
    5, 0, 1, 255, 137, 153, 61, 29, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130,
];

/// Options whose content comes from a synthetic install holding
/// `textures/face.png` and `sounds/hit.wav`, with no network or cache.
fn content_options(name: &str) -> Options {
    let dir = std::env::temp_dir()
        .join("rook-engine-tests")
        .join(format!("{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(dir.join("content/textures")).unwrap();
    std::fs::create_dir_all(dir.join("content/sounds")).unwrap();
    std::fs::write(dir.join("content/textures/face.png"), PNG).unwrap();
    std::fs::write(dir.join("content/sounds/hit.wav"), b"wav").unwrap();
    let mut config = rook_content::Config::empty();
    config.install = rook_content::Install::at(&dir, rook_content::InstallKind::Custom);
    Options {
        content: Some(Arc::new(rook_content::Resolver::new(config))),
        ..Options::default()
    }
}

/// Step until `done` holds (content resolves on other threads).
fn step_until(dm: &mut DataModel, mut done: impl FnMut(&[String]) -> bool) -> Vec<String> {
    let mut out = Vec::new();
    for _ in 0..500 {
        dm.step();
        out.extend(lines(dm));
        if done(&out) {
            break;
        }
        std::thread::sleep(Duration::from_millis(2));
    }
    out
}

mod adornments;
mod assets;
mod audio_api;
mod client_tweens;
mod effects;
mod explosions;
mod preload;
mod scripts;
mod session;
mod sounds;
mod terrain_api;
mod terrain_look;
mod unions;
