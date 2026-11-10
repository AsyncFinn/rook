-- The Mouse's Icon and UserInputService.MouseIcon: a Mouse icon that
-- changes is copied to MouseIcon (whose Changed fires first), the same
-- icon again isn't, and MouseIcon never sets the Mouse's. Both read ""
-- before anything sets them (the shift lock cursor is set this way).
--! client
local uis = game:GetService("UserInputService")
local mouse = game:GetService("Players").LocalPlayer:GetMouse()

local function show(tag)
    log(tag, ("%q"):format(mouse.Icon), ("%q"):format(uis.MouseIcon))
end

local events = {}
local a = uis:GetPropertyChangedSignal("MouseIcon"):Connect(function()
    table.insert(events, "uis")
end)
local b = mouse:GetPropertyChangedSignal("Icon"):Connect(function()
    table.insert(events, "mouse")
end)
local function step(tag, f)
    events = {}
    f()
    task.wait()
    show(tag)
    log("  changed", table.concat(events, ","))
end

show("start")
step("mouse a", function() mouse.Icon = "rbxasset://a.png" end)
step("uis b", function() uis.MouseIcon = "rbxasset://b.png" end)
step("mouse a again", function() mouse.Icon = "rbxasset://a.png" end)
step("mouse c", function() mouse.Icon = "rbxasset://c.png" end)
step("uis empty", function() uis.MouseIcon = "" end)
step("mouse empty", function() mouse.Icon = "" end)
a:Disconnect()
b:Disconnect()
