use super::*;

#[test]
fn wait_and_task_ordering() {
    expect(
        r#"
        print("start")
        task.defer(function() print("deferred") end)
        task.spawn(function() print("spawned") end)
        delay(0.1, function() print("delayed") end)
        local dt = wait(0.05)
        print("waited", dt >= 0.05)
        local t = task.wait()
        print("task.wait", t >= 0)
        "#,
        20,
        &[
            "start",
            "spawned",
            "deferred",
            "waited true",
            "task.wait true",
            "delayed",
        ],
    );
}

#[test]
fn signals_fire_immediately() {
    expect(
        r#"
        local f = Instance.new("Folder")
        f.ChildAdded:Connect(function(c) print("added", c.Name) end)
        local conn = f.Changed:Connect(function(prop) print("changed", prop) end)
        f:GetPropertyChangedSignal("Name"):Connect(function() print("name now", f.Name) end)
        f.Name = "Stuff"
        conn:Disconnect()
        print(conn.Connected)
        f.Name = "Other"
        local v = Instance.new("IntValue")
        v.Changed:Connect(function(new) print("value", new) end)
        v.Value = 5
        v.Name = "ignored"
        Instance.new("Part").Parent = f
        local once = 0
        f.ChildRemoved:Once(function() once += 1 end)
        f:ClearAllChildren()
        print("once", once)
        "#,
        0,
        &[
            "changed Name",
            "name now Stuff",
            "false",
            "name now Other",
            "value 5",
            "added Part",
            "once 1",
        ],
    );
}

/// In Deferred mode, disconnecting drops the connection's invocations
/// still waiting to run, and leaves the threads waiting on timers.
#[test]
fn deferred_disconnect_drops_pending_invocations() {
    let rt = Runtime::new(Dom::new(), Clock::Fixed { step: 1.0 / 60.0 }).unwrap();
    rt.world_mut().signal_behavior = SignalBehavior::Deferred;
    rt.run_source(
        "Workspace.Script",
        r#"
        local b = Instance.new("BindableEvent")
        local conn = b.Event:Connect(function(n) print("fired", n) end)
        b.Event:Connect(function(n) print("kept", n) end)
        task.delay(0, function() print("waited") end)
        b:Fire(1)
        conn:Disconnect()
        "#,
        None,
    )
    .unwrap();
    for _ in 0..2 {
        step(&rt);
    }
    let out: Vec<String> = rt.take_log().into_iter().map(|e| e.message).collect();
    assert_eq!(out, ["kept 1", "waited"]);
}

/// Measured: a wait too long for a 32-bit float (`math.huge`, past about
/// 3.4e38, NaN) lasts one step; 1e20 really waits.
#[test]
fn waits_too_long_for_a_float_last_one_step() {
    let mut out = run(
        r#"
        task.spawn(function() wait(math.huge) print("wait huge") end)
        task.spawn(function() wait(0/0) print("wait nan") end)
        task.spawn(function() task.wait(math.huge) print("task.wait huge") end)
        task.spawn(function() task.wait(1e20) print("task.wait 1e20") end)
        delay(math.huge, function() print("delay huge") end)
        task.delay(3.5e38, function() print("task.delay 3.5e38") end)
        task.delay(3.4e38, function() print("task.delay 3.4e38") end)
        "#,
        10,
    );
    out.sort();
    assert_eq!(
        out,
        [
            "delay huge",
            "task.delay 3.5e38",
            "task.wait huge",
            "wait huge",
            "wait nan",
        ]
    );
}

#[test]
fn event_wait_resumes_with_arguments() {
    expect(
        r#"
        local b = Instance.new("BindableEvent")
        task.delay(0.1, function() b:Fire("hello", 42) end)
        local a, n = b.Event:Wait()
        print(a, n)
        "#,
        10,
        &["hello 42"],
    );
}

#[test]
fn wait_for_child() {
    expect(
        r#"
        task.delay(0.2, function()
            local p = Instance.new("Part")
            p.Name = "Late"
            p.Parent = workspace
        end)
        local p = workspace:WaitForChild("Late")
        print("got", p.Name)
        print(workspace:WaitForChild("Never", 0.1))
        "#,
        30,
        &["got Late", "nil"],
    );
}

#[test]
fn wait_for_child_warns() {
    let out = run(r#"workspace:WaitForChild("Missing")"#, 6 * 60);
    assert_eq!(
        out,
        ["WARN Infinite yield possible on 'Workspace:WaitForChild(\"Missing\")'"]
    );
}
