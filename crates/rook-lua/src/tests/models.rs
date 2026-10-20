use super::*;

/// Doors that swing by `SetPrimaryPartCFrame(GetPrimaryPartCFrame() * r)`
/// every frame (a 61k-part place's) used to balloon and vanish: rounding error in
/// the rotation tripled with each move until parts were orthonormalized.
#[test]
fn repeated_model_moves_keep_parts_rigid() {
    expect(
        r#"
        local m = Instance.new("Model")
        local door = Instance.new("Part")
        door.Anchored = true
        door.CFrame = CFrame.new(-837.6, 5766.9, 1323.1) * CFrame.Angles(0.3, 1.47, -0.2)
        door.Parent = m
        local hinge = door:Clone()
        hinge.CFrame = door.CFrame * CFrame.new(2, 0, 0)
        hinge.Parent = m
        m.PrimaryPart = door
        m.Parent = workspace
        for _ = 1, 600 do
            m:SetPrimaryPartCFrame(m:GetPrimaryPartCFrame() * CFrame.Angles(0, math.rad(-6), 0))
        end
        local function scale(p) return math.round(p.CFrame.RightVector.Magnitude * 1e4) / 1e4 end
        print(scale(door), scale(hinge), math.round((door.Position - hinge.Position).Magnitude * 10) / 10)
        "#,
        0,
        &["1 1 2"],
    );
}

/// QSML3's doors turn every part about `GetModelCFrame()`, which Roblox puts
/// at the bounding box centre (turned to the primary part), not the primary
/// part itself: an invisible part beside the hinge pulls it there.
#[test]
fn model_cframe_is_the_bounding_box() {
    expect(
        r#"
        local m = Instance.new("Model")
        local door = Instance.new("Part")
        door.Size = Vector3.new(0.6, 7.7, 5)
        door.CFrame = CFrame.new(10, 5, 0) * CFrame.Angles(0, math.rad(30), 0)
        door.Parent = m
        local stabilizer = Instance.new("Part")
        stabilizer.Size = Vector3.new(0.6, 8, 5)
        stabilizer.CFrame = door.CFrame * CFrame.new(0, 0.15, 4)
        stabilizer.Parent = m
        m.PrimaryPart = door
        local box, size = m:GetBoundingBox()
        local p = door.CFrame:ToObjectSpace(m:GetModelCFrame())
        local function r(v) return math.round(v * 100) / 100 end
        print(m:GetModelCFrame() == box, r(p.X), r(p.Y), r(p.Z), r(size.X), r(size.Y), r(size.Z))
        "#,
        0,
        &["true 0 0.15 2 0.6 8 9"],
    );
}

/// Without a primary part the box turns with the saved pivot, so a light
/// spun about `GetModelCFrame()`'s X axis turns about the pivot's.
#[test]
fn model_cframe_turns_with_the_pivot() {
    expect(
        r#"
        local m = Instance.new("Model")
        local p = Instance.new("Part")
        p.Size = Vector3.new(1, 2, 3)
        p.Parent = m
        m.WorldPivot = CFrame.new(5, 0, 0) * CFrame.Angles(0, 0, math.rad(90))
        local c = m:GetModelCFrame()
        local function r(v) return math.round(v * 100) / 100 end
        print(r(c.XVector.Y), r(c.Position.X))
        "#,
        0,
        &["1 0"],
    );
}

/// An Attachment's Position, Orientation and World views read and set its
/// CFrame, under its part.
#[test]
fn attachment_views_of_cframe() {
    expect(
        "local p = Instance.new('Part') p.Position = Vector3.new(10, 0, 0) \
         local a = Instance.new('Attachment', p) a.Position = Vector3.new(0, 2, 0) \
         print(a.CFrame.Position, a.WorldPosition) \
         a.Orientation = Vector3.new(0, 90, 0) \
         print(math.round(a.Axis.Z), math.round(a.WorldAxis.Z)) \
         a.WorldPosition = Vector3.new(0, 0, 0) \
         print(a.Position)",
        0,
        &["0, 2, 0 10, 2, 0", "-1 -1", "-10, 0, 0"],
    );
}
