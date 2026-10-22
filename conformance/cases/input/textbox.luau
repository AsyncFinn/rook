-- TextBox focus and typing: focusing by a click (ClearTextOnFocus),
-- typed text, Backspace and Left, Enter ending it; Escape; Tab; a
-- MultiLine box without ClearTextOnFocus taking Enter as a newline and
-- losing focus to a click elsewhere; then CaptureFocus and ReleaseFocus
-- from script. The order of Text and CursorPosition changes, Focused,
-- FocusLost with its arguments, UserInputService's TextBox events and
-- its key events' gameProcessedEvent. T at (100, 100) is 200 x 50; M at
-- (100, 200) is 200 x 100.
--! client input 200,125 down up text:ab key:C key:Backspace key:Left text:x key:Return 200,125 down up text:y key:Tab key:Escape 200,250 down up text:p key:Return text:q 500,400 down up
local UserInputService = game:GetService("UserInputService")
local player = game:GetService("Players").LocalPlayer
local screen = Instance.new("ScreenGui")
screen.ResetOnSpawn = false

local connections = {}
local function on(signal, f)
	table.insert(connections, signal:Connect(f))
end
local lost = 0
local function box(name, y, height, props)
	local b = Instance.new("TextBox")
	b.Name = name
	b.Position = UDim2.fromOffset(100, y)
	b.Size = UDim2.fromOffset(200, height)
	for k, value in props do
		b[k] = value
	end
	on(b.Focused, function()
		log(name, "Focused", string.format("%q", b.Text), b.CursorPosition)
	end)
	on(b.FocusLost, function(enter, input)
		lost += 1
		local cause = if input then tostring(input.KeyCode) .. " " .. tostring(input.UserInputType) else "nil"
		log(name, "FocusLost", enter, cause, string.format("%q", b.Text))
	end)
	on(b:GetPropertyChangedSignal("Text"), function()
		log(name, "Text", string.format("%q", b.Text))
	end)
	on(b:GetPropertyChangedSignal("CursorPosition"), function()
		log(name, "CursorPosition", b.CursorPosition)
	end)
	b.Parent = screen
	return b
end
local t = box("T", 100, 50, { Text = "start" })
local m = box("M", 200, 100, { Text = "", MultiLine = true, ClearTextOnFocus = false })
on(UserInputService.TextBoxFocused, function(b)
	log("UIS TextBoxFocused", b.Name)
end)
on(UserInputService.TextBoxFocusReleased, function(b)
	log("UIS TextBoxFocusReleased", b.Name)
end)
for _, event in { "InputBegan", "InputEnded" } do
	on(UserInputService[event], function(input, processed)
		if input.UserInputType == Enum.UserInputType.Keyboard then
			log("UIS", event, input.KeyCode, processed)
		end
	end)
end
screen.Parent = player:WaitForChild("PlayerGui")

local start = os.clock()
while lost < 3 and os.clock() - start < 30 do
	task.wait()
end
task.wait(0.3)
log("M text", string.format("%q", m.Text), "focused", UserInputService:GetFocusedTextBox())

t.Text = "keep"
t:CaptureFocus()
log("captured", string.format("%q", t.Text), t:IsFocused(), t.CursorPosition, UserInputService:GetFocusedTextBox())
task.wait(0.1)
m:CaptureFocus()
log("M captured", m:IsFocused(), t:IsFocused(), m.CursorPosition)
task.wait(0.1)
m:ReleaseFocus(true)
task.wait(0.1)
t.ClearTextOnFocus = false
t.Text = "abc"
t:CaptureFocus()
log("no clear", t.CursorPosition, t.SelectionStart)
task.wait(0.1)
t:ReleaseFocus()
task.wait(0.1)
for _, c in connections do
	c:Disconnect()
end
screen:Destroy()
