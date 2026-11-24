use crate::tests::*;

#[test]
fn legacy_lowercase_members() {
    expect(
        r#"
        local m = Instance.new("Model", workspace)
        local p = Instance.new("Part")
        p.Parent = m
        print(m:findFirstChild("Part") == p, #m:children(), #m:getChildren())
        local c = p:clone()
        print(c.Parent, c.Name)
        p:remove()
        print(p.Parent, #m:GetChildren())
        print(game:service("Workspace") == workspace)
        print(game.workspace == workspace, game.Workspace == workspace)
        print(m.className, p.className)
        local lighting = game:GetService("Lighting")
        print(game.lighting == lighting)
        local s = Instance.new("Sound")
        s.Playing = true
        local h = Instance.new("Humanoid")
        h.maxHealth = 50
        print(s.isPlaying, h.MaxHealth, h.maxHealth, p.brickColor == p.BrickColor)
        "#,
        0,
        &[
            "true 1 1",
            "nil Part",
            "nil 0",
            "true",
            "true true",
            "Model Part",
            "true",
            "true 50 50 true",
        ],
    );
}

#[test]
fn unimplemented_members_are_counted() {
    expect(
        r#"
        local ok, e = pcall(function() return workspace:ArePartsTouchingOthers({}) end)
        print(e)
        "#,
        0,
        &["Workspace:ArePartsTouchingOthers is not implemented in Rook yet"],
    );
}

#[test]
fn services_datastore_http_bindables() {
    expect(
        r#"
        local ds = game:GetService("DataStoreService"):GetDataStore("Stats")
        print(ds == game:GetService("DataStoreService"):GetDataStore("Stats"), ds.ClassName)
        ds:SetAsync("p1", { cash = 5, items = { "sword" } })
        local v = ds:GetAsync("p1")
        v.cash = 99
        print(ds:GetAsync("p1").cash, ds:GetAsync("p1").items[1])
        print(ds:IncrementAsync("visits"), ds:IncrementAsync("visits", 4))
        print(ds:UpdateAsync("p1", function(old) old.cash += 1 return old end).cash)
        print(pcall(function() ds:SetAsync("bad", workspace) end))
        local od = game:GetService("DataStoreService"):GetOrderedDataStore("Top")
        for i, n in { 5, 1, 9 } do od:SetAsync("k" .. i, n) end
        local page = od:GetSortedAsync(false, 2):GetCurrentPage()
        print(page[1].key, page[1].value, page[2].value, #page)
        local http = game:GetService("HttpService")
        print(http:JSONEncode({ 1, 2, 3 }), http:JSONDecode('{"a":[true,null,2]}').a[3])
        print(#http:GenerateGUID(), #http:GenerateGUID(false), http:UrlEncode("a b&c"))
        local bf = Instance.new("BindableFunction")
        task.delay(0.1, function() bf.OnInvoke = function(x) return x * 2 end end)
        print("invoke", bf:Invoke(21))
        print(pcall(function() return bf.OnInvoke end))
        "#,
        20,
        &[
            "true DataStore",
            "5 sword",
            "1 5",
            "6",
            "false Cannot store Instance in data store. Data stores can only accept valid UTF-8 characters.",
            "k3 9 5 2",
            "[1,2,3] 2",
            "38 36 a%20b%26c",
            "invoke 42",
            "false OnInvoke is a callback member of BindableFunction; you can only set the callback value, get is not available",
        ],
    );
}

/// Every no-op member must be documented with what will take it over.
#[test]
fn inert_members_are_documented() {
    let doc = include_str!("../../../../../docs/guide/stubs.md");
    for (owner, name) in crate::api::methods::inert_members() {
        let row = format!("`{owner}:{name}`");
        let grouped = format!("`{owner}:");
        assert!(
            doc.contains(&row) || (doc.contains(&grouped) && doc.contains(&format!("`{name}`"))),
            "{owner}:{name} is inert but not listed in docs/guide/stubs.md"
        );
    }
}

#[test]
fn legacy_enum_names_resolve() {
    expect(
        r#"
        print(Enum.KeyCode.Unknown == Enum.KeyCode.None, Enum.KeyCode.Unknown.Name)
        print(Enum.SurfaceType.Bumps)
        local p = Instance.new("Part")
        p.TopSurface = "Spawn"
        print(p.TopSurface)
        "#,
        0,
        &[
            "true None",
            "Enum.SurfaceType.Glue",
            "Enum.SurfaceType.Smooth",
        ],
    );
}

/// `AudioPlayer.AssetId` is `Asset` by its older name: each reads and
/// writes the other (measured in Studio 0.748, a number included).
#[test]
fn audio_player_asset_id_is_asset() {
    expect(
        r#"
        local a = Instance.new("AudioPlayer")
        a.Asset = "rbxassetid://123"
        print(a.AssetId)
        a.AssetId = "rbxassetid://456"
        print(a.Asset)
        a.AssetId = 5
        print(a.Asset, typeof(a.AssetId))
        a:GetPropertyChangedSignal("AssetId"):Connect(function() print("AssetId changed") end)
        a.Asset = "rbxassetid://789"
        "#,
        0,
        &[
            "rbxassetid://123",
            "rbxassetid://456",
            "5 string",
            "AssetId changed",
        ],
    );
}

/// Measured in Studio 0.748: an instance of another class as `self` is no
/// `self` at all, so `player.Kick(workspace)` can't kick anything.
#[test]
fn a_member_function_needs_self_of_its_own_class() {
    expect(
        r#"
        local p = Instance.new("Part")
        local sky = Instance.new("Sky")
        local function try(f, ...)
            print((select(2, pcall(f, ...))))
        end
        try(p.ClearAllChildren, Instance.new("Folder"))
        try(p.BreakJoints, workspace)
        try(p.BreakJoints, nil)
        try(workspace.GetServerTimeNow, sky)
        print(pcall(workspace.FindFirstChild, p, "x"))
        "#,
        0,
        &[
            "nil",
            "Expected ':' not '.' calling member function BreakJoints",
            "Expected ':' not '.' calling member function BreakJoints",
            "Expected ':' not '.' calling member function GetServerTimeNow",
            "true nil",
        ],
    );
}
