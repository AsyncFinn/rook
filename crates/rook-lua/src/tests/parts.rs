use super::*;

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
