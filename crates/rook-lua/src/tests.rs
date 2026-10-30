use super::*;

/// Run `source` as a script, step `frames` 60 Hz frames, and return the
/// output lines (errors prefixed with `ERR `, warnings with `WARN `).
fn run(source: &str, frames: u32) -> Vec<String> {
    let rt = Runtime::new(Dom::new(), Clock::Fixed { step: 1.0 / 60.0 }).unwrap();
    rt.run_source("Workspace.Script", source, None).unwrap();
    for _ in 0..frames {
        step(&rt);
    }
    rt.take_log()
        .into_iter()
        .map(|e| match e.kind {
            MessageType::Error => format!("ERR {}", e.message),
            MessageType::Warning => format!("WARN {}", e.message),
            _ => e.message,
        })
        .collect()
}

fn step(rt: &Runtime) {
    let dt = rt.world_mut().scheduler.begin_frame();
    rt.render_step(dt);
    rt.restyle();
    rt.resume_due(Queue::Legacy);
    let rs = rt.world().dom.find_service("RunService");
    if let Some(rs) = rs {
        rt.fire_event(rs, "Heartbeat", dt);
        rt.drain_deferred();
    }
    rt.resume_due(Queue::Task);
}

#[track_caller]
fn expect(source: &str, frames: u32, lines: &[&str]) {
    let out = run(source, frames);
    assert_eq!(out, lines, "\nsource:\n{source}");
}

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

#[test]
fn errors_are_plain_strings() {
    expect(
        r#"
        local ok, e = pcall(function() return workspace.Nope end)
        print(ok, type(e), e)
        ok, e = pcall(function() Instance.new("Part").Size = "big" end)
        print(e)
        ok, e = pcall(function() Instance.new("NotAClass") end)
        print(e)
        ok, e = pcall(function() workspace.Parent = nil end)
        print(e)
        "#,
        0,
        &[
            "false string Nope is not a valid member of Workspace \"Workspace\"",
            "Unable to assign property Size. Vector3 expected, got string",
            "Unable to create an Instance of type \"NotAClass\"",
            "The Parent property of Workspace is locked, current parent: Game, new parent NULL",
        ],
    );
}

#[test]
fn uncaught_errors_report_with_stack() {
    let out = run("local x = nil\nprint(x.y)", 0);
    assert_eq!(out.len(), 1);
    assert!(
        out[0].starts_with("ERR Workspace.Script:2: attempt to index nil with 'y'"),
        "{out:?}"
    );
    assert!(
        out[0].contains("Stack Begin\nScript 'Workspace.Script', Line 2"),
        "{out:?}"
    );
}

