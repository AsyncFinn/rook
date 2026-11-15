use super::*;

#[test]
fn print_and_instances() {
    expect(
        r#"
        local p = Instance.new("Part")
        p.Name = "Brick"
        p.Parent = workspace
        print(p, p.Parent, p:GetFullName(), p.ClassName, typeof(p))
        print(workspace:FindFirstChild("Brick") == p, workspace.Brick == p)
        print(p:IsA("BasePart"), p:IsA("Model"), game:GetService("Workspace") == workspace)
        print(#workspace:GetChildren(), p.Anchored, p.Size == Vector3.new(4, 1.2, 2))
        "#,
        0,
        &[
            "Brick Workspace Workspace.Brick Part Instance",
            "true true",
            "true false true",
            "1 false true",
        ],
    );
}

/// `DescendantRemoving` steps are skipped only while nothing listens: a
/// handler connected by an earlier step of the same `Destroy` still hears
/// every descendant.
#[test]
fn descendant_removing_connected_mid_destroy() {
    expect(
        r#"
        local model = Instance.new("Model")
        Instance.new("Part", model).Name = "A"
        model.Parent = workspace
        model.Destroying:Connect(function()
            workspace.DescendantRemoving:Connect(function(d) print("removing", d.Name) end)
        end)
        model:Destroy()
        "#,
        0,
        &["removing Model", "removing A"],
    );
}

/// `inst.Name` is kept in the index shim, so every rename must reach the
/// next read: a script's own, one the engine makes while it waits, one
/// whose journal entry is trimmed before the script runs again, and one
/// made while the journal is off.
#[test]
fn renames_are_seen_by_the_next_read() {
    let rt = Runtime::new(Dom::new(), Clock::Fixed { step: 1.0 / 60.0 }).unwrap();
    rt.run_source(
        "Workspace.Script",
        r#"
        local p = Instance.new("Part")
        p.Name = "A"
        p.Parent = workspace
        -- Later reads come from the shim's copy.
        local function read()
            local name
            for _ = 1, 3 do
                name = p.Name
            end
            return name
        end
        -- No engine call between the copy, the rename and the read.
        local first = read()
        p.Name = "B"
        print(first, read())
        p.Changed:Connect(function()
            print("changed", read())
        end)
        p.Name = "C"
        task.wait()
        print(read())
        task.wait()
        print(read())
        task.wait()
        print(read())
        "#,
        None,
    )
    .unwrap();
    let rename = |from: &str, to: &str| {
        let mut w = rt.world_mut();
        let ws = w.dom.find_service("Workspace").unwrap();
        let part = w.dom.find_first_child(ws, from, false).unwrap();
        w.dom.set_name(part, to.into()).unwrap();
    };
    rename("C", "D");
    step(&rt);
    rename("D", "E");
    {
        // Trimmed as the engine trims: once its readers have read.
        let mut w = rt.world_mut();
        let w = &mut *w;
        w.gui.note_changes(&w.dom);
        w.note_style_changes();
        let end = w.dom.journal().end();
        w.dom.journal_mut().trim(end);
    }
    step(&rt);
    rt.world_mut().dom.journal_mut().set_enabled(false);
    rename("E", "F");
    rt.world_mut().dom.journal_mut().set_enabled(true);
    step(&rt);
    let log: Vec<String> = rt.take_log().into_iter().map(|e| e.message).collect();
    assert_eq!(log, ["A B", "changed C", "D", "E", "F"]);
}

/// A callback and a runtime-only value belong to the instance, not to the
/// userdata they were set through, which is collected once no script holds
/// it and made again on the next use.
#[test]
fn instance_state_outlives_its_userdata() {
    let rt = Runtime::new(Dom::new(), Clock::Fixed { step: 1.0 / 60.0 }).unwrap();
    rt.run_source(
        "Workspace.Script",
        r#"
        local f = Instance.new("BindableFunction")
        f.Name = "F"
        f.Parent = workspace
        f.OnInvoke = function()
            return "answered"
        end
        local part = Instance.new("Part")
        part.Name = "P"
        part.Parent = workspace
        f, part = nil, nil
        task.wait()
        print(workspace.F:Invoke(), workspace.P.Extra)
        "#,
        None,
    )
    .unwrap();
    // Engine Luau keeps values Rook can't store this way (`Tween.TweenInfo`).
    let part = {
        let w = rt.world();
        let ws = w.dom.find_service("Workspace").unwrap();
        w.dom.find_first_child(ws, "P", false).unwrap()
    };
    let std: mlua::Table = rt.lua().named_registry_value("rook.std").unwrap();
    let set_internal: mlua::Function = std.get("set_internal").unwrap();
    let ud = rt.push_instance(part).unwrap();
    set_internal.call::<()>((ud, "Extra", "kept")).unwrap();
    rt.lua().gc_collect().unwrap();
    rt.lua().gc_collect().unwrap();
    step(&rt);
    let log: Vec<String> = rt.take_log().into_iter().map(|e| e.message).collect();
    assert_eq!(log, ["answered kept"]);
}

/// `GetChildren` copies an array it keeps per parent, so every change to
/// the children must reach the next call: from scripts (new, reparented,
/// destroyed, cloned), from the engine while the script waits, and with
/// the journal off; and what a script does to its copy stays its own.
#[test]
fn get_children_is_never_stale() {
    let rt = Runtime::new(Dom::new(), Clock::Fixed { step: 1.0 / 60.0 }).unwrap();
    rt.run_source(
        "Workspace.Script",
        r#"
        local m = Instance.new("Model")
        m.Name = "M"
        m.Parent = workspace
        local function names()
            local list = {}
            for _, c in m:GetChildren() do
                table.insert(list, c.Name)
            end
            return table.concat(list, ",")
        end
        local a = Instance.new("Part")
        a.Name = "A"
        a.Parent = m
        print(names(), names())
        local b = Instance.new("Part")
        b.Name = "B"
        b.Parent = m
        local first = names()
        a.Parent = workspace
        print(first, names())
        b:Clone().Parent = m
        print(names())
        b:Destroy()
        print(names())
        local mine = m:GetChildren()
        mine[1] = nil
        table.insert(mine, workspace)
        print(#mine, names(), mine ~= m:GetChildren())
        task.wait()
        print(names())
        task.wait()
        print(names())
        "#,
        None,
    )
    .unwrap();
    let add = |name: &str| {
        let mut w = rt.world_mut();
        let ws = w.dom.find_service("Workspace").unwrap();
        let m = w.dom.find_first_child(ws, "M", false).unwrap();
        let part = w.dom.create_by_name("Part").unwrap();
        w.dom.set_name(part, name.into()).unwrap();
        w.dom.set_parent(part, Some(m)).unwrap();
    };
    add("C");
    step(&rt);
    rt.world_mut().dom.journal_mut().set_enabled(false);
    add("D");
    rt.world_mut().dom.journal_mut().set_enabled(true);
    step(&rt);
    let log: Vec<String> = rt.take_log().into_iter().map(|e| e.message).collect();
    assert_eq!(
        log,
        ["A A", "A,B B", "B,B", "B", "1 B true", "B,C", "B,C,D"]
    );
}
