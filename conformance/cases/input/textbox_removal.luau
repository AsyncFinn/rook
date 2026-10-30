-- A focused TextBox loses focus when it or an ancestor moves (destroyed,
-- unparented, or reparented anywhere, even within its ScreenGui) and
-- keeps it when hidden: Visible off on it or an ancestor, or its
-- ScreenGui disabled. What fires, in order, and what GetFocusedTextBox
-- says right after the change and a frame later.
--! client
local UserInputService = game:GetService("UserInputService")
local player = game:GetService("Players").LocalPlayer
local playerGui = player:WaitForChild("PlayerGui")

local function name(x)
	return if x then x.Name else "nil"
end
local released = UserInputService.TextBoxFocusReleased:Connect(function(b)
	log("  UIS TextBoxFocusReleased", name(b))
end)

local cases = {
	{ "destroy", function(g) g.box:Destroy() end },
	{ "parent nil", function(g) g.box.Parent = nil end },
	{ "to a sibling frame", function(g) g.box.Parent = g.sibling end },
	{ "frame destroyed", function(g) g.frame:Destroy() end },
	{ "screen to ReplicatedStorage", function(g) g.screen.Parent = game:GetService("ReplicatedStorage") end },
	{ "same parent", function(g) g.box.Parent = g.frame end },
	{ "box hidden", function(g) g.box.Visible = false end },
	{ "frame hidden", function(g) g.frame.Visible = false end },
	{ "screen disabled", function(g) g.screen.Enabled = false end },
}
for _, case in cases do
	local screen = Instance.new("ScreenGui")
	screen.Name = "S"
	screen.ResetOnSpawn = false
	local frame = Instance.new("Frame")
	frame.Name = "F"
	frame.Size = UDim2.fromOffset(300, 300)
	frame.Parent = screen
	local sibling = Instance.new("Frame")
	sibling.Name = "G"
	sibling.Parent = screen
	local box = Instance.new("TextBox")
	box.Name = "Box"
	box.Size = UDim2.fromOffset(200, 40)
	box.ClearTextOnFocus = false
	box.Text = "hi"
	box.Parent = frame
	screen.Parent = playerGui
	box:CaptureFocus()
	task.wait()
	local connections = {
		box.AncestryChanged:Connect(function()
			log("  AncestryChanged")
		end),
		box:GetPropertyChangedSignal("CursorPosition"):Connect(function()
			log("  CursorPosition", box.CursorPosition)
		end),
		box.FocusLost:Connect(function(enter, input)
			log("  FocusLost", enter, input, name(UserInputService:GetFocusedTextBox()))
		end),
	}
	log(case[1])
	case[2]({ box = box, frame = frame, sibling = sibling, screen = screen })
	log("  now", name(UserInputService:GetFocusedTextBox()), box:IsFocused(), box.CursorPosition)
	task.wait()
	log("  a frame later", name(UserInputService:GetFocusedTextBox()))
	for _, c in connections do
		c:Disconnect()
	end
	box:ReleaseFocus()
	screen:Destroy()
	task.wait()
end
released:Disconnect()
