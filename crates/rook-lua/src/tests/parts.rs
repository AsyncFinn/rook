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
