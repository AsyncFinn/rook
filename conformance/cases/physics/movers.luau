-- BodyMovers and RocketPropulsion, compared once they've settled.
-- Physics needs the parts in Workspace: they go far above the place, each
-- in its own spot, and are destroyed at the end.
local made = {}
local function part(x, anchored, size)
    local p = Instance.new("Part")
    p.Anchored = anchored
    p.Size = size or Vector3.new(2, 2, 2)
    p.CFrame = CFrame.new(x, 1000, 0)
    p.Parent = workspace
    if not anchored then
        p:SetNetworkOwner(nil)
    end
    table.insert(made, p)
    return p
end
local function mover(class, parent, props)
    local m = Instance.new(class)
    for k, v in pairs(props) do
        m[k] = v
    end
    m.Parent = parent
    return m
end
local function n(x, places)
    local s = string.format("%." .. (places or 1) .. "f", x)
    if tonumber(s) == 0 then
        s = string.format("%." .. (places or 1) .. "f", 0)
    end
    return s
end
local function v(p, ...)
    local t = {}
    for _, x in ipairs({ p.X, p.Y, p.Z }) do
        t[#t + 1] = n(x, ...)
    end
    return table.concat(t, ",")
end

-- BodyVelocity reaches its target and holds against gravity.
local bv = part(0, false)
mover("BodyVelocity", bv, { Velocity = Vector3.new(10, 0, 0), MaxForce = Vector3.new(1e9, 1e9, 1e9), P = 10 })
-- With no force allowed on Y it falls while moving along X.
local bvx = part(100, false)
mover("BodyVelocity", bvx, { Velocity = Vector3.new(0, 0, 10), MaxForce = Vector3.new(0, 0, 1e9) })

-- BodyPosition and BodyGyro settle on their targets, whatever the mass.
local bp = part(200, false)
mover("BodyPosition", bp, { Position = Vector3.new(210, 1005, 3) })
local bpBig = part(220, false, Vector3.new(4, 4, 4))
mover("BodyPosition", bpBig, { Position = Vector3.new(220, 1005, 3), MaxForce = Vector3.new(1e5, 1e5, 1e5) })
local bg = part(300, false)
mover("BodyPosition", bg, { Position = Vector3.new(300, 1000, 0), MaxForce = Vector3.new(1e9, 1e9, 1e9) })
mover("BodyGyro", bg, { CFrame = CFrame.Angles(0, 1, 0), MaxTorque = Vector3.new(1e9, 1e9, 1e9) })

-- An anchored part doesn't move.
local anchored = part(400, true)
mover("BodyPosition", anchored, { Position = Vector3.new(410, 1000, 0) })

-- A rocket flies to its target and stays there.
local target = part(600, true, Vector3.new(1, 1, 1))
local rp = part(500, false)
local rocket = mover("RocketPropulsion", rp, { Target = target })
local reached = 0
rocket.ReachedTarget:Connect(function()
    reached += 1
end)
rocket:Fire()

task.wait(0.5)
log("BodyVelocity", v(bv.AssemblyLinearVelocity))
log("BodyVelocity X only", n(bvx.AssemblyLinearVelocity.Z), bvx.AssemblyLinearVelocity.Y < -50)
log("rocket flying", reached, rp.AssemblyLinearVelocity.X > 25, math.abs(rp.AssemblyLinearVelocity.Y) < 6)

task.wait(4.5)
log("BodyPosition", v(bp.Position), v(bpBig.Position))
log("BodyGyro", v(Vector3.new(bg.CFrame:ToEulerAnglesYXZ()), 2))
log("anchored", v(anchored.Position))
log("rocket", reached, (rp.Position - target.Position).Magnitude <= rocket.TargetRadius, n(rp.AssemblyLinearVelocity.Magnitude))
rocket:Abort()
task.wait(0.5)
log("aborted", rp.AssemblyLinearVelocity.Y < -50)

for _, p in ipairs(made) do
    p:Destroy()
end
