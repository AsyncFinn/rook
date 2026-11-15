use super::*;

#[test]
fn reads_lay_out_and_fire_changed() {
    expect(
        r#"
        local f = Instance.new("Frame")
        f.Size = UDim2.new(0, 100, 0, 50)
        local c = Instance.new("Frame")
        c.Size = UDim2.new(0.5, 0, 0.5, 10)
        c.Parent = f
        print(c.AbsoluteSize, c.AbsolutePosition)
        c:GetPropertyChangedSignal("AbsoluteSize"):Connect(function()
            print("changed", c.AbsoluteSize)
        end)
        f.Size = UDim2.new(0, 40, 0, 20)
        print(c.AbsoluteSize)
        -- A server has no top bar.
        print(game:GetService("GuiService"):GetGuiInset())
        "#,
        1,
        &["50, 35 0, 0", "changed 20, 20", "20, 20", "0, 0 0, 0"],
    );
}

/// Moving `GuiService.SelectedObject`: SelectionLost and SelectionGained,
/// then the property's change, then SelectionChanged up from the old
/// object and up from the new one, each GUI object once (Studio's log).
#[test]
fn selection_moves_fire_studio_events() {
    expect(
        r#"
        local GS = game:GetService("GuiService")
        local function new(class, name, parent)
            local o = Instance.new(class)
            o.Name = name
            o.Parent = parent
            return o
        end
        local sg = new("ScreenGui", "G", nil)
        local outer = new("Frame", "Outer", sg)
        local a = new("TextButton", "A", outer)
        local inner = new("Frame", "Inner", outer)
        local b = new("TextButton", "B", inner)
        local other = new("TextButton", "Other", sg)
        local plain = new("Frame", "Plain", sg)
        local out = {}
        local function n(x) return if x then x.Name else "nil" end
        for _, o in { sg, outer, a, inner, b, other, plain } do
            if o:IsA("GuiObject") then
                o.SelectionGained:Connect(function() table.insert(out, o.Name .. " Gained") end)
                o.SelectionLost:Connect(function() table.insert(out, o.Name .. " Lost") end)
            end
            o.SelectionChanged:Connect(function(me, old, new)
                table.insert(out, `{o.Name} Changed {me} {n(old)} {n(new)}`)
            end)
        end
        GS:GetPropertyChangedSignal("SelectedObject"):Connect(function()
            table.insert(out, "Prop " .. n(GS.SelectedObject))
        end)
        for _, step in { { "b", b }, { "b again", b }, { "other", other }, { "plain", plain },
            { "nil", nil }, { "a", a }, { "b2", b } } do
            table.insert(out, "-- " .. step[1])
            GS.SelectedObject = step[2]
            task.wait()
        end
        print(table.concat(out, "\n"))
        "#,
        20,
        &[[
            "-- b",
            "B Gained",
            "Prop B",
            "B Changed true nil B",
            "Inner Changed false nil B",
            "Outer Changed false nil B",
            "G Changed false nil B",
            "-- b again",
            "-- other",
            "B Lost",
            "Other Gained",
            "Prop Other",
            "B Changed false B Other",
            "Inner Changed false B Other",
            "Outer Changed false B Other",
            "G Changed false B Other",
            "Other Changed true B Other",
            "-- plain",
            "Other Lost",
            "Plain Gained",
            "Prop Plain",
            "Other Changed false Other Plain",
            "G Changed false Other Plain",
            "Plain Changed true Other Plain",
            "-- nil",
            "Plain Lost",
            "Prop nil",
            "Plain Changed false Plain nil",
            "G Changed false Plain nil",
            "-- a",
            "A Gained",
            "Prop A",
            "A Changed true nil A",
            "Outer Changed false nil A",
            "G Changed false nil A",
            "-- b2",
            "A Lost",
            "B Gained",
            "Prop B",
            "A Changed false A B",
            "Outer Changed false A B",
            "G Changed false A B",
            "B Changed true A B",
            "Inner Changed false A B",
        ]
        .join("\n")
        .as_str()],
    );
}

/// A zero LightDirection isn't taken: the frame keeps the one it had
/// (docs/behavior/gui/viewport-frames.md).
#[test]
fn a_viewport_frame_keeps_its_light_direction_over_zero() {
    expect(
        r#"
        local v = Instance.new("ViewportFrame")
        v.LightDirection = Vector3.new(0, 0, 1)
        v.LightDirection = Vector3.zero
        print(v.LightDirection)
        "#,
        0,
        &["0, 0, 1"],
    );
}
