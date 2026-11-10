//! HumanoidDescription members, against values read in Studio
//! (docs/behavior/characters/rigs.md#emotes).

use super::*;

#[test]
fn emotes() {
    expect(
        r#"
        local function owned(t)
            local names = {}
            for name in t do table.insert(names, name) end
            table.sort(names)
            for i, name in names do names[i] = name .. "=" .. table.concat(t[name], ",") end
            return "{" .. table.concat(names, " ") .. "}"
        end
        local function equipped(t)
            local out = {}
            for _, e in t do table.insert(out, e.Slot .. ":" .. e.Name) end
            return "[" .. table.concat(out, " ") .. "]"
        end
        local d = Instance.new("HumanoidDescription")
        print(owned(d:GetEmotes()), equipped(d:GetEquippedEmotes()))
        d.EmotesChanged:Connect(function(e) print("emotes", owned(e)) end)
        d.EquippedEmotesChanged:Connect(function(e) print("equipped", equipped(e)) end)
        d:SetEmotes({["Odd^Name"] = {10, 20}, Wave = {1, 1.5, "7"}, Gone = {}, [5] = {3}})
        d:AddEmote("Wave", 2)
        d:AddEmote("Wave", 2)
        d:RemoveEmote("Nope")
        d:RemoveEmote("5")
        d:SetEquippedEmotes({{Slot = 5, Name = "A"}, "B", {Name = "C"}, {Slot = 2.5, Name = "D"}, "E", {Slot = 2, Name = "F"}})
        print(pcall(function() d:SetEquippedEmotes({x = "a"}) end))
        print(owned(d:Clone():GetEmotes()), equipped(d:Clone():GetEquippedEmotes()))
        "#,
        0,
        &[
            "{} []",
            "emotes {5=3 Odd^Name=10,20 Wave=1}",
            "emotes {5=3 Odd^Name=10,20 Wave=1,2}",
            "emotes {5=3 Odd^Name=10,20 Wave=1,2}",
            "emotes {Odd^Name=10,20 Wave=1,2}",
            "equipped [1:B 2:F 3:D 5:A]",
            "false Unable to cast Dictionary to Array",
            "{Odd^Name=10,20 Wave=1,2} [1:B 2:F 3:D 5:A]",
        ],
    );
}
