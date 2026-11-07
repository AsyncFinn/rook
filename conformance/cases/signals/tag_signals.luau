-- CollectionService's added and removed signals: when an instance counts as
-- tagged, and what each signal is handed.
local CS = game:GetService("CollectionService")
local tag = "RookTagSignalCase"
local name = function(i)
    return i.Name
end
local added = CS:GetInstanceAddedSignal(tag):Connect(function(i)
    log("added", name(i), "in game", i:IsDescendantOf(game), "parent", i.Parent)
end)
local removed = CS:GetInstanceRemovedSignal(tag):Connect(function(i)
    log("removed", name(i), "in game", i:IsDescendantOf(game), "parent", i.Parent, "tagged", i:HasTag(tag))
end)
log("signal", typeof(CS:GetInstanceAddedSignal(tag)), tostring(CS:GetInstanceAddedSignal(tag)))
log("same object", CS:GetInstanceAddedSignal(tag) == CS:GetInstanceAddedSignal(tag))
for _, bad in { "nil", "number" } do
    local arg = if bad == "nil" then nil else 5
    log(bad, pcall(function()
        return tostring(CS:GetInstanceAddedSignal(arg))
    end))
end
log("empty", pcall(function()
    return tostring(CS:GetInstanceAddedSignal(""))
end))

local function step(what)
    task.wait()
    log("--", what)
end

local a = Instance.new("Folder")
a.Name = "a"
a:AddTag(tag)
step("tagged outside the game")
log("GetTagged", #CS:GetTagged(tag))
a.Parent = workspace
step("parented in")
log("GetTagged", #CS:GetTagged(tag))
a:AddTag(tag)
step("tagged twice")
a.Parent = game:GetService("ReplicatedStorage")
step("moved inside the game")
a.Parent = nil
step("taken out")
log("GetTagged", #CS:GetTagged(tag))
a.Parent = workspace
step("put back")
a:RemoveTag(tag)
step("tag removed in the game")
a:RemoveTag(tag)
step("removed twice")
a:AddTag(tag)
step("tagged in the game")

local holder = Instance.new("Folder")
holder.Name = "holder"
local inner = Instance.new("Folder")
inner.Name = "inner"
inner:AddTag(tag)
inner.Parent = holder
local second = inner:Clone()
second.Name = "second"
second.Parent = holder
log("clone keeps tag", second:HasTag(tag))
holder.Parent = workspace
step("subtree parented in")
holder.Parent = nil
step("subtree taken out")
holder.Parent = workspace
step("subtree back")
holder:Destroy()
step("subtree destroyed")
a:Destroy()
step("destroyed")

local twice = Instance.new("Folder")
twice.Name = "twice"
twice.Parent = workspace
twice:AddTag(tag)
twice:RemoveTag(tag)
twice:AddTag(tag)
step("added, removed, added in one step")
twice:Destroy()
step("end")
added:Disconnect()
removed:Disconnect()
