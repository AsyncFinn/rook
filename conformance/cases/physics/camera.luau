-- Camera methods beyond rays: the occlusion queries, roll and the render
-- CFrame, the legacy orbit steps, ZoomToExtents and the inert deprecated
-- ones. A server's cameras have a 1 by 1 viewport and a 0.5 near plane.
-- Parts go far above the place and are destroyed at the end; the group
-- made is unregistered.
local PS = game:GetService("PhysicsService")
local O = Vector3.new(0, 1000, 0)
local made = {}
local function part(name, at, size, props)
    local p = Instance.new("Part")
    p.Name = name
    p.Anchored = true
    p.Size = size
    p.CFrame = CFrame.new(O + at)
    for k, x in pairs(props or {}) do
        p[k] = x
    end
    p.Parent = workspace
    table.insert(made, p)
    return p
end
-- Numbers to 3 places; one that rounds to zero prints as 0, whatever
-- its sign, so float noise around zero doesn't show.
local function r(x, places)
    local f = "%." .. (places or 3) .. "f"
    local text = string.format(f, x)
    return if tonumber(text) == 0 then string.format(f, 0) else text
end
local function v(x)
    return r(x.X) .. "," .. r(x.Y) .. "," .. r(x.Z)
end
local function names(list)
    local t = {}
    for i, p in ipairs(list) do
        t[i] = p.Name
    end
    return "[" .. table.concat(t, ",") .. "]"
end
local function try(name, f)
    local ok, res = pcall(f)
    log(name, ok, res)
end
-- Heading and pitch of a camera's look, its distance from its Focus.
local function aim(c)
    local l = c.CFrame.LookVector
    return "yaw "
        .. r(math.deg(math.atan2(-l.X, -l.Z)), 2)
        .. " pitch "
        .. r(math.deg(math.asin(l.Y)), 2)
        .. " d "
        .. r((c.CFrame.Position - c.Focus.Position).Magnitude)
end

log("near plane", workspace.CurrentCamera.NearPlaneZ, Instance.new("Camera").NearPlaneZ)

-- GetLargestCutoffDistance
PS:RegisterCollisionGroup("RookCam")
PS:CollisionGroupSetCollidable("RookCam", "Default", false)
local wall = part("Wall", Vector3.new(0, 0, 10), Vector3.new(40, 40, 1))
local cam = Instance.new("Camera")
cam.Parent = workspace
task.wait()
local function cut(name, cz, focus, turn, list)
    cam.CFrame = CFrame.new(O + Vector3.new(0, 0, cz)) * (turn or CFrame.identity)
    cam.Focus = CFrame.new(O + focus)
    log("cutoff " .. name, r(cam:GetLargestCutoffDistance(list or {})))
end
local Z = Vector3.zero
cut("base", 20, Z)
cut("ignored", 20, Z, nil, { wall })
cut("focus in front", 20, Vector3.new(0, 0, 15))
cut("focus in wall", 20, Vector3.new(0, 0, 9.6))
cut("focus behind wall", 20, Vector3.new(0, 0, 9))
cut("far", 40, Z)
cut("focus aside", 20, Vector3.new(3, 0, 0))
cut("turned", 20, Z, CFrame.Angles(0, math.rad(90), 0))
cut("aside turned", 20, Vector3.new(3, 0, 0), CFrame.Angles(0, math.rad(90), 0))
cut("focus at camera", 20, Vector3.new(0, 0, 20))
cut("focus behind camera", 20, Vector3.new(0, 0, 30))
local loose = Instance.new("Camera")
loose.CFrame = CFrame.new(O + Vector3.new(0, 0, 20))
loose.Focus = CFrame.new(O)
log("cutoff unparented", r(loose:GetLargestCutoffDistance({})))
loose:Destroy()
wall.CanCollide = false
cut("no collide", 20, Z)
wall.CanQuery = false
cut("no query", 20, Z)
wall.CanCollide, wall.CanQuery = true, true
wall.Transparency = 0.96
cut("transparency 0.96", 20, Z)
wall.Transparency = 0.95
cut("transparency 0.95", 20, Z)
wall.Transparency = 0
wall.CollisionGroup = "RookCam"
cut("group", 20, Z)
wall.CollisionGroup = "Default"
local m = Instance.new("Model")
m.Parent = workspace
wall.Parent = m
cut("ignore model", 20, Z, nil, { m })
wall.Parent = workspace
m:Destroy()
wall.Size = Vector3.new(0.02, 0.02, 1)
for _, at in ipairs({ Vector3.new(0, 0, 10), Vector3.new(0.35, 0.35, 10), Vector3.new(-0.35, 0.35, 10), Vector3.new(0.17, 0, 10), Vector3.new(0, 0.17, 10) }) do
    wall.Position = O + at
    cut("dot " .. r(at.X) .. "," .. r(at.Y), 20, Z)
