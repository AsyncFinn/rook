//! What Roblox's CoreScripts need that game scripts use too.

use super::*;

#[test]
fn style_rules_set_what_an_object_leaves_unset() {
    expect(
        r#"
        local gui = Instance.new("ScreenGui")
        local sheet = Instance.new("StyleSheet")
        sheet:SetAttribute("Gap", UDim.new(0, 7))
        sheet.Parent = gui
        local link = Instance.new("StyleLink")
        link.StyleSheet = sheet
        link.Parent = gui
        local rule = Instance.new("StyleRule")
        rule.Selector = ".wide"
        rule:SetProperties({ Size = UDim2.fromOffset(200, 10), BackgroundTransparency = 0.5 })
        rule.Parent = sheet
        local corner = Instance.new("StyleRule")
        corner.Selector = "Frame.wide::UICorner"
        corner:SetProperties({ CornerRadius = "$Gap" })
        corner.Parent = sheet
        local f = Instance.new("Frame")
        f:AddTag("wide")
        f.BackgroundTransparency = 0.25
        f.Parent = gui
        task.wait()
        print(f.Size, f.BackgroundTransparency, f.UICorner.CornerRadius)
        f:RemoveTag("wide")
        task.wait()
        print(f.Size, f:FindFirstChild("UICorner"))
        "#,
        3,
        &["{0, 200}, {0, 10} 0.25 0, 7", "{0, 0}, {0, 0} nil"],
    );
}

#[test]
fn render_step_bindings_run_by_priority() {
    expect(
        r#"
        local rs = game:GetService("RunService")
        rs:BindToRenderStep("late", 300, function() print("late") end)
        rs:BindToRenderStep("early", 100, function(dt) print("early", dt > 0) end)
        task.wait()
        rs:UnbindFromRenderStep("late")
        "#,
        2,
        &["early true", "late", "early true"],
    );
}

#[test]
fn starter_gui_core_calls() {
    expect(
        r#"
        local sg = game:GetService("StarterGui")
        print(pcall(sg.SetCore, sg, "ResetButtonCallback", false))
        sg:SetCoreGuiEnabled(Enum.CoreGuiType.Backpack, false)
        print(sg:GetCoreGuiEnabled(Enum.CoreGuiType.Backpack), sg:GetCoreGuiEnabled(Enum.CoreGuiType.All))
        sg:SetCoreGuiEnabled(Enum.CoreGuiType.All, true)
        print(sg:GetCoreGuiEnabled(Enum.CoreGuiType.Backpack))
        "#,
        1,
        &[
            "false SetCore: ResetButtonCallback has not been registered by the CoreScripts",
            "false false",
            "true",
        ],
    );
}

#[test]
fn tags_teams_and_translators() {
    expect(
        r#"
        local cs = game:GetService("CollectionService")
        local p = Instance.new("Part", workspace)
        cs:AddTag(p, "Lava")
        print(#cs:GetTagged("Lava"), cs:HasTag(p, "Lava"), cs:GetTags(p)[1])
        local teams = game:GetService("Teams")
        Instance.new("Team", teams).Name = "Red"
        print(#teams:GetTeams())
        local t = Instance.new("LocalizationTable")
        t:SetEntries({ { Key = "Hi", Source = "Hello {1}", Values = { de = "Hallo {name}" } } })
        print(t:GetTranslator("en-us"):FormatByKey("Hi", { "Ana" }))
        print(t:GetTranslator("de-de"):FormatByKey("Hi", { name = "Ben" }))
        "#,
        1,
        &["1 true Lava", "1", "Hello Ana", "Hallo Ben"],
    );
}

#[test]
fn date_time_and_settings() {
    expect(
        r#"
        local d = DateTime.fromUniversalTime(2020, 2, 30, 13, 5)
        print(d.UnixTimestamp, d:ToIsoDate(), d:FormatUniversalTime("LL", "en-us"))
        print(settings().Rendering ~= nil, UserSettings().GameSettings.ComputerMovementMode)
        -- Only Roblox's own code reads most of the player's settings.
        print(pcall(function() return UserSettings().GameSettings.ReducedMotion end))
        "#,
        1,
        &[
            "1583067900 2020-03-01T13:05:00.000Z March 1, 2020",
            "true Enum.ComputerMovementMode.Default",
            "false The current thread cannot access 'ReducedMotion' (lacking capability RobloxScript)",
        ],
    );
}
