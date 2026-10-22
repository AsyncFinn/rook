-- GUI buttons under the mouse: the order of MouseButton1Down, Up, Click,
-- Activated (with its click count), InputBegan and InputEnded, GuiState
-- and UserInputService's events with gameProcessedEvent; pressing on one
-- button and releasing on another; button 2. A at (100, 100), B at
-- (300, 100), each 100 x 50.
--! client input 150,125 down up down 350,125 up 150,125 down2 up2 down up 600,400
local UserInputService = game:GetService("UserInputService")
local player = game:GetService("Players").LocalPlayer
local screen = Instance.new("ScreenGui")
screen.ResetOnSpawn = false

local releases = 0
local connections = {}
local function on(signal, f)
	table.insert(connections, signal:Connect(f))
end
local function button(name, x)
	local b = Instance.new("TextButton")
	b.Name = name
	b.Position = UDim2.fromOffset(x, 100)
	b.Size = UDim2.fromOffset(100, 50)
	for _, event in { "MouseButton1Down", "MouseButton1Up", "MouseButton2Down", "MouseButton2Up" } do
		on(b[event], function(px, py)
			log(name, event, px, py)
		end)
	end
	for _, event in { "MouseButton1Click", "MouseButton2Click", "MouseEnter", "MouseLeave" } do
		on(b[event], function()
			log(name, event)
		end)
	end
	on(b.Activated, function(input, count)
		log(name, "Activated", input.UserInputType, count)
	end)
	for _, event in { "InputBegan", "InputEnded" } do
		on(b[event], function(input)
			log(name, event, input.UserInputType, input.UserInputState)
		end)
	end
	on(b:GetPropertyChangedSignal("GuiState"), function()
		log(name, "GuiState", b.GuiState)
	end)
	b.Parent = screen
	return b
end
button("A", 100)
button("B", 300)
local mouse = { [Enum.UserInputType.MouseButton1] = true, [Enum.UserInputType.MouseButton2] = true }
on(UserInputService.InputBegan, function(input, processed)
	if mouse[input.UserInputType] then
		log("UIS InputBegan", input.UserInputType, processed, input.Position)
	end
end)
on(UserInputService.InputEnded, function(input, processed)
	if mouse[input.UserInputType] then
		log("UIS InputEnded", input.UserInputType, processed, input.Position)
		releases += 1
	end
end)
screen.Parent = player:WaitForChild("PlayerGui")

local start = os.clock()
while releases < 4 and os.clock() - start < 20 do
	task.wait()
end
task.wait(0.6)
for _, c in connections do
	c:Disconnect()
end
screen:Destroy()
