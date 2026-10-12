-- Scene queries: legacy rays and regions, Raycast and its params, shape
-- casts, overlaps, touching parts and camera rays. Parts go far above the
-- place and are destroyed at the end; the group made is unregistered.
local PS = game:GetService("PhysicsService")
local made = {}
local O = Vector3.new(0, 1000, 0)
local function part(name, at, size, props, class)
    local p = Instance.new(class or "Part")
    p.Name = name
    p.Anchored = true
    p.Size = size or Vector3.new(2, 2, 2)
    p.CFrame = typeof(at) == "CFrame" and at or CFrame.new(O + at)
    for k, v in pairs(props or {}) do
        p[k] = v
    end
    p.Parent = workspace
    table.insert(made, p)
    return p
end
local function r(x)
    return string.format("%.2f", x)
end
local function v(x)
    return r(x.X) .. "," .. r(x.Y - (math.abs(x.Y) > 500 and 1000 or 0)) .. "," .. r(x.Z)
end
local function names(list)
    local t = {}
    for _, p in ipairs(list) do
        t[#t + 1] = p.Name
    end
    table.sort(t)
    return "[" .. table.concat(t, ",") .. "]"
end
local function ray(name, from, dir, ...)
    local hit, pos, normal, material = workspace:FindPartOnRay(Ray.new(O + from, dir), ...)
    log(name, hit and hit.Name, v(pos), v(normal), material)
end
local function try(name, f)
    local ok, err = pcall(f)
    log(name, ok, err)
end
local Z = Vector3.new(0, 0, -100)

-- Legacy rays.
local a = part("A", Vector3.new(0, 0, -10))
ray("basic", Vector3.zero, Z)
ray("short", Vector3.zero, Vector3.new(0, 0, -5))
log("results", select("#", workspace:FindPartOnRay(Ray.new(O, Vector3.new(0, 0, -5)))))
log("lowercase", (workspace:findPartOnRay(Ray.new(O, Z))))
a.CanQuery = false
ray("canquery off", Vector3.zero, Z)
a.CanCollide = false
ray("both off", Vector3.zero, Z)
a.CanQuery = true
ray("cancollide off", Vector3.zero, Z)
a.CanCollide = true
a.Transparency = 1
ray("transparent", Vector3.zero, Z)
ray("inside", Vector3.new(0, 0, -10), Z)
ray("on surface", Vector3.new(0, 0, -9), Z)
part("P1", Vector3.new(20, 0, 0), Vector3.new(4, 4, 4))
part("P2", Vector3.new(20, 0, -3), Vector3.new(4, 4, 4))
ray("start in overlap", Vector3.new(20, 0, 0), Z)
local far = part("Far", Vector3.new(40, 0, -14999))
ray("15000", Vector3.new(40, 0, 0), Vector3.new(0, 0, -100000))
far.CFrame = CFrame.new(O + Vector3.new(40, 0, -15002))
ray("15002", Vector3.new(40, 0, 0), Vector3.new(0, 0, -100000))
local b = part("B", Vector3.new(50, 0, -10))
b.CFrame = CFrame.new(O + Vector3.new(50, 0, -20))
ray("moved this frame", Vector3.new(50, 0, 0), Z)
local m = Instance.new("Model")
m.Name = "M"
m.Parent = workspace
table.insert(made, m)
local c = part("C", Vector3.new(100, 0, -10), nil, { Material = Enum.Material.Wood })
c.Parent = m
part("D", Vector3.new(100, 0, -20))
local R = Ray.new(O + Vector3.new(100, 0, 0), Z)
log("ignore model", (workspace:FindPartOnRay(R, m)))
log("ignore list", (workspace:FindPartOnRayWithIgnoreList(R, { m })))
log("whitelist", (workspace:FindPartOnRayWithWhitelist(R, { m })))
log("whitelist empty", (workspace:FindPartOnRayWithWhitelist(R, {})))
log("material", select(4, workspace:FindPartOnRay(R)))
local ball = part("Ball", Vector3.new(150, 0, -10), Vector3.new(4, 4, 4), { Shape = Enum.PartType.Ball })
ray("ball", Vector3.new(151, 0, 0), Z)
ray("ball corner", Vector3.new(151.9, 1.9, 0), Z)
part("W", Vector3.new(200, 0, 0), nil, nil, "WedgePart")
ray("wedge", Vector3.new(200, 10, 0.9), Vector3.new(0, -20, 0))
try("no ray", function()
    return workspace:FindPartOnRay(5)
end)
try("no list", function()
    return workspace:FindPartOnRayWithIgnoreList(R)
end)

-- Collision groups: legacy rays act as Default; Raycast uses its params'.
PS:RegisterCollisionGroup("RookQ")
PS:CollisionGroupSetCollidable("RookQ", "Default", false)
local g = part("G", Vector3.new(250, 0, -10), nil, { CollisionGroup = "RookQ" })
local nc = part("NC", Vector3.new(250, 0, -5), nil, { CanCollide = false })
part("Plain", Vector3.new(250, 0, -15))
local G = O + Vector3.new(250, 0, 0)
local function cast(name, params)
    local res = workspace:Raycast(G, Z, params)
    log(name, res and res.Instance.Name)
end
nc.Parent = nil
ray("legacy group", Vector3.new(250, 0, 0), Z)
nc.Parent = workspace
local p = RaycastParams.new()
log("params", p)
log("fields", p.FilterType, #p.FilterDescendantsInstances, p.IgnoreWater, p.CollisionGroup, p.RespectCanCollide, p.BruteForceAllSlow)
cast("default", p)
cast("no params", nil)
p.RespectCanCollide = true
cast("respect", p)
p.RespectCanCollide = false
p.CollisionGroup = "RookQ"
cast("group", p)
p.CollisionGroup = "Nope"
cast("unknown group", p)
p.CollisionGroup = "Default"
p.FilterType = Enum.RaycastFilterType.Include
p.FilterDescendantsInstances = { g, nc }
cast("include", p)
p.FilterType = Enum.RaycastFilterType.Blacklist
p.FilterDescendantsInstances = { { nc }, {} }
cast("nested exclude", p)
log("blacklist", p.FilterType, Enum.RaycastFilterType.Whitelist == Enum.RaycastFilterType.Include)
log("one", p)
p:AddToFilter({ g, { m } })
log("many", p, #p.FilterDescendantsInstances)
local list = {}
p.FilterDescendantsInstances = list
table.insert(list, g)
log("copied", #p.FilterDescendantsInstances)
log("equal", RaycastParams.new() == RaycastParams.new())
try("bad member", function()
    return p.Foo
end)
try("enum number", function()
    p.FilterType = 1
end)
try("not a table", function()
    p.FilterDescendantsInstances = 5
end)
try("not instances", function()
    p.FilterDescendantsInstances = { 1 }
end)
try("add bad", function()
    p:AddToFilter(5)
end)
p.IgnoreWater = 1
log("truthy", p.IgnoreWater)
local res = workspace:Raycast(G, Z)
log("result", typeof(res), res.Instance, v(res.Position), v(res.Normal), res.Material, r(res.Distance))
log("result tostring", workspace:Raycast(O + Vector3.new(0, 0, 20), Vector3.new(0, 0, -40)))
try("result member", function()
    return res.Foo
end)
try("result write", function()
    res.Distance = 1
end)
log("miss", workspace:Raycast(O + Vector3.new(0, 500, 0), Vector3.new(0, 10, 0)))
try("raycast bad", function()
    return workspace:Raycast(5, Z)
end)
try("raycast params bad", function()
    return workspace:Raycast(G, Z, 5)
end)

-- OverlapParams and the overlap queries.
local op = OverlapParams.new()
log("overlap", op)
op.MaxParts = 2.7
log("maxparts", op.MaxParts)
op.MaxParts = 0
try("tolerance", function()
    op.Tolerance = 0.5
end)
local bc = CFrame.new(G + Vector3.new(0, 0, -10))
local function box(name, params)
    log(name, names(workspace:GetPartBoundsInBox(bc, Vector3.new(2, 2, 14), params)))
end
box("box", op)
op.RespectCanCollide = true
box("box respect", op)
op.RespectCanCollide = false
op.CollisionGroup = "RookQ"
box("box group", op)
op.CollisionGroup = "Default"
op.MaxParts = 1
-- Which part comes first is Roblox's own order; only the count is logged.
log("box max", #workspace:GetPartBoundsInBox(bc, Vector3.new(2, 2, 14), op))
op.MaxParts = 0
log("radius", names(workspace:GetPartBoundsInRadius(G + Vector3.new(0, 0, -10), 4)))
log("radius negative", names(workspace:GetPartBoundsInRadius(G, -1)))
local bp = part("BP", Vector3.new(300, 0, 0))
log("radius corner", names(workspace:GetPartBoundsInRadius(O + Vector3.new(301.5, 1.5, 1.5), 0.9)), names(workspace:GetPartBoundsInRadius(O + Vector3.new(301.5, 1.5, 1.5), 0.8)))
log("box ball corner", names(workspace:GetPartBoundsInBox(CFrame.new(O + Vector3.new(151.85, 1.85, -8.15)), Vector3.new(0.2, 0.2, 0.2))))
local probe = part("Probe", Vector3.new(151.85, 1.85, -8.15), Vector3.new(0.2, 0.2, 0.2))
log("in part corner", names(workspace:GetPartsInPart(probe)))
probe.CFrame = CFrame.new(O + Vector3.new(151, 0, -10))
log("in part", names(workspace:GetPartsInPart(probe)), names(workspace:GetPartsInPart(ball)))
try("in part bad", function()
    return workspace:GetPartsInPart(workspace)
end)

-- Shape casts.
local wall = part("Wall", Vector3.new(350, 0, -10), Vector3.new(10, 10, 1))
local S = O + Vector3.new(350, 0, 0)
local sc = workspace:Spherecast(S, 1, Z)
log("sphere", sc.Instance, r(sc.Distance), v(sc.Position), v(sc.Normal))
local bk = workspace:Blockcast(CFrame.new(S), Vector3.new(2, 2, 2), Z)
log("block", bk.Instance, r(bk.Distance), v(bk.Normal))
log("start inside", workspace:Spherecast(S + Vector3.new(0, 0, -9), 1, Z))
try("radius", function()
    return workspace:Spherecast(S, 600, Z)
end)
try("side", function()
    return workspace:Blockcast(CFrame.new(S), Vector3.new(1600, 1, 1), Z)
end)
try("distance", function()
    return workspace:Spherecast(S, 1, Vector3.new(0, 0, -2000))
end)

-- Touching parts.
local t = part("T", Vector3.new(400, 0, 0))
part("Over", Vector3.new(401, 0, 0))
part("Face", Vector3.new(400, 2, 0))
local ghost = part("Ghost", Vector3.new(400, 0, 1), nil, { CanCollide = false })
part("Shy", Vector3.new(400, 0, -1), nil, { CanTouch = false })
log("touching", names(t:GetTouchingParts()), names(ghost:GetTouchingParts()))
local conn = ghost.Touched:Connect(function() end)
log("touching listened", names(ghost:GetTouchingParts()))
conn:Disconnect()
local loose = Instance.new("Part")
log("touching outside", names(loose:GetTouchingParts()), names(t:GetNoCollisionConstraints()))
loose:Destroy()

-- Region3, a frame after the parts were made (Studio sometimes misses
-- parts made in the same frame).
for i = 1, 25 do
    part("N" .. i, Vector3.new(450, i * 3, 0))
end
task.wait()
local function region(a, b, ...)
    return workspace:FindPartsInRegion3(Region3.new(O + a, O + b), ...)
end
log("region", names(region(Vector3.new(95, -5, -25), Vector3.new(105, 5, 5))))
log("region lower", names(workspace:findPartsInRegion3(Region3.new(O + Vector3.new(95, -5, -25), O + Vector3.new(105, 5, 5)))))
log("region face", names(region(Vector3.new(101, -1, -11), Vector3.new(102, 1, -9))))
log("region gap", names(region(Vector3.new(101.01, -1, -11), Vector3.new(102, 1, -9))))
log("region ball corner", names(region(Vector3.new(151.7, 1.7, -8.3), Vector3.new(152, 2, -8))))
local rot = part("Rot", CFrame.new(O + Vector3.new(500, 0, 0)) * CFrame.Angles(0, math.rad(45), 0), Vector3.new(4, 4, 4))
task.wait()
log("region rotated", names(region(Vector3.new(502.5, -1, 2.5), Vector3.new(502.8, 1, 2.8))), names(region(Vector3.new(501.2, -1, -0.1), Vector3.new(501.3, 1, 0.1))))
log("region flags", names(region(Vector3.new(240, -5, -20), Vector3.new(260, 5, 0))))
log("region ignore", names(region(Vector3.new(95, -5, -25), Vector3.new(105, 5, 5), m)))
log("region lists", names(workspace:FindPartsInRegion3WithIgnoreList(Region3.new(O + Vector3.new(95, -5, -25), O + Vector3.new(105, 5, 5)), { m })), names(workspace:FindPartsInRegion3WithWhiteList(Region3.new(O + Vector3.new(95, -5, -25), O + Vector3.new(105, 5, 5)), { m })))
local N = { Vector3.new(440, -10, -10), Vector3.new(460, 200, 10) }
log("region max", #region(N[1], N[2]), #region(N[1], N[2], nil, 100), #region(N[1], N[2], nil, 0), #region(N[1], N[2], nil, 3))
log("region inverted", #region(N[2], N[1]))
log("empty", workspace:IsRegion3Empty(Region3.new(O + N[1], O + N[2])), workspace:IsRegion3Empty(Region3.new(O + Vector3.new(600, 0, 0), O + Vector3.new(610, 10, 10))))
log("empty ignore", workspace:IsRegion3EmptyWithIgnoreList(Region3.new(O + Vector3.new(95, -5, -15), O + Vector3.new(105, 5, -5)), { m }), workspace:IsRegion3Empty(Region3.new(O + Vector3.new(95, -5, -15), O + Vector3.new(105, 5, -5)), m))
try("region bad", function()
    return workspace:FindPartsInRegion3(5)
end)

-- Camera rays (a server camera's viewport is 1 by 1).
local cam = Instance.new("Camera")
cam.CFrame = CFrame.new(10, 20, 30)
cam.FieldOfView = 70
local function cray(name, rr)
    log(name, v(rr.Origin), v(rr.Direction))
end
cray("center", cam:ViewportPointToRay(0.5, 0.5))
cray("depth", cam:ViewportPointToRay(0.5, 0.5, 5))
cray("corner", cam:ViewportPointToRay(0, 0))
cray("corner depth", cam:ViewportPointToRay(0, 0, 10))
cray("screen", cam:ScreenPointToRay(1, 1))
local sp, on = cam:WorldToViewportPoint(Vector3.new(10, 20, 0))
log("project", v(sp), on, select(2, cam:WorldToScreenPoint(Vector3.new(10, 20, 40))))
cam:Destroy()

PS:UnregisterCollisionGroup("RookQ")
for _, x in ipairs(made) do
    x:Destroy()
end
