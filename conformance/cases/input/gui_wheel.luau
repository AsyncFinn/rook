-- The wheel on ScrollingFrames: how far a notch scrolls, which of two
-- nested frames takes it, a frame whose canvas is only wider, one with
-- ScrollingEnabled off, gameProcessedEvent; and dragging a bar's thumb.
-- S at (100, 100) is 200 x 200 with a 1000-high canvas; Inner (100 x 100,
-- a 150-high canvas) sits at (20, 20) on it; Wide at (400, 100) and Off
-- at (400, 250) are 200 x 100.
--! client input 150,150 wheeldown wait wait wheeldown wait wait wheelup wait wait wheelup wait wait 500,150 wheeldown wait wait 500,275 wheeldown wait wait 294,105 down 294,155 up 600,450
local UserInputService = game:GetService("UserInputService")
local player = game:GetService("Players").LocalPlayer
local screen = Instance.new("ScreenGui")
screen.ResetOnSpawn = false

local function scroller(name, parent, x, y, size, canvas)
	local f = Instance.new("ScrollingFrame")
	f.Name = name
	f.Position = UDim2.fromOffset(x, y)
	f.Size = size
	f.CanvasSize = canvas
	f.Parent = parent
	return f
end
local s = scroller("S", screen, 100, 100, UDim2.fromOffset(200, 200), UDim2.fromOffset(0, 1000))
local inner = scroller("Inner", s, 20, 20, UDim2.fromOffset(100, 100), UDim2.fromOffset(0, 150))
local wide = scroller("Wide", screen, 400, 100, UDim2.fromOffset(200, 100), UDim2.fromOffset(1000, 0))
local off = scroller("Off", screen, 400, 250, UDim2.fromOffset(200, 100), UDim2.fromOffset(0, 1000))
off.ScrollingEnabled = false

local function v(p)
	return string.format("%g,%g", math.round(p.X * 10) / 10 + 0, math.round(p.Y * 10) / 10 + 0)
end
local function positions()
	return v(s.CanvasPosition), v(inner.CanvasPosition), v(wide.CanvasPosition), v(off.CanvasPosition)
end
local wheels, released = 0, false
local connections = {}
table.insert(connections, UserInputService.InputChanged:Connect(function(input, processed)
	if input.UserInputType == Enum.UserInputType.MouseWheel then
		wheels += 1
		local n = wheels
		log("wheel", n, input.Position.Z, "processed", processed)
		task.delay(0.35, function()
			log("after wheel", n, positions())
		end)
	end
end))
table.insert(connections, UserInputService.InputEnded:Connect(function(input, processed)
	if input.UserInputType == Enum.UserInputType.MouseButton1 then
		log("drag released", processed, positions())
		released = true
	end
end))
screen.Parent = player:WaitForChild("PlayerGui")
log("window", v(s.AbsoluteWindowSize), "canvas", v(s.AbsoluteCanvasSize))

local start = os.clock()
while not released and os.clock() - start < 20 do
	task.wait()
end
task.wait(0.3)
log("end", positions())
for _, c in connections do
	c:Disconnect()
end
screen:Destroy()
