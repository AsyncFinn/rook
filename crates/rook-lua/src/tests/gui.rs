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
