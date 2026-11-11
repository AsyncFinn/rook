//! Starting and stopping Scripts as they move and are enabled or disabled.

use super::*;

#[test]
fn scripts_moved_into_workspace_start() {
    let mut s = Server::new(&Options::default()).unwrap();
    add_script(&s, "ServerStorage", "Inner", "print('inner started')");
    add_script(
        &s,
        "ServerScriptService",
        "Outer",
        r#"
        local inner = game:GetService("ServerStorage").Inner
        task.wait(0.1)
        print("moving")
        inner.Parent = workspace
        "#,
    );
    s.start();
    s.run_for(0.5);
    assert_eq!(lines(&s), ["moving", "inner started"]);
    assert_eq!(s.started_scripts(), 2);
}

/// A script that disables itself runs on until it yields, then its threads
/// and connections stop; enabling it again runs it afresh (old places
/// re-enable a script to replay an animation). Matches a Studio recording
/// of the same scripts (2026-10-17).
#[test]
fn disabled_scripts_stop_and_rerun_when_enabled() {
    let mut s = Server::new(&Options::default()).unwrap();
    let event = add_under(&s, service(&s, "Workspace"), "BindableEvent", "Event", "");
    let target = add_under(
        &s,
        event,
        "Script",
        "Target",
        r#"
        print("run")
        script.Parent.Event:Connect(function() print("conn") end)
        task.delay(0.3, function() print("delayed") end)
        wait(0.1)
        print("before disable")
        script.Disabled = true
        print("after disable")
        "#,
    );
    s.runtime
        .world_mut()
        .dom
        .set_property_by_name(target, "Disabled", Value::Bool(true))
        .unwrap();
    add_script(
        &s,
        "ServerScriptService",
        "Driver",
        r#"
        local ev = workspace.Event
        local s = ev.Target
        task.wait(1)
        s.Disabled = false
        task.wait(0.6)
        ev:Fire()
        print("fired")
        s.Disabled = false
        task.wait(0.05)
        print("re-enabled")
        task.wait(0.6)
        ev:Fire()
        print("fired2")
        s.Disabled = false
        task.wait(0.05)
        s.Disabled = true
        task.wait(0.6)
        ev:Fire()
        print("end")
        "#,
    );
    s.start();
    s.run_for(4.0);
    assert_eq!(
        lines(&s),
        [
            "run",
            "before disable",
            "after disable",
            "fired",
            "run",
            "re-enabled",
            "before disable",
            "after disable",
            "fired2",
            "run",
            "end",
        ]
    );
}

/// A loader that enables its game's disabled scripts as the place starts
/// (setting `Disabled` to false before anything yields) starts each of
/// them once, whether it comes before or after the loader in the tree.
#[test]
fn scripts_enabled_while_the_place_starts_run_once() {
    let mut s = Server::new(&Options::default()).unwrap();
    let sss = service(&s, "ServerScriptService");
    let early = add_under(&s, sss, "Script", "Early", "print('early')");
    add_script(
        &s,
        "ServerScriptService",
        "Loader",
        r#"
        local sss = game:GetService("ServerScriptService")
        sss.Early.Disabled = false
        sss.Late.Disabled = false
        print("loaded")
        "#,
    );
    let late = add_under(&s, sss, "Script", "Late", "print('late')");
    for id in [early, late] {
        s.runtime
            .world_mut()
            .dom
            .set_property_by_name(id, "Disabled", Value::Bool(true))
            .unwrap();
    }
    s.start();
    s.run_for(0.2);
    let mut out = lines(&s);
    out.sort();
    assert_eq!(out, ["early", "late", "loaded"]);
    assert_eq!(s.started_scripts(), 3);
}

/// A running script stops once it, or an ancestor, leaves where scripts
/// run: destroyed, parented to nil or moved to ServerStorage. Put back, it
/// runs afresh. Matches Studio 0.747 (2026-11-11), where each such
/// script's Heartbeat connection and loop stopped at once.
#[test]
fn scripts_moved_out_of_where_they_run_stop() {
    let mut s = Server::new(&Options::default()).unwrap();
    let workspace = service(&s, "Workspace");
    add_under(&s, workspace, "BindableEvent", "Event", "");
    let listen = |name: &str| {
        format!(
            "print('{name} run') workspace.Event.Event:Connect(function() print('{name} heard') end)"
        )
    };
    for name in ["Destroyed", "Orphaned", "Stored", "Back"] {
        add_under(&s, workspace, "Script", name, &listen(name));
    }
    let folder = add_under(&s, workspace, "Folder", "Folder", "");
    add_under(&s, folder, "Script", "Inside", &listen("Inside"));
    add_script(
        &s,
        "ServerScriptService",
        "Driver",
        r#"
        task.wait(0.1)
        local back = workspace.Back
        workspace.Destroyed:Destroy()
        workspace.Orphaned.Parent = nil
        workspace.Stored.Parent = game:GetService("ServerStorage")
        back.Parent = nil
        workspace.Folder:Destroy()
        task.wait(0.1)
        workspace.Event:Fire()
        print("fired")
        back.Parent = workspace
        task.wait(0.1)
        workspace.Event:Fire()
        "#,
    );
    s.start();
    s.run_for(0.5);
    let out = lines(&s);
    let (start, rest) = out.split_at(5);
    let mut start = start.to_vec();
    start.sort();
    assert_eq!(
        start,
        [
            "Back run",
            "Destroyed run",
            "Inside run",
            "Orphaned run",
            "Stored run"
        ]
    );
    assert_eq!(rest, ["fired", "Back run", "Back heard"]);
}

/// A script destroyed during a frame misses that frame's later events,
/// as in Studio, where it stops as it's destroyed.
#[test]
fn a_destroyed_script_misses_the_rest_of_the_frame() {
    let mut s = Server::new(&Options::default()).unwrap();
    add_under(
        &s,
        service(&s, "Workspace"),
        "Script",
        "Target",
        "game:GetService('RunService').Heartbeat:Connect(function() print('beat') end)",
    );
    add_script(
        &s,
        "ServerScriptService",
        "Driver",
        r#"
        wait(0.1)
        workspace.Target:Destroy()
        print("destroyed")
        "#,
    );
    s.start();
    s.run_for(0.3);
    let out = lines(&s);
    let at = out.iter().position(|l| l == "destroyed").unwrap();
    assert!(at > 0 && out[..at].iter().all(|l| l == "beat"), "{out:?}");
    assert_eq!(at + 1, out.len(), "{out:?}");
}
