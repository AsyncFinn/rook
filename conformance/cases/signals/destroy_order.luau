-- What removal handlers see and can do: Destroy and Parent = nil on a
-- part with nested children, a child moved out while its parent is being
-- destroyed, and the dying instance moving itself. Everything lives
-- outside the place.
local function build()
    local root = Instance.new("Folder"); root.Name = "Root"
    local a = Instance.new("Folder"); a.Name = "A"; a.Parent = root
    local p = Instance.new("Part"); p.Name = "P"; p.Parent = a
    local e = Instance.new("ParticleEmitter"); e.Name = "E"; e.Parent = p
    local q = Instance.new("Folder"); q.Name = "Q"; q.Parent = e
    local function state()
        return `P.Parent={p.Parent} E.Parent={e.Parent} Q.Parent={q.Parent} Pkids={#p:GetChildren()}`
    end
    root.DescendantRemoving:Connect(function(d) log("Root.DescendantRemoving", d.Name, state()) end)
    a.DescendantRemoving:Connect(function(d) log("A.DescendantRemoving", d.Name) end)
    p.DescendantRemoving:Connect(function(d) log("P.DescendantRemoving", d.Name, state()) end)
    a.ChildRemoved:Connect(function(c) log("A.ChildRemoved", c.Name, state()) end)
    p.ChildRemoved:Connect(function(c) log("P.ChildRemoved", c.Name, state()) end)
    e.ChildRemoved:Connect(function(c) log("E.ChildRemoved", c.Name) end)
    for _, inst in { p, e, q } do
        inst.Destroying:Connect(function() log(inst.Name .. ".Destroying", state()) end)
        inst.AncestryChanged:Connect(function(child, parent)
            log(inst.Name .. ".AncestryChanged", child.Name, parent, state())
        end)
        inst:GetPropertyChangedSignal("Parent"):Connect(function()
            log(inst.Name .. ".Parent changed", state())
        end)
    end
    return root, p, e, q
end

log("-- Destroy")
local root, p, e, q = build()
p:Destroy()
log("after", p.Parent, e.Parent, q.Parent)
task.wait()
root:Destroy()

log("-- Parent = nil")
root, p, e = build()
p.Parent = nil
log("after", p.Parent, e.Parent)
task.wait()
root:Destroy()

log("-- a child moved out while its parent is destroyed")
root = Instance.new("Folder"); root.Name = "Root"
local x = Instance.new("Folder"); x.Name = "X"; x.Parent = root
p = Instance.new("Part"); p.Name = "P"; p.Parent = root
e = Instance.new("ParticleEmitter"); e.Name = "E"; e.Parent = p
local f = Instance.new("Folder"); f.Name = "F"; f.Parent = p
root.DescendantRemoving:Connect(function(d)
    log("Root.DescendantRemoving", d.Name, "parent", d.Parent)
    if d == p then
        log("move E", pcall(function() e.Parent = x end))
    end
end)
p.ChildRemoved:Connect(function(c) log("P.ChildRemoved", c.Name) end)
e.Destroying:Connect(function() log("E.Destroying", e.Parent) end)
f.Destroying:Connect(function() log("F.Destroying", f.Parent) end)
p:Destroy()
log("after", e.Parent, f.Parent)
log("move E again", pcall(function() e.Parent = x end))
task.wait()

log("-- the dying instance moves itself")
local p2 = Instance.new("Part"); p2.Name = "P2"; p2.Parent = root
p2.Destroying:Connect(function()
    log("in Destroying", pcall(function() p2.Parent = x end))
end)
root.DescendantRemoving:Connect(function(d)
    if d == p2 then
        log("in DescendantRemoving", pcall(function() p2.Parent = x end))
    end
end)
p2:Destroy()
log("after", p2.Parent)
task.wait()
root:Destroy()
