-- What takes a click where GUI overlaps: a button under an inactive
-- Frame, an Active Frame, a fully transparent Active Frame and a
-- TextLabel; a non-interactable button over another; plain and Active
-- labels; the clipped-away part of a button; a turned button inside and
-- outside its turned shape; a hidden ancestor; a disabled ScreenGui; and
-- a ClickDetector behind an inactive and an Active Frame. Each spot is
-- 100 x 50; a wall fills the view behind.
--! client input 100,225 down up 250,225 down up 400,225 down up 550,225 down up 700,225 down up 100,325 down up 250,325 down up 400,325 down up 175,425 down up 395,405 down up 380,455 down up 550,425 down up 700,425 down up 100,515 down up 250,515 down up 400,515 down up 760,500
local UserInputService = game:GetService("UserInputService")
local player = game:GetService("Players").LocalPlayer
local camera = workspace.CurrentCamera
camera.CameraType = Enum.CameraType.Scriptable
local view = CFrame.lookAt(Vector3.new(0, 500, 30), Vector3.new(0, 500, 0))
camera.CFrame = view

local connections = {}
local function on(signal, f)
	table.insert(connections, signal:Connect(f))
end
on(game:GetService("RunService").RenderStepped, function()
	camera.CFrame = view
end)

local wall = Instance.new("Part")
wall.Anchored = true
wall.Size = Vector3.new(400, 400, 1)
wall.Position = Vector3.new(0, 500, 0)
local detector = Instance.new("ClickDetector")
detector.MaxActivationDistance = 1000
detector.Parent = wall
on(detector.MouseClick, function()
	log("detector click")
end)
wall.Parent = workspace

local screen = Instance.new("ScreenGui")
screen.ResetOnSpawn = false
local off = Instance.new("ScreenGui")
off.ResetOnSpawn = false
off.Enabled = false
local function make(class, name, x, y, parent, props)
	local o = Instance.new(class)
	o.Name = name
	o.Position = UDim2.fromOffset(x, y)
	o.Size = UDim2.fromOffset(100, 50)
	for k, value in props or {} do
		o[k] = value
	end
	o.Parent = parent
	return o
end
make("TextButton", "underPassive", 50, 200, screen)
make("Frame", "passive", 50, 200, screen, { ZIndex = 2 })
make("TextButton", "underActive", 200, 200, screen)
make("Frame", "active", 200, 200, screen, { ZIndex = 2, Active = true })
make("TextButton", "underClear", 350, 200, screen)
make("Frame", "clear", 350, 200, screen, { ZIndex = 2, Active = true, BackgroundTransparency = 1 })
make("TextButton", "underLabel", 500, 200, screen)
make("TextLabel", "label", 500, 200, screen, { ZIndex = 2 })
make("TextButton", "underOff", 650, 200, screen)
make("ImageButton", "offTop", 650, 200, screen, { ZIndex = 2, Interactable = false })
make("TextLabel", "plainLabel", 50, 300, screen)
make("TextLabel", "activeLabel", 200, 300, screen, { Active = true })
make("TextButton", "inactiveButton", 350, 300, screen, { Active = false })
local clip = make("Frame", "clip", 50, 400, screen, { ClipsDescendants = true })
make("TextButton", "clipped", 50, 0, clip)
make("TextButton", "turned", 300, 400, screen, { Rotation = 45 })
make("TextButton", "inOff", 500, 400, off)
local hidden = make("Frame", "hidden", 650, 400, screen, { Visible = false })
make("TextButton", "inHidden", 0, 0, hidden)
make("Frame", "passiveOnWall", 50, 490, screen)
make("Frame", "activeOnWall", 200, 490, screen, { Active = true })
make("TextLabel", "labelOnWall", 350, 490, screen)

for _, o in screen:GetDescendants() do
	if o:IsA("GuiButton") then
		on(o.MouseButton1Click, function()
			log(o.Name, "click")
		end)
	end
	on(o.InputBegan, function(input)
		if input.UserInputType == Enum.UserInputType.MouseButton1 then
			log(o.Name, "InputBegan")
		end
	end)
end
for _, o in off:GetDescendants() do
	on(o.MouseButton1Click, function()
		log(o.Name, "click")
	end)
end
local presses = 0
on(UserInputService.InputBegan, function(input, processed)
	if input.UserInputType == Enum.UserInputType.MouseButton1 then
		presses += 1
		log("press", presses, "processed", processed)
	end
end)
screen.Parent = player:WaitForChild("PlayerGui")
off.Parent = player.PlayerGui
log("at 100,225", #player.PlayerGui:GetGuiObjectsAtPosition(100, 225))

local start = os.clock()
while presses < 16 and os.clock() - start < 30 do
	task.wait()
end
task.wait(0.6)
for _, c in connections do
	c:Disconnect()
end
screen:Destroy()
off:Destroy()
wall:Destroy()
camera.CameraType = Enum.CameraType.Custom
