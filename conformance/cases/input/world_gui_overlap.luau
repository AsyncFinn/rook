-- What on-screen GUI lets a click through to a SurfaceGui button behind
-- it: an inactive Frame, an Active Frame, a fully transparent Active
-- Frame, a TextLabel, a non-interactable button, a button with Active
-- off, an Active TextLabel and nothing at all; then, with the SurfaceGui's
-- Active off, nothing and an inactive Frame. A ClickDetector sits on the
-- wall under the SurfaceGui. Each spot is 100 x 50; the SurfaceGui's
-- button fills the view behind.
--! client input 100,225 down up 250,225 down up 400,225 down up 550,225 down up 700,225 down up 100,325 down up 250,325 down up 400,325 down up 400,325 down up 100,225 down up 760,500
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
local surface = Instance.new("SurfaceGui")
surface.Face = Enum.NormalId.Back
surface.SizingMode = Enum.SurfaceGuiSizingMode.FixedSize
surface.CanvasSize = Vector2.new(400, 400)
local worldButton = Instance.new("TextButton")
worldButton.Name = "worldButton"
worldButton.Size = UDim2.fromScale(1, 1)
worldButton.Text = ""
worldButton.Parent = surface
surface.Parent = wall
wall.Parent = workspace

local screen = Instance.new("ScreenGui")
screen.ResetOnSpawn = false
local function make(class, name, x, y, props)
	local o = Instance.new(class)
	o.Name = name
	o.Position = UDim2.fromOffset(x, y)
	o.Size = UDim2.fromOffset(100, 50)
	for k, value in props or {} do
		o[k] = value
	end
	o.Parent = screen
	return o
end
make("Frame", "passive", 50, 200)
make("Frame", "active", 200, 200, { Active = true })
make("Frame", "clear", 350, 200, { Active = true, BackgroundTransparency = 1 })
make("TextLabel", "label", 500, 200)
make("TextButton", "offButton", 650, 200, { Interactable = false })
make("TextButton", "inactiveButton", 50, 300, { Active = false })
make("TextLabel", "activeLabel", 200, 300, { Active = true })

local function watch(o)
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
for _, o in screen:GetDescendants() do
	watch(o)
end
watch(worldButton)
local presses = 0
on(UserInputService.InputBegan, function(input, processed)
	if input.UserInputType == Enum.UserInputType.MouseButton1 then
		presses += 1
		log("press", presses, "processed", processed)
		if presses == 8 then
			surface.Active = false
		end
	end
end)
screen.Parent = player:WaitForChild("PlayerGui")

local start = os.clock()
while presses < 10 and os.clock() - start < 30 do
	task.wait()
end
task.wait(0.6)
for _, c in connections do
	c:Disconnect()
end
screen:Destroy()
wall:Destroy()
camera.CameraType = Enum.CameraType.Custom
