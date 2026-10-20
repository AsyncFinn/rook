use super::*;

/// The blast as measured in Studio 0.742 (docs/behavior/effects/explosions.md):
/// parts whose surface it reaches are hit with their centre's distance;
/// unanchored ones fly off at BlastPressure × |Size|² × 5.48e-5 / mass
/// whatever their distance, spinning about that line; Welds within the
/// joint radius break and WeldConstraints don't; Terrain is carved; the
/// Explosion is gone 2.75 s later.
#[test]
fn explosions_hit_break_push_and_go() {
    let mut s = Server::new(&Options::default()).unwrap();
    add_script(
        &s,
        "ServerScriptService",
        "Blast",
        r#"
        workspace.Gravity = 0
        local O = Vector3.new(0, 200, 0)
        local function part(name, offset, size)
            local p = Instance.new("Part")
            p.Name = name
            p.Size = size or Vector3.one
            p.CanCollide = false
            p.Position = O + offset
            p.Parent = workspace
            return p
        end
        part("near", Vector3.new(2, 0, 0))
        part("edge", Vector3.new(10.4, 0, 0))
        part("out", Vector3.new(10.6, 0, 0))
        part("long", Vector3.new(0, 0, -6), Vector3.new(1, 1, 10))
        local a, b = part("wa", Vector3.new(-3, 0, 0)), part("wb", Vector3.new(-3, 1, 0))
        local weld = Instance.new("Weld")
        weld.Part0, weld.Part1, weld.C1 = a, b, CFrame.new(0, -1, 0)
        weld.Parent = a
        local c, d = part("ca", Vector3.new(0, -3, 0)), part("cb", Vector3.new(0, -4, 0))
        local wc = Instance.new("WeldConstraint")
        wc.Part0, wc.Part1 = c, d
        wc.Parent = c
        workspace.Terrain:FillBlock(CFrame.new(0, -20, 0), Vector3.new(16, 16, 16), Enum.Material.Rock)
        task.wait(0.1)
        local x = Instance.new("Explosion")
        x.Position = O
        x.BlastRadius = 10
        x.BlastPressure = 5000
        x.DestroyJointRadiusPercent = 0.5
        local hits = {}
        x.Hit:Connect(function(p, distance)
            table.insert(hits, string.format("%s %.1f", p.Name, distance))
        end)
        x.Parent = workspace
        local x2 = Instance.new("Explosion")
        x2.Position = Vector3.new(0, -12, 0)
        x2.BlastRadius = 4
        x2.Parent = workspace
        task.wait(0.1)
        table.sort(hits)
        print(table.concat(hits, ", "))
        local v = workspace.near.AssemblyLinearVelocity
        print(string.format("near %.3f %.3f %.2f", v.X, v.Magnitude, workspace.near.AssemblyAngularVelocity.X))
        print(string.format("long %.3f", workspace.long.AssemblyLinearVelocity.Magnitude))
        print(weld.Parent, wc.Parent, x.Parent)
        local _, occupancy = workspace.Terrain:ReadVoxels(Region3.new(Vector3.new(0, -16, 0), Vector3.new(4, -12, 4)), 4)
        print(string.format("%.2f", occupancy[1][1][1]))
        task.wait(2.7)
        print(x.Parent)
        "#,
    );
    s.start();
    s.run_for(3.0);
    assert_eq!(
        lines(&s),
        [
            "ca 3.0, cb 4.0, edge 10.4, long 6.0, near 2.0, wa 3.0, wb 3.2",
            "near 1.175 1.175 3.05",
            "long 3.994",
            "nil ca Workspace",
            "0.37",
            "nil",
        ]
    );
}
