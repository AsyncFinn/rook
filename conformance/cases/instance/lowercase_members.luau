-- A key with a lowercase first letter that names no member: reads fall
-- back to the capitalized member after the children, writes don't.
local function read(i, k)
    local ok, v = pcall(function()
        return i[k]
    end)
    return ok, if ok then typeof(v) .. " " .. tostring(v) else v
end
local p = Instance.new("Part")
p.Name = "Holder"
for _, k in { "anchored", "transparency", "size", "name", "cFrame", "aNCHORED", "foo" } do
    log("read", k, read(p, k))
end
for _, k in { "getChildren", "isA", "touched", "changed", "ancestryChanged" } do
    local ok, v = pcall(function()
        return p[k]
    end)
    log("read", k, ok, if ok then typeof(v) else v)
end
log("call isA", pcall(function()
    return p:isA("BasePart")
end))
log("call getFullName", pcall(function()
    return p:getFullName()
end))
log("write anchored", pcall(function()
    p.anchored = true
end), p.Anchored)
log("write name", pcall(function()
    p.name = "Other"
end), p.Name)
-- `brickColor` is a member of its own (the API lists it).
log("write brickColor", pcall(function()
    p.brickColor = BrickColor.new("Bright red")
end), p.BrickColor)
local foo = Instance.new("Folder")
foo.Name = "Foo"
foo.Parent = p
log("foo with child Foo", read(p, "foo"))
for _, k in { "anchored", "name" } do
    local c = Instance.new("Folder")
    c.Name = k
    c.Parent = p
    log("child", k, read(p, k))
end
local a = Instance.new("Folder")
a.Name = "Anchored"
a.Parent = p
log("child Anchored", read(p, "Anchored"))
local run = game:GetService("RunService")
log("read renderStepped", pcall(function()
    return typeof(run.renderStepped)
end))
log("write renderStepped", pcall(function()
    run.renderStepped = 1
end))
local t = Instance.new("TextLabel")
log("read textSize", read(t, "textSize"))
log("write textSize", pcall(function()
    t.textSize = 20
end))
t:Destroy()
p:Destroy()
