-- Joints and assemblies as scripts see them: MakeJoints, GetJoints,
-- GetConnectedParts, roots, moving joined parts, BreakJoints, motors.
-- Physics needs the parts in Workspace: they go far above the place and
-- are destroyed at the end.
local made = {}
local function part(name, x, y, anchored, size)
    local p = Instance.new("Part")
    p.Name = name
    p.Anchored = anchored
    p.Size = size or Vector3.new(2, 2, 2)
    p.CFrame = CFrame.new(x, 1000 + y, 0)
    for _, s in ipairs({ "Top", "Bottom", "Left", "Right", "Front", "Back" }) do
        p[s .. "Surface"] = Enum.SurfaceType.Smooth
    end
    p.Parent = workspace
    table.insert(made, p)
    return p
end
local function n(x)
    if math.abs(x) < 0.005 then
        x = 0
    end
    return string.format("%.2f", x)
end
local function cf(c)
    local t = { c:GetComponents() }
    for i, x in ipairs(t) do
        t[i] = n(x)
    end
    return table.concat(t, " ")
end
local function pos(p)
    local v = p.Position - Vector3.new(0, 1000, 0)
    return n(v.X) .. "," .. n(v.Y) .. "," .. n(v.Z)
end
local function names(list)
    local t = {}
    for _, p in ipairs(list) do
        t[#t + 1] = p.Name
    end
    table.sort(t)
    return table.concat(t, ",")
end
local function joints(p)
    local t = {}
    for _, j in ipairs(p:GetJoints()) do
        t[#t + 1] = j.ClassName .. "(" .. j.Part0.Name .. "-" .. j.Part1.Name .. ")@" .. j.Parent.Name
    end
    table.sort(t)
    return table.concat(t, ",")
end

-- MakeJoints from surfaces.
local a = part("A", 0, 0, true)
local b = part("B", 0, 2, true)
a.TopSurface = Enum.SurfaceType.Weld
b:MakeJoints()
local w = a:GetJoints()[1]
log("weld", w.ClassName, w.Name, w.Parent.Name, w.Part0.Name, w.Part1.Name)
log("C0", cf(w.C0))
log("C1", cf(w.C1))
b:MakeJoints()
log("again", #b:GetJoints())

local c = part("C", 10, 0, true)
local d = part("D", 10, 2, true)
c.TopSurface = Enum.SurfaceType.Studs
d.BottomSurface = Enum.SurfaceType.Inlet
d:MakeJoints()
log("studs-inlet", joints(c))
c.TopSurface = Enum.SurfaceType.Hinge
d:BreakJoints()
d:MakeJoints()
log("hinge", joints(c))

-- Assemblies.
local e = part("E", 20, 0, false)
local f = part("F", 23, 0, false, Vector3.new(4, 4, 4))
local g = part("G", 26, 0, false)
local w1 = Instance.new("Weld")
w1.Part0, w1.Part1, w1.C0 = e, f, CFrame.new(3, 0, 0)
w1.Parent = e
local wc = Instance.new("WeldConstraint")
wc.Part0, wc.Part1 = f, g
wc.Parent = f
log("connected", names(e:GetConnectedParts()), names(e:GetConnectedParts(true)))
log("root", e:GetRootPart().Name, e.AssemblyRootPart.Name, "grounded", e:IsGrounded())
log("mass", n(e:GetMass()), n(e.AssemblyMass))
e.Anchored = true
log("anchored root", g.AssemblyRootPart.Name, "grounded", g:IsGrounded(), "mass", g.AssemblyMass)

-- Setting CFrame moves the assembly; Position moves one part and
-- rewrites its side of the weld.
g.CFrame = g.CFrame + Vector3.new(0, 10, 0)
log("after CFrame", pos(e), pos(f), pos(g))
f.Position = f.Position + Vector3.new(0, 5, 0)
log("after Position", pos(e), pos(f), pos(g), "C0", cf(w1.C0), "C1", cf(w1.C1))

-- BreakJoints takes WeldConstraints too.
f:BreakJoints()
log("broken", #e:GetJoints(), #g:GetJoints(), w1.Parent, wc.Parent)

-- Model:MakeJoints joins parts outside the model too.
local m = Instance.new("Model")
m.Parent = workspace
table.insert(made, m)
local h = part("H", 40, 0, true)
local i = part("I", 40, 2, true)
local k = part("K", 42, 0, true)
h.Parent, i.Parent = m, m
for _, p in ipairs({ h, i, k }) do
    p.RightSurface = Enum.SurfaceType.Weld
    p.TopSurface = Enum.SurfaceType.Weld
end
m:MakeJoints()
log("model", joints(h), "|", joints(k))
m:BreakJoints()
log("model broken", #h:GetJoints(), #k:GetJoints())

-- Each anchored part roots its own assembly; an unanchored part between
-- two joins the newer joint's.
local q = part("Q", 80, 0, true)
local r = part("R", 82, 0, false)
local t = part("T", 84, 0, true)
for _, pair in ipairs({ { q, r }, { r, t } }) do
    local wq = Instance.new("Weld")
    wq.Part0, wq.Part1, wq.C0 = pair[1], pair[2], pair[1].CFrame:Inverse() * pair[2].CFrame
    wq.Parent = pair[1]
end
log("anchored roots", q.AssemblyRootPart.Name, r.AssemblyRootPart.Name, t.AssemblyRootPart.Name, #q:GetConnectedParts(true))
t.CFrame = t.CFrame + Vector3.new(0, 10, 0)
log("moved T", pos(q), pos(r), pos(t))

-- Joints outside Workspace don't count.
local storage = game:GetService("ServerStorage")
local s = Instance.new("Part")
s.Parent = storage
log("outside", s.AssemblyRootPart, s:GetRootPart(), #s:GetConnectedParts(), #s:GetJoints())
s:Destroy()

-- Motors turn toward DesiredAngle.
local p0 = part("P0", 60, 0, true, Vector3.new(3, 3, 3))
local p1 = part("P1", 60, 4, false)
local mo = Instance.new("Motor")
mo.Part0, mo.Part1, mo.C0, mo.C1 = p0, p1, CFrame.new(4, 0, 0), CFrame.new(0, 1, 0)
mo.MaxVelocity = 0.1
mo:SetDesiredAngle(1)
mo.Parent = p0
log("desired", n(mo.DesiredAngle), n(mo.CurrentAngle))
task.wait(1)
log("turned", n(mo.CurrentAngle), "P1", cf(p0.CFrame:Inverse() * p1.CFrame))

for _, p in ipairs(made) do
    p:Destroy()
end
