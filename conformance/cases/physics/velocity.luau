-- The deprecated Velocity and RotVelocity against the assembly velocities,
-- and anchored parts with a velocity carrying what rests on them.
-- Physics needs the parts in Workspace: they go far above the place and
-- are destroyed at the end.
local made = {}
local function part(x, y, anchored, size)
    local p = Instance.new("Part")
    p.Anchored = anchored
    p.Size = size or Vector3.new(2, 2, 2)
    p.CFrame = CFrame.new(x, 1000 + y, 0)
    p.Parent = workspace
    if not anchored then
        p:SetNetworkOwner(nil)
    end
    table.insert(made, p)
    return p
end
local function v(x)
    local t = {}
    for _, c in ipairs({ x.X, x.Y, x.Z }) do
        local s = string.format("%.1f", c)
        t[#t + 1] = tonumber(s) == 0 and "0.0" or s
    end
    return table.concat(t, ",")
end

-- Setting Velocity sets the assembly's velocity at once.
local a = part(0, 0, false)
a.Velocity = Vector3.new(50, 0, 0)
log("Velocity", v(a.AssemblyLinearVelocity), v(a.Velocity))
a.RotVelocity = Vector3.new(0, 3, 0)
log("RotVelocity", v(a.AssemblyAngularVelocity), v(a.RotVelocity))
a.AssemblyLinearVelocity = Vector3.new(0, 0, 20)
log("AssemblyLinearVelocity", v(a.Velocity))

-- On a welded part, Velocity is the assembly's velocity at that part.
local root = part(20, 0, false, Vector3.new(4, 4, 4))
local arm = part(24, 0, false)
local w = Instance.new("WeldConstraint")
w.Part0, w.Part1 = root, arm
w.Parent = root
arm.Velocity = Vector3.new(0, 0, 7)
log("welded", v(root.AssemblyLinearVelocity), v(root.Velocity))
root.AssemblyLinearVelocity = Vector3.zero
root.AssemblyAngularVelocity = Vector3.new(0, 2, 0)
log("spinning", v(arm.Velocity), v(arm.AssemblyLinearVelocity), v(arm.RotVelocity))

-- The launched part keeps going.
local launched = part(40, 0, false)
launched.Velocity = Vector3.new(30, 0, 0)

-- An anchored belt keeps its velocity and carries a box.
local belt = part(60, 0, true, Vector3.new(30, 1, 4))
belt.Velocity = Vector3.new(10, 0, 0)
local box = part(50, 1.5, false)

task.wait(1)
log("launched", launched.Position.X - 40 > 25)
log("belt", v(belt.Velocity), v(belt.Position - Vector3.new(60, 1000, 0)))
local carried = box.AssemblyLinearVelocity
log("box", math.abs(carried.X - 10) < 0.3, math.abs(carried.Z) < 0.1, box.Position.X - 50 > 8)

for _, p in ipairs(made) do
    p:Destroy()
end
