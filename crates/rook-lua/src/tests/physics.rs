use crate::tests::*;

/// Writing a NaN or infinite number into a part's pose, size or velocity
/// (a 2017-era disaster script does) leaves the part where Studio 0.744
/// puts it, and nothing a script reads back is NaN.
#[test]
fn nan_writes_read_back_as_studio_has_them() {
    expect(
        r#"
        local nan, inf = 0/0, math.huge
        local function part()
            local p = Instance.new("Part")
            p.Anchored = true
            p.Size = Vector3.new(4, 1, 2)
            p.Position = Vector3.new(3, 20, 7)
            p.Parent = workspace
            return p
        end
        local a = part(); a.CFrame = CFrame.new(nan, 5, 0)
        local b = part(); b.CFrame = CFrame.new(1, 5, 2) * CFrame.Angles(nan, 0, 0)
        local c = part(); c.Position = Vector3.new(1, inf, 1)
        local d = part(); d.Orientation = Vector3.new(nan, 0, 0)
        local e = part(); e.CFrame = CFrame.new(1e30, 5, 0)
        print(Instance.new("Part").Orientation, Instance.new("Part").Rotation, CFrame.new(0, -1000000, 0) == d.CFrame)
        print(a.Position, b.Position, c.Position, d.Position, d.Orientation, e.Position.X > 1e29)
        local s = part(); s.Size = Vector3.new(nan, 1, 1); s.Size = Vector3.new(inf, 3, 3)
        local v = part(); v.Anchored = false
        v.AssemblyLinearVelocity = Vector3.new(nan, 5, 0)
        v.AssemblyAngularVelocity = Vector3.new(0, inf, 0)
        print(s.Size, v.AssemblyLinearVelocity, v.AssemblyAngularVelocity)
        "#,
        0,
        &[
            "0, 0, 0 0, 0, 0 true",
            "0, -1000000, 0 0, -1000000, 0 0, -1000000, 0 0, -1000000, 0 0, 0, 0 true",
            "4, 1, 2 0, 0, 0 0, 0, 0",
        ],
    );
}

