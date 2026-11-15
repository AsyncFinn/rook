use super::*;

/// Luau building an NPC: a Humanoid of `rig`, an anchored root turned 90
/// degrees, and a hand of the rig's name welded to it.
const NPC: &str = r#"
local function npc(rig)
    local m = Instance.new("Model")
    local h = Instance.new("Humanoid")
    h.RigType = rig
    h.Parent = m
    local root = Instance.new("Part")
    root.Name = "HumanoidRootPart"
    root.Anchored = true
    root.CFrame = CFrame.new(10, 3, 0) * CFrame.Angles(0, math.rad(90), 0)
    root.Parent = m
    local hand = Instance.new("Part")
    hand.Name = rig == Enum.HumanoidRigType.R15 and "RightHand" or "Right Arm"
    hand.Parent = m
    m.Parent = workspace
    return m, h, root, hand
end
local function tool(name, handle)
    local t = Instance.new("Tool")
    t.Name = name
    if handle then
        local hd = Instance.new("Part")
        hd.Name = "Handle"
        hd.Parent = t
    end
    t.Parent = workspace
    return t
end
local function cf(c)
    local out = {}
    for i, v in {c:GetComponents()} do
        out[i] = math.abs(v) < 1e-5 and "0" or string.format("%.3g", v)
    end
    return table.concat(out, ",")
end
"#;

#[track_caller]
fn expect_npc(source: &str, lines: &[&str]) {
    expect(&format!("{NPC}{source}"), 1, lines);
}

/// Measured in Studio 0.747: `GripForward`, `GripUp` and `GripRight` read
/// `Grip`'s columns; writing one keeps it, makes a neighbor perpendicular
/// again and crosses for the third, with a fixed (left-handed) matrix when
/// the neighbor was parallel. The classic sword's lunge writes this
/// sequence, through two such cases.
#[test]
fn grip_directions_write_as_studio() {
    expect_npc(
        r#"
        local t = Instance.new("Tool")
        t.Grip = CFrame.new(1, 2, 3) * CFrame.Angles(0.3, 0.5, 0.7)
        local function v(x) return string.format("%.3f %.3f %.3f", x.X, x.Y, x.Z) end
        print(t.GripPos, v(t.GripForward), v(t.GripUp), v(t.GripRight))
        t.Grip = CFrame.Angles(0.3, 0.5, 0.7)
        t.GripRight = Vector3.new(0, 0, 1)
        print(cf(t.Grip))
        t.Grip = CFrame.new(0, 0, -1.5)
        for _, s in {
            {"GripForward", Vector3.new(0, 0, 1)}, {"GripRight", Vector3.new(0, 1, 0)},
            {"GripUp", Vector3.new(1, 0, 0)}, {"GripForward", Vector3.new(-1, 0, 0)},
            {"GripRight", Vector3.new(0, 1, 0)}, {"GripUp", Vector3.new(0, 0, 1)},
        } do
            t[s[1]] = s[2]
            print(cf(t.Grip))
        end
        t.GripPos = Vector3.new(4, 5, 6)
        print(t.Grip.Position)
        "#,
        &[
            "1, 2, 3 -0.479 0.259 -0.838 -0.565 0.639 0.521 0.671 0.724 -0.160",
            "0,0,0,0,-0.662,-0.749,0,0.749,-0.662,1,0,0",
            "0,0,-1.5,-1,0,0,0,1,0,0,0,-1",
            "0,0,-1.5,0,1,0,-1,0,0,0,0,-1",
            "0,0,-1.5,0,1,0,-1,0,0,0,0,1",
            "0,0,-1.5,0,1,0,-1,0,0,0,0,-1",
            "0,0,-1.5,0,1,0,1,0,0,0,0,-1",
            "0,0,-1.5,0,0,1,1,0,0,0,1,0",
            "4, 5, 6",
        ],
    );
}

/// Measured on R6 and R15 NPCs: the grip is a Weld named `RightGrip` in
/// the hand, from the hand to the Handle, `C1` the Tool's `Grip` (and
/// following it); R6's `C0` is fixed, the Handle stops colliding after
/// `Equipped`, and `UnequipTools` without a player leaves the Tool in nil,
/// collidable again, 4 up and 8 ahead of the root part.
#[test]
fn npc_grip_and_letting_go() {
    expect_npc(
        r#"
        local m, h, root, hand = npc(Enum.HumanoidRigType.R6)
        local t = tool("Sword", true)
        local hd = t.Handle
        hd.CanCollide = false
        t.Grip = CFrame.new(0, 0, 1)
        t.AncestryChanged:Connect(function() print("ancestry") end)
        t.Equipped:Connect(function(mouse)
            print("equipped", hd.CanCollide, hand:FindFirstChild("RightGrip"), mouse.ClassName, mouse.Name)
            print(pcall(function() return mouse.Hit end))
        end)
        t.Unequipped:Connect(function() print("unequipped", hd.CanCollide, hand:FindFirstChild("RightGrip")) end)
        h:EquipTool(t)
        local g = hand.RightGrip
        print(g.ClassName, g.Part0 == hand, g.Part1 == hd, cf(g.C0), cf(g.C1), hd.CanCollide)
        t.GripPos = Vector3.new(0, 0, 2)
        print(cf(g.C1))
        h:UnequipTools()
        local at = root.CFrame:PointToObjectSpace(hd.Position)
        print(t.Parent, g.Parent, hd.CanCollide, string.format("%.2f %.2f %.2f", at.X, at.Y, at.Z))
        "#,
        &[
            "ancestry",
            "equipped false nil Mouse Instance",
            "false This Mouse is no longer active",
            "Weld true true 0,-1,0,1,0,0,0,0,1,0,-1,0 0,0,1,1,0,0,0,1,0,0,0,1 false",
            "0,0,2,1,0,0,0,1,0,0,0,1",
            "unequipped true nil",
            "ancestry",
            "nil nil true 0.00 4.00 -8.00",
        ],
    );
}