end
wall:Destroy()
try("cutoff nil", function()
    return cam:GetLargestCutoffDistance()
end)
try("cutoff number", function()
    return cam:GetLargestCutoffDistance(5)
end)

-- GetPartsObscuringTarget
cam.CFrame = CFrame.new(O + Vector3.new(0, 0, 20))
local box = Vector3.new(1, 1, 0.5)
local near1 = part("Near1", Vector3.new(0, 0, 15), box)
part("Far1", Vector3.new(0, 0, 5), box)
part("Near2", Vector3.new(1.25, 0, 17.5), box)
part("Far2", Vector3.new(6.25, 0, 7.5), box)
task.wait()
local p1, p2 = O, O + Vector3.new(10, 0, 0)
local function ob(name, points, ignore)
    log("obscuring " .. name, names(cam:GetPartsObscuringTarget(points, ignore or {})))
end
ob("p1", { p1 })
ob("p2", { p2 })
ob("p1 p2", { p1, p2 })
ob("p2 p1", { p2, p1 })
ob("twice", { p1, p1 })
ob("none", {})
ob("ignore", { p1 }, { near1 })
ob("mixed", { p1, "x", p2 })
ob("at face", { O + Vector3.new(0, 0, 15.25) })
ob("past face", { O + Vector3.new(0, 0, 14.75) })
ob("inside", { O + Vector3.new(0, 0, 15) })
ob("far", { O + Vector3.new(0, 0, -20000) })
local both = part("Both", Vector3.new(0, 0, 19), Vector3.new(20, 2, 0.2))
local around = part("Around", Vector3.new(0, 0, 20), Vector3.new(4, 4, 4))
task.wait()
ob("shared", { p2, p1 })
around:Destroy()
both:Destroy()
near1.CanCollide = false
near1.Transparency = 1
ob("ghost", { p1 })
near1.CanQuery = false
ob("no query", { p1 })
near1.CanQuery = true
near1.CanCollide = true
near1.Transparency = 0
near1.CollisionGroup = "RookCam"
ob("group", { p1 })
near1.CollisionGroup = "Default"
workspace.Terrain:FillBlock(CFrame.new(O + Vector3.new(0, 0, 10)), Vector3.new(20, 20, 4), Enum.Material.Rock)
ob("terrain", { O + Vector3.new(0, 0, 10) })
workspace.Terrain:FillBlock(CFrame.new(O + Vector3.new(0, 0, 10)), Vector3.new(20, 20, 4), Enum.Material.Air)
local loose2 = Instance.new("Camera")
loose2.CFrame = cam.CFrame
log("obscuring unparented", names(loose2:GetPartsObscuringTarget({ p1 }, {})))
loose2:Destroy()
try("obscuring number", function()
    return cam:GetPartsObscuringTarget(5, {})
end)
try("obscuring nil", function()
    return cam:GetPartsObscuringTarget(nil, {})
end)
try("obscuring ignore number", function()
    return cam:GetPartsObscuringTarget({ p1 }, 5)
end)
try("obscuring ignore nil", function()
    return cam:GetPartsObscuringTarget({ p1 })
end)
cam:Destroy()

-- Roll and the render CFrame
for _, kind in ipairs({ Enum.CameraType.Scriptable, Enum.CameraType.Custom }) do
    local c = Instance.new("Camera")
    c.CameraType = kind
    c.CFrame = CFrame.new(1, 2, 3) * CFrame.Angles(0, 0, 0.2)
    log("roll of a rolled CFrame", kind, c:GetRoll())
    c:SetRoll(0.5)
    local rc = c:GetRenderCFrame()
    log("roll", kind, r(c:GetRoll()), v(rc.Position), v(rc.RightVector), v(rc.UpVector), c.CFrame == CFrame.new(1, 2, 3) * CFrame.Angles(0, 0, 0.2))
    c:SetRoll(7)
    log("roll 7", kind, r(c:GetRoll()))
    c.CameraType = Enum.CameraType.Fixed
    c.CameraType = kind
    log("roll after type change", kind, c:GetRoll(), c:GetRenderCFrame() == c.CFrame)
    c:Destroy()
