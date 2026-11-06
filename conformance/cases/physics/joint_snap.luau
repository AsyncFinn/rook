-- When joints move their parts: nothing moves until a script reads where the
-- part that moves is, or queries the world, so what's changed before that
-- still counts. Parts go far above the place and are destroyed at the end.
local made = {}
local function part(name, x, anchored, size)
    local p = Instance.new("Part")
    p.Name = name
    p.Anchored = anchored
    p.Size = size or Vector3.new(2, 2, 2)
    p.CFrame = CFrame.new(x, 1000, 0)
    p.Parent = workspace
    table.insert(made, p)
    return p
end
local function pos(p)
    local v = p.Position - Vector3.new(0, 1000, 0)
    return string.format("%.2f,%.2f,%.2f", v.X, v.Y, v.Z)
end
local function joint(class, a, b, c0)
    local j = Instance.new(class)
    j.Part0 = a
    j.Part1 = b
    if class ~= "WeldConstraint" then
        j.C0 = c0 or CFrame.new(0, 5, 0)
    end
    return j
end
local function case(name, f)
    log(name, f())
    for _, p in ipairs(made) do
        p:Destroy()
    end
    table.clear(made)
end

for _, class in ipairs({
    "Weld",
    "Snap",
    "Glue",
    "ManualWeld",
    "Motor",
    "Motor6D",
    "Rotate",
    "RotateV",
    "RotateP",
    "WeldConstraint",
}) do
    case(class .. " anchored to free", function()
        local a, b = part("a", 0, true), part("b", 20, false)
        joint(class, a, b).Parent = a
        return pos(a), pos(b)
    end)
end

case("the root stays: the larger part", function()
    local a, b = part("a", 0, false, Vector3.new(4, 4, 4)), part("b", 20, false)
    joint("Weld", a, b).Parent = a
    return pos(a), pos(b)
end)
case("the root stays: the larger part as Part1", function()
    local a, b = part("a", 0, false), part("b", 20, false, Vector3.new(4, 4, 4))
    joint("Weld", a, b).Parent = a
    return pos(a), pos(b)
end)
case("the root stays: equal parts", function()
    local a, b = part("a", 0, false), part("b", 20, false)
    joint("Weld", a, b).Parent = a
    return pos(a), pos(b)
end)
case("both anchored", function()
    local a, b = part("a", 0, true), part("b", 20, true)
    joint("Weld", a, b).Parent = a
    return pos(a), pos(b)
end)
case("both anchored, then one let go", function()
    local a, b = part("a", 0, true), part("b", 20, true)
    joint("Weld", a, b).Parent = a
    local before = pos(b)
    b.Anchored = false
    return before, pos(a), pos(b)
end)
case("anchoring before the first read decides what moves", function()
    local a, b = part("a", 0, false), part("b", 20, false)
    joint("Weld", a, b).Parent = a
    a.Anchored = true
    return pos(a), pos(b)
end)
case("a later C0", function()
    local a, b = part("a", 0, true), part("b", 20, false)
    local w = joint("Weld", a, b)
    w.Parent = a
    local before = pos(b)
    w.C0 = CFrame.new(0, 7, 0)
    return before, pos(b)
end)
case("C1 and rotation", function()
    local a, b = part("a", 0, true), part("b", 20, false)
    local w = joint("Weld", a, b, CFrame.new(3, 0, 0) * CFrame.Angles(0, math.pi / 2, 0))
    w.C1 = CFrame.new(0, 2, 0)
    w.Parent = a
    return pos(b), string.format("%.2f", b.CFrame.LookVector.X)
end)
case("disabled, then enabled with a new C0", function()
    local a, b = part("a", 0, true), part("b", 20, false)
    local w = joint("Weld", a, b)
    w.Enabled = false
    w.Parent = a
    local before = pos(b)
    w.C0 = CFrame.new(0, 9, 0)
    local after = pos(b)
    w.Enabled = true
    return before, after, pos(b)
end)
case("a joint outside Workspace", function()
    local a, b = part("a", 0, true), part("b", 20, false)
    local w = joint("Weld", a, b)
    w.Parent = game:GetService("Lighting")
    local before = pos(b)
    w:Destroy()
    return before
end)
case("a joint in a folder entering Workspace", function()
    local a, b = part("a", 0, true), part("b", 20, false)
    local folder = Instance.new("Folder")
    table.insert(made, folder)
    joint("Weld", a, b).Parent = folder
    local before = pos(b)
    folder.Parent = workspace
    return before, pos(b)
end)
case("a part out of Workspace is left until it's back", function()
    local a, b = part("a", 0, true), part("b", 20, false)
    b.Parent = nil
    joint("Weld", a, b).Parent = a
    local away = pos(b)
    b.Parent = workspace
    return away, pos(b)
end)
case("destroyed before any read", function()
    local a, b = part("a", 0, true), part("b", 20, false)
    local w = joint("Weld", a, b)
    w.Parent = a
    w:Destroy()
    return pos(b)
end)
case("the whole side moves together", function()
    local a, b, c = part("a", 0, true), part("b", 20, false), part("c", 30, false)
    joint("Weld", b, c, CFrame.new(10, 0, 0)).Parent = b
    local before = pos(c)
    joint("Weld", a, b).Parent = a
    return before, pos(b), pos(c)
end)
case("a method looking is a read too", function()
    local a, b = part("a", 0, true), part("b", 20, false)
    local m = Instance.new("Model")
    table.insert(made, m)
    m.Parent = workspace
    b.Parent = m
    m.PrimaryPart = b
    joint("Weld", a, b).Parent = a
    a.Anchored = false
    a.Anchored = true
    local at = m:GetPivot().Position
    return string.format("%.2f,%.2f,%.2f", at.X, at.Y - 1000, at.Z)
end)
case("reading the root moves nothing", function()
    local a, b = part("a", 0, false), part("b", 20, false)
    joint("Weld", a, b).Parent = a
    local seen = b.CFrame.Position.X
    a.Anchored = true
    return seen, pos(a), pos(b)
end)
for name, read in pairs({
    CFrame = function(a)
        return a.CFrame
    end,
    Position = function(a)
        return a.Position
    end,
}) do
    case("reading the part that moves: " .. name, function()
        local a, b = part("a", 0, false), part("b", 20, false)
        joint("Weld", a, b).Parent = a
        read(a)
        a.Anchored = true
        return pos(a), pos(b)
    end)
end
case("a query moves everything", function()
    local a, b = part("a", 0, false), part("b", 20, false)
    joint("Weld", a, b).Parent = a
    workspace:Raycast(Vector3.new(0, 900, 0), Vector3.new(0, 1, 0))
    a.Anchored = true
    return pos(a), pos(b)
end)
case("the assembly's center of mass looks ahead", function()
    local a, b = part("a", 0, false), part("b", 20, false)
    joint("Weld", a, b).Parent = a
    local center = b.AssemblyCenterOfMass
    a.Anchored = true
    return string.format("%.2f", center.Y - 1000), pos(a), pos(b)
end)
