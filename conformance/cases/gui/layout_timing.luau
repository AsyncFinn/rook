-- When layout happens: reading an Absolute property lays out at once;
-- otherwise the frame's layout does, after RenderStepped. Changed fires
-- only for values that moved.
--! client
local RunService = game:GetService("RunService")
local player = game:GetService("Players").LocalPlayer
local screen = Instance.new("ScreenGui")
screen.Parent = player:WaitForChild("PlayerGui")
local f = Instance.new("Frame")
f.Size = UDim2.fromOffset(10, 10)
f.Parent = screen
local c = Instance.new("Frame")
c.Size = UDim2.fromScale(0.5, 0.5)
c.Parent = f
RunService.RenderStepped:Wait()
RunService.Heartbeat:Wait()

local events = {}
for _, o in { f, c } do
	o:GetPropertyChangedSignal("AbsoluteSize"):Connect(function()
		table.insert(events, (if o == f then "F " else "C ") .. tostring(o.AbsoluteSize))
	end)
end

f.Size = UDim2.fromOffset(100, 50)
log("before a frame", #events)
RunService.RenderStepped:Wait()
RunService.Heartbeat:Wait()
log("after a frame", table.concat(events, ", "))

table.clear(events)
f.Size = UDim2.fromOffset(40, 30)
f.Size = UDim2.fromOffset(100, 50)
RunService.RenderStepped:Wait()
RunService.Heartbeat:Wait()
log("set and set back", #events)

f.Size = UDim2.fromOffset(80, 80)
log("read", c.AbsoluteSize)
task.wait()
log("after the read", table.concat(events, ", "))

screen:Destroy()