end
try("roll nil", function()
    return Instance.new("Camera"):SetRoll()
end)

-- PanUnits and TiltUnits orbit the Focus
local o = Instance.new("Camera")
o.Focus = CFrame.new()
local function orbit(yaw, pitch, dist)
    o.CFrame = CFrame.Angles(0, math.rad(yaw), 0) * CFrame.Angles(math.rad(pitch), 0, 0) * CFrame.new(0, 0, dist or 10)
end
for _, case in ipairs({ { 0, 1 }, { 0, -1 }, { 0, 2 }, { 10, 1 }, { 22, 0 }, { 23, 0 }, { 89, 1 }, { -170, 1 }, { 0, 1.7 }, { 0, -1.7 } }) do
    orbit(case[1], -20)
    local res = o:PanUnits(case[2])
    log("pan", case[1], case[2], res, aim(o))
end
for _, case in ipairs({ { 0, 1 }, { 0, -1 }, { 0, 0 }, { 7, 0 }, { -20, 1 }, { 30, 10 }, { 80, -10 }, { -89, 0 }, { 80, 1 }, { 0, 1.5 } }) do
    orbit(17, case[1])
    local res = o:TiltUnits(case[2])
    log("tilt", case[1], case[2], res, aim(o))
end
o.CFrame = CFrame.lookAt(Vector3.new(0, 10, 0), Vector3.zero)
local down = o.CFrame
o:PanUnits(1)
log("pan looking down", o.CFrame == down)
orbit(17, -20, 0)
o:PanUnits(1)
log("pan on focus", aim(o))
o.Focus = CFrame.new(5, 0, 0) * CFrame.Angles(0, 1, 0)
o.CFrame = CFrame.new(0, 0, 10) * CFrame.Angles(0, 0.3, 0.4)
o:PanUnits(1)
log("pan rolled", v(o.CFrame.Position), v(o.CFrame.UpVector), v(o.Focus.Position))
o.Focus = CFrame.new()
o.CFrame = CFrame.new(10, 20, 30) * CFrame.Angles(0.3, 0.5, 0.2)
o:TiltUnits(1)
log("tilt rolled", aim(o), v(o.CFrame.UpVector))
try("pan nil", function()
    return o:PanUnits()
end)
try("tilt string", function()
    return o:TiltUnits("x")
end)

-- ZoomToExtents
local start = CFrame.Angles(0, math.rad(30), 0) * CFrame.Angles(math.rad(-20), 0, 0) * CFrame.new(0, 0, 10)
local function zoom(name, at, size, fov, from)
    o.FieldOfView = fov or 70
    o.Focus = CFrame.new(1, 2, 3)
    o.CFrame = from or start
    local ok, err = pcall(function()
        o:ZoomToExtents(at, size)
    end)
    log("zoom " .. name, ok, err, aim(o), v(o.Focus.Position), o.Focus.Rotation == CFrame.identity)
end
zoom("cube", CFrame.new(), Vector3.new(4, 4, 4))
zoom("fov 30", CFrame.new(), Vector3.new(4, 4, 4), 30)
zoom("fov 120", CFrame.new(), Vector3.new(4, 4, 4), 120)
zoom("long", CFrame.new(), Vector3.new(20, 1, 1))
zoom("turned box", CFrame.new(5, 6, 7) * CFrame.Angles(0, 1, 0), Vector3.new(20, 1, 1))
zoom("no size", CFrame.new(5, 0, 0), Vector3.zero)
zoom("negative", CFrame.new(), Vector3.new(-4, -4, -4))
zoom("rolled camera", CFrame.new(), Vector3.new(4, 4, 4), 70, CFrame.new(3, 4, 5) * CFrame.Angles(0.2, 0.4, 0.9))
log("zoom keeps roll", v(o.CFrame.UpVector))
zoom("nil box", nil, Vector3.new(4, 4, 4))
zoom("nil size", CFrame.new(), nil)
o.FieldOfView = 70

-- Deprecated and inert
log("speeds", o:GetPanSpeed(), o:GetTiltSpeed())
try("pan mode", function()
    o:SetCameraPanMode(Enum.CameraPanMode.EdgeBump)
    o:SetCameraPanMode()
    return aim(o)
end)
o:Destroy()

PS:UnregisterCollisionGroup("RookCam")
for _, x in ipairs(made) do
    x:Destroy()
end
