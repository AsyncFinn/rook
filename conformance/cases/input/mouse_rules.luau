-- What Mouse.Target and Mouse.Hit see: a large part in front of the camera
-- with a wall behind it, changed between looks, then how far the ray reaches
-- (a target 20 studs through: its near face is 10 before the distance).
--! client input 300,300 wait wait wait wait wait wait wait wait
local Players = game:GetService("Players")
local RunService = game:GetService("RunService")

local mouse = Players.LocalPlayer:GetMouse()
local camera = workspace.CurrentCamera
local ORIGIN = Vector3.new(0, 5000, 0)
camera.CameraType = Enum.CameraType.Scriptable

local folder = Instance.new("Folder")
folder.Parent = workspace
local function wall(name, z, parent)
    local p = Instance.new("Part")
    p.Name = name
    p.Anchored = true
    p.Size = Vector3.new(2000, 2000, 1)
    p.Position = ORIGIN - Vector3.new(0, 0, z)
    p.Parent = parent
    return p
end

-- The camera comes back every look: the default scripts may move it.
local function look()
    for _ = 1, 3 do
        camera.CameraType = Enum.CameraType.Scriptable
        camera.CFrame = CFrame.new(ORIGIN)
        task.wait(0.05)
    end
    RunService.RenderStepped:Wait()
    RunService.RenderStepped:Wait()
    local target = mouse.Target
    -- Along the camera's axis: the mouse's slant depends on the window.
    local depth = (ORIGIN - mouse.Hit.Position).Z
    return target and target.Name or "nothing", string.format("%.1f", depth)
end

local back = wall("back", 60, workspace)
local front = wall("front", 30, folder)
log("plain", look())

local function with(name, change)
    for k, v in pairs(change) do
        front[k] = v
    end
    log(name, look())
    front.CanCollide, front.CanTouch, front.Transparency = true, true, 0
    front.CanQuery = true
end
with("CanCollide off", { CanCollide = false })
with("CanCollide and CanQuery off", { CanCollide = false, CanQuery = false })
with("CanTouch off", { CanTouch = false })
with("see-through", { Transparency = 1 })
with("nearly see-through", { Transparency = 0.99 })
with("see-through, not colliding", { Transparency = 1, CanCollide = false })

mouse.TargetFilter = front
log("filtering the part", look())
mouse.TargetFilter = folder
log("filtering its folder", look())
mouse.TargetFilter = nil

front.Parent = nil
log("only the wall", look())
back.Parent = nil
do
    local target = Instance.new("Part")
    target.Name = "target"
    target.Anchored = true
    target.Size = Vector3.new(20, 20, 20)
    target.Parent = workspace
    for _, distance in ipairs({ 900, 2000, 2057, 2060, 3000 }) do
        local aim
        for _ = 1, 3 do
            camera.CFrame = CFrame.new(ORIGIN)
            task.wait(0.05)
        end
        aim = mouse.UnitRay
        target.CFrame = CFrame.lookAt(aim.Origin + aim.Direction * distance, aim.Origin)
        local name = look()
        log("along the ray", distance, name)
    end
    target:Destroy()
end
do
    camera.CFrame = CFrame.new(ORIGIN)
    task.wait(0.1)
    log("nothing there", string.format("%.0f", (mouse.Hit.Position - mouse.UnitRay.Origin).Magnitude))
    log("origin", (mouse.Origin.Position - camera.CFrame.Position).Magnitude < 1)
end

back:Destroy()
front:Destroy()
folder:Destroy()
camera.CameraType = Enum.CameraType.Custom