/// Measured: R15 takes `C0` from the hand's `RightGripAttachment`, else
/// 0.2 below the hand; a renamed hand gets no grip.
#[test]
fn r15_grip_comes_from_the_attachment() {
    expect_npc(
        r#"
        local m, h, root, hand = npc(Enum.HumanoidRigType.R15)
        local a = Instance.new("Attachment")
        a.Name = "RightGripAttachment"
        a.CFrame = CFrame.new(0, -0.15, 0.1) * CFrame.Angles(math.rad(-90), 0, 0)
        a.Parent = hand
        local t = tool("A", true)
        h:EquipTool(t)
        print(cf(hand.RightGrip.C0))
        h:UnequipTools()
        a:Destroy()
        t.Parent = workspace
        h:EquipTool(t)
        print(cf(hand.RightGrip.C0))
        h:UnequipTools()
        hand.Name = "Renamed"
        t.Parent = workspace
        h:EquipTool(t)
        print(t.Parent == m, hand:FindFirstChild("RightGrip"))
        "#,
        &[
            "0,-0.15,0.1,1,0,0,0,0,1,0,-1,0",
            "0,-0.2,0,1,0,0,0,0,1,0,-1,0",
            "true nil",
        ],
    );
}

/// Measured: a Tool without a Handle whose `RequiresHandle` is on moves
/// into the character but isn't held: no `Equipped` or `Unequipped`, and
/// `Activate()` warns. Without the requirement it equips and activates.
#[test]
fn requires_handle_decides_holding() {
    expect_npc(
        r#"
        local m, h = npc(Enum.HumanoidRigType.R6)
        for _, needs in {true, false} do
            local t = tool(tostring(needs), false)
            t.RequiresHandle = needs
            t.Equipped:Connect(function() print("equipped", t.Name) end)
            t.Unequipped:Connect(function() print("unequipped", t.Name) end)
            t.Activated:Connect(function() print("activated", t.Name) end)
            t.Parent = m
            t:Activate()
            t.Parent = workspace
        end
        "#,
        &[
            "WARN Tool:Activate() called from script when tool is not equipped. Tool will not be activated.",
            "equipped false",
            "activated false",
            "unequipped false",
        ],
    );
}

/// Measured: `Activate()` and `Deactivate()` fire on a held Tool, nothing
/// on a disabled one (no warning), and `Activate()` on a Tool no one holds
/// warns.
#[test]
fn activate_needs_a_held_enabled_tool() {
    expect_npc(
        r#"
        local m, h = npc(Enum.HumanoidRigType.R6)
        local t = tool("T", true)
        t.Activated:Connect(function() print("activated") end)
        t.Deactivated:Connect(function() print("deactivated") end)
        t:Activate()
        h:EquipTool(t)
        t:Activate()
        t:Deactivate()
        t.Enabled = false
        t:Activate()
        print("done")
        "#,
        &[
            "WARN Tool:Activate() called from script when tool is not equipped. Tool will not be activated.",
            "activated",
            "deactivated",
            "done",
        ],
    );
}

/// The backpack CoreScript's `HopperBin:ToggleSelect()` only flips
/// `Active` (measured: modern Roblox fires no `Selected`); game scripts
/// can't call it.
#[test]
fn hopper_bins_only_toggle_active() {
    let rt = Runtime::new(Dom::new(), Clock::Fixed { step: 1.0 / 60.0 }).unwrap();
    let source = r#"
        local bin = Instance.new("HopperBin")
        bin.Selected:Connect(function() print("selected") end)
        bin:ToggleSelect()
        print(bin.Active)
        bin:ToggleSelect()
        print(bin.Active)
    "#;
    let script = {
        let mut w = rt.world_mut();
        let ws = w.dom.get_service("Workspace").unwrap();
        let script = w.dom.create_by_name("Script").unwrap();
        w.dom.set_parent(script, Some(ws)).unwrap();
        let code = crate::bytecode::compiler().compile(source).unwrap();
        w.bytecode.insert(
            script,
            crate::world::Compiled {
                code: code.into(),
                core: true,
            },
        );
        script
    };
    assert!(rt.run_script(script));
    let log: Vec<String> = rt.take_log().into_iter().map(|e| e.message).collect();
    assert_eq!(log, ["true", "false"]);
    expect(
        "print(pcall(function() Instance.new('HopperBin'):ToggleSelect() end))",
        0,
        &["false The current thread cannot call 'ToggleSelect' (lacking capability RobloxScript)"],
    );
}
