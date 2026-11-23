use crate::tests::*;

/// Measured in Studio 0.747: datatype constructors read a number argument
/// that isn't a number, or a string that reads as one, as 0; methods
/// still error.
#[test]
fn constructors_read_non_numbers_as_zero() {
    expect(
        r#"
        print(Color3.new('New Yeller'), Color3.new('0.5', '0.25', ' 1 '))
        print(Color3.new('0x10', '1e-1', 'inf'), Color3.new(true, 1, 1), Color3.new(1, nil, 1))
        print(Color3.fromRGB('abc', 2, 3), Color3.fromHSV(nil, 1, 1))
        print(Vector3.new('abc', 2, 3), Vector3.new(Vector3.one, 2, 3), Vector2.new({}, 2))
        print(Vector3int16.new('a', 2, 3), Vector2int16.new('a', 2))
        print(UDim.new('abc', 2), UDim2.new('abc', 2, 3, 4))
        print(UDim2.fromScale('a', 2), UDim2.fromOffset('a', 2))
        print(CFrame.new('abc', 2, 3), CFrame.new(1, 2, 3, 'a', 0, 0, 1))
        print(pcall(CFrame.Angles, 'a', 0, 0))
        "#,
        0,
        &[
            "0, 0, 0 0.5, 0.25, 1",
            "16, 0.100000001, inf 0, 1, 1 1, 0, 1",
            "0, 0.00784313772, 0.0117647061 1, 0, 0",
            "0, 2, 3 0, 2, 3 0, 2",
            "0, 2, 3 0, 2",
            "0, 2 {0, 2}, {3, 4}",
            "{0, 0}, {2, 0} {0, 0}, {0, 2}",
            "0, 2, 3, 1, 0, 0, 0, 1, 0, 0, 0, 1 1, 2, 3, 1, 0, 0, 0, 1, 0, 0, 0, 1",
            "false Unable to cast string to float",
        ],
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

/// Measured: offsets are cut toward zero, and NaN, infinities and
/// anything past an `i32` read -2147483648 whichever side they're on.
#[test]
fn udim_offsets() {
    expect(
        r#"
        local t = {}
        for _, v in {3.7, -3.7, 2147483647.9, 2147483648, -2147483649, 1e20, -math.huge, 0/0} do
            table.insert(t, UDim.new(0, v).Offset)
        end
        print(table.concat(t, " "), UDim2.new(0, 0, 0, 0/0).Y.Offset, UDim2.fromOffset(0/0, 0).X.Offset)
        "#,
        0,
        &[
            "3 -3 2147483647 -2147483648 -2147483648 -2147483648 -2147483648 -2147483648 -2147483648 -2147483648",
        ],
    );
}

/// Luau's own library lacks these; Roblox adds them (creator-docs utf8).
#[test]
fn utf8_normalizes() {
    assert_eq!(
        run(
            r#"
            print(#utf8.nfcnormalize("e\u{301}"), #utf8.nfdnormalize("\u{e9}"))
            print(utf8.nfcnormalize("e\u{301}") == "\u{e9}")
            "#,
            1
        ),
        ["2 3", "true"]
    );
}

/// Reading a member a datatype doesn't have raises Studio's error, worded
/// its way for the type; real members still read (Studio 0.747,
/// 2026-11-11, every datatype scripts can make).
#[test]
fn datatypes_refuse_members_they_dont_have() {
    expect(
        r#"
        local cf = CFrame.new(1, 2, 3)
        print(cf.X, cf.p.Y, cf.Position.Z, typeof(cf.Inverse))
        print(select(2, pcall(function() return cf.ClassName end)))
        print(select(2, pcall(function() return cf[1] end)))
        print(select(2, pcall(function() return Color3.new().ClassName end)))
        print(select(2, pcall(function() return UDim.new().ClassName end)))
        print(select(2, pcall(function() return UDim.new()[1] end)))
        print(select(2, pcall(function() return Enum.Material.Plastic.ClassName end)))
        print(select(2, pcall(function() return Enum.Material.ClassName end)))
        print(select(2, pcall(function() return Faces.new().ClassName end)))
        print(UDim.new(0.5).Scale, Enum.Material.Plastic.Name)
        "#,
        0,
        &[
            "1 2 3 function",
            "ClassName is not a valid member of CFrame",
            "invalid argument #2 (string expected, got number)",
            "ClassName is not a valid member of Color3",
            "'ClassName' is not a valid member of UDim",
            "'1' is not a valid member of UDim",
            "ClassName is not a valid member of \"Enum.Material.Plastic\"",
            "ClassName is not a valid member of \"Enum.Material\"",
            "'ClassName' is not a valid member of Faces",
            "0.5 Plastic",
        ],
    );
}

/// Faces, Axes and PhysicalProperties name themselves to `typeof`, print
/// their members in Studio's order and compare as in Studio 0.747
/// (2026-11-11), and a part's physical properties read back as the same
/// kind.
#[test]
fn faces_axes_and_physical_properties_are_their_own_types() {
    expect(
        r#"
        local part = Instance.new("Part")
        part.CustomPhysicalProperties = PhysicalProperties.new(1, 0.5, 0.25)
        for _, v in {Faces.new(Enum.NormalId.Top), Axes.new(Enum.Axis.X), part.CustomPhysicalProperties} do
            print(typeof(v), tostring(v))
        end
        print(Faces.new(Enum.NormalId.Top) == Faces.new(Enum.NormalId.Top))
        print(tostring(Faces.new(Enum.NormalId.Front, Enum.NormalId.Right, Enum.NormalId.Top)))
        print(tostring(Axes.new(Enum.Axis.Z, Enum.NormalId.Left)), "[" .. tostring(Faces.new()) .. "]")
        "#,
        0,
        &[
            "Faces Top",
            "Axes X",
            "PhysicalProperties 1, 0.5, 0.25, 1, 1, 1",
            "true",
            "Right, Top, Front",
            "X, Z []",
        ],
    );
}

/// Unset custom physical properties read nil, and setting nil unsets them
/// (measured in Studio 0.748).
#[test]
fn custom_physical_properties_unset_is_nil() {
    expect(
        r#"
        local p = Instance.new("Part")
        print(p.CustomPhysicalProperties)
        p.CustomPhysicalProperties = PhysicalProperties.new(1, 0.3, 0.5)
        print(p.CustomPhysicalProperties.Density)
        p.CustomPhysicalProperties = nil
        print(p.CustomPhysicalProperties)
        local q = Instance.new("Part")
        q.CustomPhysicalProperties = p.CustomPhysicalProperties
        print(q.CustomPhysicalProperties)
        "#,
        0,
        &["nil", "1", "nil", "nil"],
    );
}