/// A WorldModel's queries find its own parts and Workspace's don't
/// (docs/behavior/physics/queries.md#worldmodels).
#[test]
fn a_world_models_raycast_finds_its_own_parts() {
    expect(
        r#"
        local wm = Instance.new("WorldModel")
        wm.Parent = game:GetService("ReplicatedStorage")
        local p = Instance.new("Part")
        p.Anchored = true
        p.Size = Vector3.new(4, 4, 4)
        p.Position = Vector3.new(0, 500, 0)
        p.Parent = wm
        local down = Vector3.new(0, -20, 0)
        local hit = wm:Raycast(Vector3.new(0, 510, 0), down)
        print(hit and hit.Instance == p, hit and hit.Position)
        print(workspace:Raycast(Vector3.new(0, 510, 0), down))
        print(#wm:GetPartBoundsInBox(CFrame.new(0, 500, 0), Vector3.one * 10))
        "#,
        0,
        &["true 0, 502, 0", "nil", "1"],
    );
}

/// A WeldConstraint keeps where its parts were when a script welded them,
/// in its hidden CFrame0 (Part1's pose in Part0's frame), and again when
/// one of them is moved on its own by Position; moving one by CFrame
/// carries the other along and keeps it.
#[test]
fn weld_constraints_keep_their_offset() {
    let rt = Runtime::new(Dom::new(), Clock::Fixed { step: 1.0 / 60.0 }).unwrap();
    rt.run_source(
        "Workspace.Script",
        r#"local base = Instance.new("Part")
        base.Name = "Base"
        base.Anchored = true
        base.Parent = workspace
        local arm = base:Clone()
        arm.Name = "Arm"
        arm.Anchored = false
        arm.Position = Vector3.new(6, 0, 0)
        arm.Parent = workspace
        local weld = Instance.new("WeldConstraint")
        weld.Part0, weld.Part1 = base, arm
        weld.Parent = base"#,
        None,
    )
    .unwrap();
    let offset = |rt: &Runtime| {
        let w = rt.world();
        let ws = w.dom.find_service("Workspace").unwrap();
        let base = w.dom.find_first_child(ws, "Base", false).unwrap();
        let weld = w
            .dom
            .find_first_child(base, "WeldConstraint", false)
            .unwrap();
        w.dom.cframe_property(weld, "CFrame0").unwrap().position
    };
    assert_eq!(offset(&rt), glam::Vec3::new(6.0, 0.0, 0.0));
    rt.run_source(
        "Workspace.Move",
        "workspace.Arm.Position = Vector3.new(0, 0, 3)",
        None,
    )
    .unwrap();
    assert_eq!(offset(&rt), glam::Vec3::new(0.0, 0.0, 3.0));
    rt.run_source(
        "Workspace.Carry",
        "workspace.Base.CFrame = CFrame.new(10, 0, 0) print(workspace.Arm.Position)",
        None,
    )
    .unwrap();
    assert_eq!(offset(&rt), glam::Vec3::new(0.0, 0.0, 3.0));
    let printed: Vec<String> = rt.take_log().into_iter().map(|e| e.message).collect();
    assert_eq!(printed, ["10, 0, 3"]);
}

/// `ExcludeInstances` and `IncludeInstances` with the legacy filter, as
/// measured in Studio 0.748: every list narrows the candidates (the
/// lists intersect, exclusion wins), descendants count, and an assigned
/// table is copied.
#[test]
fn raycast_include_and_exclude_lists() {
    expect(
        r#"
        local function wall(name, z, parent)
            local p = Instance.new("Part")
            p.Name = name
            p.Anchored = true
            p.Size = Vector3.new(10, 10, 1)
            p.CFrame = CFrame.new(0, 50, z)
            p.Parent = parent or workspace
            return p
        end
        local A = wall("A", -10)
        local B = wall("B", -20)
        local F = Instance.new("Folder")
        F.Parent = workspace
        local C = wall("C", -30, F)
        local function cast(p)
            local r = workspace:Raycast(Vector3.new(0, 50, 0), Vector3.new(0, 0, -100), p)
            return r and r.Instance.Name or "nil"
        end
        local p = RaycastParams.new()
        print(p.ExcludeInstances, p.IncludeInstances)
        p.FilterType = Enum.RaycastFilterType.Include
        p.FilterDescendantsInstances = { A, B, F }
        p.ExcludeInstances = { A }
        print(cast(p))
        local t = {}
        p.ExcludeInstances = t
        table.insert(t, A)
        print(cast(p), rawequal(p.ExcludeInstances, t))
        local q = RaycastParams.new()
        q.ExcludeInstances = { A, B, F }
        print(cast(q))
        q.ExcludeInstances = { A, B }
        print(cast(q))
        local r = RaycastParams.new()
        r.IncludeInstances = { B, F }
        print(cast(r))
        r.IncludeInstances = {}
        print(cast(r))
        r.IncludeInstances = { B, F }
        r.ExcludeInstances = { B }
        print(cast(r))
        local s = RaycastParams.new()
        s.FilterDescendantsInstances = { A }
        s.IncludeInstances = { A, B }
        print(cast(s))
        local v = RaycastParams.new()
        v.FilterType = Enum.RaycastFilterType.Include
        v.FilterDescendantsInstances = { F }
        v.IncludeInstances = { B }
        print(cast(v))
        r.ExcludeInstances = nil
        print(r.ExcludeInstances, cast(r))
        "#,
        1,
        &[
            "nil nil", "B", "A false", "nil", "C", "B", "nil", "C", "B", "nil", "nil B",
        ],
    );
}

/// `OverlapParams` take the same lists: an overlap finds only what they
/// leave in.
#[test]
fn overlap_include_and_exclude_lists() {
    expect(
        r#"
        local function block(name, x)
            local p = Instance.new("Part")
            p.Name = name
            p.Anchored = true
            p.CFrame = CFrame.new(x, 50, 0)
            p.Parent = workspace
            return p
        end
        local a, b = block("A", 0), block("B", 3)
        local function names(params)
            local found = workspace:GetPartBoundsInBox(CFrame.new(1.5, 50, 0), Vector3.new(10, 10, 10), params)
            local list = {}
            for _, p in found do table.insert(list, p.Name) end
            table.sort(list)
            return table.concat(list, ",")
        end
        local o = OverlapParams.new()
        print(o.ExcludeInstances, names(o))
        o.ExcludeInstances = { a }
        print(names(o))
        o.ExcludeInstances = nil
        o.IncludeInstances = { a }
        print(names(o))
        print(pcall(function() o.IncludeInstances = 5 end))
        "#,
        0,
        &[
            "nil A,B",
            "B",
            "A",
            "false Invalid value type; expected a table",
        ],
    );
}
