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
