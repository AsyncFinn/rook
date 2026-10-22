-- How far a legacy Style moves children in: each FrameStyle and
-- ButtonStyle at a large and two small sizes, with UIPadding and an Inset
-- border, and text. Style changes none of the properties scripts read.
--! client
local player = game:GetService("Players").LocalPlayer
local screen = Instance.new("ScreenGui")
screen.Parent = player:WaitForChild("PlayerGui")

local function v(p)
	return string.format("%g,%g", math.round(p.X * 1000) / 1000 + 0, math.round(p.Y * 1000) / 1000 + 0)
end
local function inside(f)
	local c = Instance.new("Frame")
	c.Size = UDim2.fromScale(1, 1)
	c.Parent = f
	return c
end

local styled = {}
for _, s in Enum.FrameStyle:GetEnumItems() do
	table.insert(styled, { "Frame", s })
end
for _, s in Enum.ButtonStyle:GetEnumItems() do
	table.insert(styled, { "TextButton", s })
end
for _, e in styled do
	local f = Instance.new(e[1])
	f.Style = e[2]
	f.Parent = screen
	local c = inside(f)
	local row = {}
	for _, size in { Vector2.new(120, 80), Vector2.new(200, 10), Vector2.new(30, 40) } do
		f.Size = UDim2.fromOffset(size.X, size.Y)
		table.insert(row, v(c.AbsolutePosition - f.AbsolutePosition) .. " " .. v(c.AbsoluteSize))
	end
	log(e[2].Name, table.concat(row, " | "))
	f:Destroy()
end

local f = Instance.new("Frame")
f.Style = Enum.FrameStyle.RobloxSquare
f.Size = UDim2.fromOffset(120, 80)
f.BorderMode = Enum.BorderMode.Inset
f.BorderSizePixel = 4
f.Parent = screen
local c = inside(f)
log("inset border", v(c.AbsolutePosition - f.AbsolutePosition), v(c.AbsoluteSize))
local pad = Instance.new("UIPadding")
pad.PaddingLeft = UDim.new(0, 5)
pad.PaddingTop = UDim.new(0.5, 0)
pad.Parent = f
log("padded", v(c.AbsolutePosition - f.AbsolutePosition), v(c.AbsoluteSize))
log("reads", f.BackgroundColor3 == Color3.fromRGB(163, 162, 165), f.BackgroundTransparency, f.BorderSizePixel)
f:Destroy()

local b = Instance.new("TextButton")
b.Style = Enum.ButtonStyle.RobloxButton
b.Size = UDim2.fromOffset(120, 40)
b.Text = "Hi"
b.TextSize = 14
b.Parent = screen
local custom = b:Clone()
custom.Style = Enum.ButtonStyle.Custom
custom.Parent = screen
log("text", b.TextBounds == custom.TextBounds, b.TextFits, b.AutoButtonColor)
b.TextScaled = true
custom.TextScaled = true
log("scaled text height", b.TextBounds.Y, custom.TextBounds.Y)
b:Destroy()
custom:Destroy()

local i = Instance.new("ImageButton")
i.Style = Enum.ButtonStyle.RobloxRoundButton
i.Size = UDim2.fromOffset(100, 20)
i.Parent = screen
local ic = inside(i)
log("image button", v(ic.AbsolutePosition - i.AbsolutePosition), v(ic.AbsoluteSize))
i:Destroy()
screen:Destroy()
