-- Materials, collision groups and touches as scripts see them.
-- Physics needs the parts in Workspace: they go far above the place and
-- are destroyed at the end, and every group made is unregistered.
local PS = game:GetService("PhysicsService")
local made = {}
local function part(name, x, y, anchored, size)
    local p = Instance.new("Part")
    p.Name = name
    p.Anchored = anchored
    p.Size = size or Vector3.new(2, 2, 2)
    p.CFrame = CFrame.new(x, 1000 + y, 0)
    table.insert(made, p)
    return p
end
local function n(x)
    return string.format("%.4f", x)
end
local function try(name, f)
    local ok, err = pcall(f)
    log(name, ok, err)
end

-- PhysicalProperties.
log("wood", PhysicalProperties.new(Enum.Material.Wood))
log("rubber", PhysicalProperties.new(Enum.Material.Rubber))
log("clamped", PhysicalProperties.new(1000000, -1, 5, 1000, -1, 5))
log("five", PhysicalProperties.new(1, 0.5, 0.25, 2, 3))
local pp = PhysicalProperties.new(Enum.Material.Ice)
log("fields", pp.Density, pp.Friction, pp.Elasticity, pp.FrictionWeight, pp.ElasticityWeight, n(pp.AcousticAbsorption))
log("two args", (pcall(PhysicalProperties.new, 1, 2)))

-- Mass.
local m = part("M", 0, 0, true)
log("plastic", n(m:GetMass()))
m.Material = Enum.Material.Metal
log("metal", n(m:GetMass()))
m.CustomPhysicalProperties = PhysicalProperties.new(2, 0.1, 0.9)
log("custom", n(m:GetMass()), m.CustomPhysicalProperties)
m.Shape = Enum.PartType.Ball
log("ball", n(m:GetMass()))
local w = Instance.new("WedgePart")
w.Size = Vector3.new(2, 2, 2)
local c = Instance.new("CornerWedgePart")
c.Size = Vector3.new(2, 2, 2)
log("wedges", n(w:GetMass()), n(c:GetMass()))

-- Collision groups.
local function groups()
    local t = {}
    for _, g in ipairs(PS:GetRegisteredCollisionGroups()) do
        t[#t + 1] = g.name .. "/" .. g.mask
    end
    return table.concat(t, " ")
end
log("groups", groups(), PS:GetMaxCollisionGroups())
PS:RegisterCollisionGroup("RookA")
PS:RegisterCollisionGroup("RookB")
PS:CollisionGroupSetCollidable("RookA", "RookB", false)
log("set", groups())
log("collidable", PS:CollisionGroupsAreCollidable("RookA", "RookB"), PS:CollisionGroupsAreCollidable("RookA", "RookA"), PS:CollisionGroupsAreCollidable("RookA", "Nope"))
try("unknown", function() PS:CollisionGroupSetCollidable("RookA", "Nope", true) end)
try("default", function() PS:UnregisterCollisionGroup("Default") end)
try("empty", function() PS:RegisterCollisionGroup("") end)
try("dup create", function() return PS:CreateCollisionGroup("RookA") end)
try("missing id", function() return PS:GetCollisionGroupId("Nope") end)
log("id", PS:GetCollisionGroupId("RookB"), "name", PS:GetCollisionGroupName(PS:GetCollisionGroupId("RookB")), "none", "[" .. PS:GetCollisionGroupName(31) .. "]")
local ga, gb = part("GA", 0, 0, true), part("GB", 0, 0, true)
ga.CollisionGroup = "RookA"
PS:SetPartCollisionGroup(gb, "RookB")
try("outside world", function() return ga:CanCollideWith(gb) end)
for _, p in ipairs({ ga, gb, m }) do
    p.Parent = workspace
end
log("parts", ga.CollisionGroup, gb.CollisionGroup, PS:CollisionGroupContainsPart("RookB", gb), ga:CanCollideWith(gb), ga:CanCollideWith(m))
PS:RenameCollisionGroup("RookB", "RookC")
PS:UnregisterCollisionGroup("RookC")
log("after", groups(), gb.CollisionGroup)
PS:UnregisterCollisionGroup("RookA")

-- Touches: a part dropped on an anchored floor, a non-colliding part
-- falling through it, and a part with CanTouch off.
local events = {}
local function watch(p)
    p.Touched:Connect(function(hit)
        table.insert(events, p.Name .. " touched " .. hit.Name)
    end)
    p.TouchEnded:Connect(function(hit)
        table.insert(events, p.Name .. " ended " .. hit.Name)
    end)
end
local floor = part("Floor", 100, 0, true, Vector3.new(30, 1, 30))
local drop = part("Drop", 100, 5, false)
local ghost = part("Ghost", 108, 5, false)
ghost.CanCollide = false
local shy = part("Shy", 92, 5, false)
shy.CanTouch = false
for _, p in ipairs({ floor, drop, ghost, shy }) do
    watch(p)
    p.Parent = workspace
end
task.wait(1.5)
-- How often the dropped part bounces is the solver's business; log the
-- first touch and the others' events.
local seen = {}
for _, e in ipairs(events) do
    if not e:find("Drop") then
        seen[e] = true
    end
end
local keys = {}
for k in pairs(seen) do
    keys[#keys + 1] = k
end
table.sort(keys)
log("first", events[1], events[2])
log("seen", table.concat(keys, ", "))
log("ghost fell", ghost.Position.Y < 990, "drop rests", math.abs(drop.Position.Y - 1001.5) < 0.2)

for _, p in ipairs(made) do
    p:Destroy()
end
