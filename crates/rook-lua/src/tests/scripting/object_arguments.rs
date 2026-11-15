//! `Object` parameters given an explicit nil: Roblox casts it to the null
//! object and each method decides what that means, where only a missing
//! argument is `Argument 1 missing or nil` (measured in Studio).

use crate::tests::*;

/// A handler that hands `Debris` a nil item runs on to its next line, as
/// a server that drops an item and then tells the client does.
#[test]
fn debris_accepts_a_nil_item() {
    expect(
        r#"
        local Debris = game:GetService("Debris")
        local item
        Debris:AddItem(item, 360)
        print("after")
        print(pcall(function() Debris:AddItem(nil) end))
        print(pcall(function() Debris:AddItem() end))
        print(pcall(function() Debris:AddItem(5, 1) end))
        "#,
        2,
        &[
            "after",
            "true",
            "false Argument 1 missing or nil",
            "false Unable to cast value to Object",
        ],
    );
}

#[test]
fn null_objects_follow_each_method() {
    expect(
        r#"
        local function try(f) local ok, e = pcall(f) print(ok, e) end
        local cs = game:GetService("CollectionService")
        local ps = game:GetService("PhysicsService")
        local part = Instance.new("Part")
        local folder = Instance.new("Folder")
        print(workspace:IsDescendantOf(nil), workspace:IsAncestorOf(nil))
        try(function() workspace:IsDescendantOf() end)
        try(function() cs:AddTag(nil, "x") end)
        try(function() cs:RemoveTag(nil, "x") end)
        try(function() return cs:HasTag(nil, "x") end)
        print(#cs:GetTags(nil))
        try(function() cs:AddTag(nil) end)
        try(function() cs:HasTag() end)
        try(function() ps:SetPartCollisionGroup(nil, "Default") end)
        try(function() ps:SetPartCollisionGroup(folder, "Default") end)
        try(function() ps:CollisionGroupContainsPart("Default", nil) end)
        try(function() part:CanCollideWith(nil) end)
        try(function() workspace:GetPartsInPart(nil) end)
        local hum = Instance.new("Humanoid", Instance.new("Model"))
        try(function() hum:EquipTool(nil) end)
        try(function() return Instance.new("AudioEmitter"):GetAudibilityFor(nil) end)
        local ss = game:GetService("SoundService")
        try(function() ss:SetListener(Enum.ListenerType.ObjectPosition, nil) end)
        try(function() ss:SetListener(Enum.ListenerType.ObjectPosition) end)
        "#,
        2,
        &[
            "true false",
            "false Argument 1 missing or nil",
            "true nil",
            "true nil",
            "true false",
            "0",
            "false Argument 2 missing or nil",
            "false Argument 1 missing or nil",
            "false Parameter 1 must be BasePart in SetPartCollisionGroup.",
            "false Parameter 1 must be BasePart in SetPartCollisionGroup.",
            "false Parameter 2 must be BasePart in CollisionGroupContainsPart.",
            "false CanCollideWith expects a BasePart argument.",
            "false Part parameter must be BasePart",
            "true nil",
            "true 0",
            "false SoundService:SetListener value given is not a valid instance",
            "false SoundService:SetListener called with an incorrect argument.",
        ],
    );
}

#[test]
fn a_tween_of_nil_plays_with_nothing_to_change() {
    expect(
        r#"
        local TweenService = game:GetService("TweenService")
        local t = TweenService:Create(nil, TweenInfo.new(0.1), {})
        print(t.ClassName, t.Instance)
        t:Play()
        print(t.PlaybackState)
        t:Cancel()
        print(t.PlaybackState)
        print(pcall(function() TweenService:Create(nil, TweenInfo.new(), { Transparency = 1 }) end))
        print(pcall(function() TweenService:Create() end))
        print(pcall(function() TweenService:Create(nil, TweenInfo.new()) end))
        "#,
        2,
        &[
            "Tween nil",
            "Enum.PlaybackState.Playing",
            "Enum.PlaybackState.Cancelled",
            "false TweenService:Create failed because Instance is null",
            "false Argument 1 missing or nil",
            "false Argument 3 missing or nil",
        ],
    );
}
