-- Which events fire when an instance moves between two parents that share
-- an ancestor, and in what order. Everything lives outside the place.
local root = Instance.new("Folder"); root.Name = "Root"
local a = Instance.new("Folder"); a.Name = "A"; a.Parent = root
local b = Instance.new("Folder"); b.Name = "B"; b.Parent = root
local m = Instance.new("Model"); m.Name = "M"; m.Parent = a
local p = Instance.new("Part"); p.Name = "P"; p.Parent = m
local q = Instance.new("Folder"); q.Name = "Q"; q.Parent = p
local r = Instance.new("Folder"); r.Name = "R"; r.Parent = m

root.DescendantRemoving:Connect(function(d) log("root.DescendantRemoving", d.Name) end)
root.DescendantAdded:Connect(function(d) log("root.DescendantAdded", d.Name) end)
a.ChildRemoved:Connect(function(c) log("A.ChildRemoved", c.Name) end)
b.ChildAdded:Connect(function(c) log("B.ChildAdded", c.Name) end)
for _, inst in { m, p, q, r } do
    inst.AncestryChanged:Connect(function(child, parent)
        log(inst.Name .. ".AncestryChanged", child.Name, parent)
    end)
end
m.Changed:Connect(function(prop) log("M.Changed", prop) end)
m:GetPropertyChangedSignal("Parent"):Connect(function() log("M.PropertyChangedSignal(Parent)") end)

log("set parent")
m.Parent = b
log("after set parent")
task.wait()

log("-- leave the shared ancestor")
m.Parent = nil
task.wait()
root:Destroy()