#[test]
fn engine_calls_pass_results_and_errors_through() {
    // No results, several results, a callback's results (the shim calls it
    // from Lua), instance arrays, and string errors from `inst.Key` both ways.
    expect(
        r##"
        local m = Instance.new("Model")
        local p = Instance.new("Part")
        p.Parent = m
        print(select("#", m:GetBoundingBox()), select("#", Instance.new("Folder"):Destroy()), select("#", warn()))
        local f = Instance.new("BindableFunction")
        f.OnInvoke = function(a, b) return b, a, nil end
        print(select("#", f:Invoke(1, 2)), f:Invoke(1, 2))
        local kids = m:GetChildren()
        print(#kids, kids[1] == p, m:GetChildren()[1] == kids[1])
        print(pcall(function() return m.Nope end))
        print(pcall(function() m.Nope = 1 end))
        print(pcall(function() return m[1] end))
        print(m["Part"] == p, rawequal(m.Destroy, p.Destroy))
        "##,
        0,
        &[
            "WARN ",
            "2 0 0",
            "3 2 1 nil",
            "1 true true",
            "false Nope is not a valid member of Model \"Model\"",
            "false Nope is not a valid member of Model \"Model\"",
            "false 1 is not a valid member of Model \"Model\"",
            "true true",
        ],
    );
}

#[test]
fn legacy_lowercase_members() {
    expect(
        r#"
        local m = Instance.new("Model", workspace)
        local p = Instance.new("Part")
        p.Parent = m
        print(m:findFirstChild("Part") == p, #m:children(), #m:getChildren())
        local c = p:clone()
        print(c.Parent, c.Name)
        p:remove()
        print(p.Parent, #m:GetChildren())
        print(game:service("Workspace") == workspace)
        print(game.workspace == workspace, game.Workspace == workspace)
        print(m.className, p.className)
        local lighting = game:GetService("Lighting")
        print(game.lighting == lighting)
        local s = Instance.new("Sound")
        s.Playing = true
        local h = Instance.new("Humanoid")
        h.maxHealth = 50
        print(s.isPlaying, h.MaxHealth, h.maxHealth, p.brickColor == p.BrickColor)
        "#,
        0,
        &[
            "true 1 1",
            "nil Part",
            "nil 0",
            "true",
            "true true",
            "Model Part",
            "true",
            "true 50 50 true",
        ],
    );
}

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

#[test]
fn datatypes() {
    expect(
        r#"
        local v = Vector3.new(1, 2, 3)
        print(v + Vector3.new(1, 1, 1), v * 2, v.Magnitude > 3.7, math.round(v.Unit.Magnitude), typeof(v))
        print(v:Dot(Vector3.xAxis), v:Cross(Vector3.yAxis))
        local cf = CFrame.new(1, 2, 3) * CFrame.Angles(0, math.pi / 2, 0)
        print(cf.Position, (cf * Vector3.new(0, 0, -1)):FuzzyEq(Vector3.new(0, 2, 3), 1e-5))
        print(cf:Inverse() * cf == CFrame.new() or (cf:Inverse() * cf).Position.Magnitude < 1e-5)
        print(Color3.new(1, 0, 0), Color3.fromRGB(255, 128, 0).G)
        print(BrickColor.new("Bright red"), BrickColor.new("Bright red").Number)
        print(UDim2.new(0.5, 10, 1, -5), UDim2.fromScale(1, 1).X.Scale)
        print(Enum.Material.Plastic, Enum.Material.Plastic.Value, Enum.Material.Plastic == Enum.Material.Plastic)
        local p = Instance.new("Part")
        p.Material = "Neon"
        print(p.Material, p.Material == Enum.Material.Neon)
        p.Position = Vector3.new(5, 6, 7)
        print(p.CFrame.Position, p.Position)
        p.BrickColor = BrickColor.new("Really red")
        print(p.BrickColor, p.Color)
        "#,
        0,
        &[
            "2, 3, 4 2, 4, 6 true 1 Vector3",
            "1 -3, 0, 1",
            "1, 2, 3 true",
            "true",
            "1, 0, 0 0.501960813999176",
            "Bright red 21",
            "{0.5, 10}, {1, -5} 1",
            "Enum.Material.Plastic 256 true",
            "Enum.Material.Neon true",
            "5, 6, 7 5, 6, 7",
            "Really red 1, 0, 0",
        ],
    );
}

#[test]
fn require_modules() {
    let rt = Runtime::new(Dom::new(), Clock::Fixed { step: 1.0 / 60.0 }).unwrap();
    {
        let mut w = rt.world_mut();
        let ws = w.dom.get_service("Workspace").unwrap();
        let module = w.dom.create_by_name("ModuleScript").unwrap();
        w.dom.set_name(module, "Lib".into()).unwrap();
        let source = w.dom.property_id(module, "Source").unwrap();
        w.dom
            .set_property(
                module,
                source,
                rook_types::Value::String(
                    "print('loading', script.Name) task.wait() return { answer = 42 }".into(),
                ),
            )
            .unwrap();
        w.dom.set_parent(module, Some(ws)).unwrap();
        let bad = w.dom.create_by_name("ModuleScript").unwrap();
        w.dom.set_name(bad, "Bad".into()).unwrap();
        w.dom
            .set_property(bad, source, rook_types::Value::String("return 1, 2".into()))
            .unwrap();
        w.dom.set_parent(bad, Some(ws)).unwrap();
    }
    rt.run_source(
        "Workspace.Script",
        r#"
        local a = require(workspace.Lib)
        local b = require(workspace.Lib)
        print(a.answer, a == b)
        print(pcall(require, workspace.Bad))
        "#,
        None,
    )
    .unwrap();
    for _ in 0..5 {
        step(&rt);
    }
    let out: Vec<String> = rt.take_log().into_iter().map(|e| e.message).collect();
    assert_eq!(
        out,
        [
            "loading Lib",
            "42 true",
            "Module code did not return exactly one value",
            "false Module code did not return exactly one value",
        ]
    );
}

#[test]
fn runaway_scripts_time_out() {
    let rt = Runtime::new(Dom::new(), Clock::Fixed { step: 1.0 / 60.0 }).unwrap();
    rt.set_script_timeout(Duration::from_millis(100));
    rt.run_source("Workspace.Script", "while true do end", None)
        .unwrap();
    let log = rt.take_log();
    assert_eq!(log.len(), 1);
    assert!(
        log[0]
            .message
            .contains("Script timeout: exhausted allowed execution time"),
        "{log:?}"
    );
}

#[test]
fn scripts_stop_at_the_heap_limit() {
    let rt = Runtime::new(Dom::new(), Clock::Fixed { step: 1.0 / 60.0 }).unwrap();
    rt.set_heap_limit(64 << 20);
    rt.run_source(
        "Workspace.Script",
        r#"
        local ok, e = pcall(function()
            local t = {}
            while true do table.insert(t, {}) end
        end)
        print(ok, e)
        t = nil
        -- One allocation larger than any machine has.
        print(pcall(table.create, 2^31, 0))
        "#,
        None,
    )
    .unwrap();
    let log: Vec<_> = rt.take_log().into_iter().map(|l| l.message).collect();
    assert_eq!(log.len(), 2, "{log:?}");
    assert_eq!(log[0], "false not enough memory");
    assert!(log[1].starts_with("false "), "{log:?}");
}

#[test]
fn unimplemented_members_are_counted() {
    expect(
        r#"
        local ok, e = pcall(function() return workspace:ArePartsTouchingOthers({}) end)
        print(e)
        "#,
        0,
        &["Workspace:ArePartsTouchingOthers is not implemented in Rook yet"],
    );
}

#[test]
fn services_datastore_http_bindables() {
    expect(
        r#"
        local ds = game:GetService("DataStoreService"):GetDataStore("Stats")
        print(ds == game:GetService("DataStoreService"):GetDataStore("Stats"), ds.ClassName)
        ds:SetAsync("p1", { cash = 5, items = { "sword" } })
        local v = ds:GetAsync("p1")
        v.cash = 99
        print(ds:GetAsync("p1").cash, ds:GetAsync("p1").items[1])
        print(ds:IncrementAsync("visits"), ds:IncrementAsync("visits", 4))
        print(ds:UpdateAsync("p1", function(old) old.cash += 1 return old end).cash)
        print(pcall(function() ds:SetAsync("bad", workspace) end))
        local od = game:GetService("DataStoreService"):GetOrderedDataStore("Top")
        for i, n in { 5, 1, 9 } do od:SetAsync("k" .. i, n) end
        local page = od:GetSortedAsync(false, 2):GetCurrentPage()
        print(page[1].key, page[1].value, page[2].value, #page)
        local http = game:GetService("HttpService")
        print(http:JSONEncode({ 1, 2, 3 }), http:JSONDecode('{"a":[true,null,2]}').a[3])
        print(#http:GenerateGUID(), #http:GenerateGUID(false), http:UrlEncode("a b&c"))
        local bf = Instance.new("BindableFunction")
        task.delay(0.1, function() bf.OnInvoke = function(x) return x * 2 end end)
        print("invoke", bf:Invoke(21))
        print(pcall(function() return bf.OnInvoke end))
        "#,
        20,
        &[
            "true DataStore",
            "5 sword",
            "1 5",
            "6",
            "false Cannot store Instance in data store. Data stores can only accept valid UTF-8 characters.",
            "k3 9 5 2",
            "[1,2,3] 2",
            "38 36 a%20b%26c",
            "invoke 42",
            "false OnInvoke is a callback member of BindableFunction; you can only set the callback value, get is not available",
        ],
    );
}

/// Every no-op member must be documented with what will take it over.
#[test]
fn inert_members_are_documented() {
    let doc = include_str!("../../../docs/guide/stubs.md");
    for (owner, name) in crate::api::methods::inert_members() {
        let row = format!("`{owner}:{name}`");
        let grouped = format!("`{owner}:");
        assert!(
            doc.contains(&row) || (doc.contains(&grouped) && doc.contains(&format!("`{name}`"))),
            "{owner}:{name} is inert but not listed in docs/guide/stubs.md"
        );
    }
}

#[test]
fn legacy_enum_names_resolve() {
    expect(
        r#"
        print(Enum.KeyCode.Unknown == Enum.KeyCode.None, Enum.KeyCode.Unknown.Name)
        print(Enum.SurfaceType.Bumps)
        local p = Instance.new("Part")
        p.TopSurface = "Spawn"
        print(p.TopSurface)
        "#,
        0,
        &[
            "true None",
            "Enum.SurfaceType.Glue",
            "Enum.SurfaceType.Smooth",
        ],
    );
}

/// Script environments are `safeenv`, so Luau runs builtins natively; what
/// scripts see must not change: `typeof` (Luau's builtin, which reads
/// datatypes' `__type`, the vector type's included, and ignores a proxy's),
/// globals a script assigns, and environments changed through `getfenv`.
#[test]
fn safe_environment_keeps_script_semantics() {
    expect(
        r#"
        print(typeof(Vector3.new()), typeof(game), typeof(Enum.Material.Plastic))
        local proxy = newproxy(true)
        getmetatable(proxy).__type = "Fake"
        print(typeof(proxy), typeof(CFrame.new()), typeof(nil), pcall(typeof))
        tostring = function() return "mine" end
        print(tostring(1))
        getfenv().type = function() return "custom" end
        print(type(1))
        "#,
        0,
        &[
            "Vector3 Instance EnumItem",
            "userdata CFrame nil false missing argument #1",
            "mine",
            "custom",
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
    let end = rt.world().dom.journal().end();
    rt.world_mut().dom.journal_mut().trim(end);
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

mod core_gui;
mod gui;
mod models;
mod tweens;

/// Every level of a script's stack has the script's environment, as in
/// Roblox: C functions, Rook's own Luau and level 0 included. Anti-tamper
/// scripts hang on purpose when one doesn't.
#[test]
fn getfenv_gives_the_script_environment_at_every_level() {
    expect(
        r#"
        local env = getfenv()
        xpcall(function() return game.missing end, function()
            local same = true
            for level = 0, 6 do
                same = same and getfenv(level) == env
            end
            print(same, getfenv(print) == env)
        end)
        print(pcall(getfenv, 50))
        print(pcall(getfenv, -1))
        "#,
        0,
        &[
            "true true",
            "false invalid argument #1 to 'getfenv' (invalid level)",
            "false invalid argument #1 to 'getfenv' (level must be non-negative)",
        ],
    );
}
