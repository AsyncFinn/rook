-- UIListLayout's flex and wrapping and UIFlexItem: where they put three
-- children and what the list reports, in a fixed 400 x 300 box.
--! client
local player = game:GetService("Players").LocalPlayer
local screen = Instance.new("ScreenGui")
screen.Parent = player:WaitForChild("PlayerGui")

-- Rounded, and + 0 turns -0 (float noise) into 0.
local function v(p)
	return string.format("%g,%g", math.round(p.X * 100) / 100 + 0, math.round(p.Y * 100) / 100 + 0)
end
local box = Instance.new("Frame")
box.Size = UDim2.fromOffset(400, 300)
box.Parent = screen
local list = Instance.new("UIListLayout")
list.FillDirection = Enum.FillDirection.Horizontal
list.SortOrder = Enum.SortOrder.LayoutOrder
list.Parent = box
local items = {}
for i, w in { 50, 100, 30 } do
	local f = Instance.new("Frame")
	f.Name = "F" .. i
	f.LayoutOrder = i
	f.Size = UDim2.fromOffset(w, 20 + i * 10)
	f.Parent = box
	items[i] = f
end
local function dump(tag)
	local out = {}
	for _, f in items do
		table.insert(out, f.Name .. "@" .. v(f.AbsolutePosition - box.AbsolutePosition) .. "/" .. v(f.AbsoluteSize))
	end
	log(tag, table.concat(out, " "), "content", v(list.AbsoluteContentSize))
end
local function sizes(a, b, c)
	items[1].Size = UDim2.fromOffset(a, 30)
	items[2].Size = UDim2.fromOffset(b, 40)
	items[3].Size = UDim2.fromOffset(c, 50)
end

for _, name in { "Fill", "SpaceAround", "SpaceBetween", "SpaceEvenly" } do
	list.HorizontalFlex = Enum.UIFlexAlignment[name]
	dump("along " .. name)
end
list.HorizontalFlex = Enum.UIFlexAlignment.None
for _, name in { "Fill", "SpaceAround", "SpaceBetween" } do
	list.VerticalFlex = Enum.UIFlexAlignment[name]
	dump("across " .. name)
end
list.ItemLineAlignment = Enum.ItemLineAlignment.Start
dump("across Fill, line Start")
list.VerticalFlex = Enum.UIFlexAlignment.None
list.Padding = UDim.new(0, 10)
list.HorizontalFlex = Enum.UIFlexAlignment.Fill
dump("Fill, padded")
list.HorizontalFlex = Enum.UIFlexAlignment.None
list.Padding = UDim.new(0, 0)
for _, name in { "Center", "End", "Stretch" } do
	list.ItemLineAlignment = Enum.ItemLineAlignment[name]
	dump("line " .. name)
end
list.ItemLineAlignment = Enum.ItemLineAlignment.Center
list.VerticalAlignment = Enum.VerticalAlignment.Bottom
dump("line Center, bottom")
list.ItemLineAlignment = Enum.ItemLineAlignment.Automatic
list.VerticalAlignment = Enum.VerticalAlignment.Top

local item = Instance.new("UIFlexItem")
item.Parent = items[2]
for _, name in { "Grow", "Shrink", "Fill" } do
	item.FlexMode = Enum.UIFlexMode[name]
	dump("F2 " .. name)
end
item.ItemLineAlignment = Enum.ItemLineAlignment.End
dump("F2 Fill, line End")
item.ItemLineAlignment = Enum.ItemLineAlignment.Automatic
item.FlexMode = Enum.UIFlexMode.Custom
item.GrowRatio = 2
local other = Instance.new("UIFlexItem")
other.FlexMode = Enum.UIFlexMode.Custom
other.GrowRatio = 1
other.Parent = items[1]
dump("grow 1 and 2")
list.HorizontalFlex = Enum.UIFlexAlignment.Fill
dump("grow 1 and 2, list Fill")
other.FlexMode = Enum.UIFlexMode.None
dump("F1 None, list Fill")
other:Destroy()
item:Destroy()

sizes(200, 300, 30)
dump("too long, list Fill")
local limit = Instance.new("UISizeConstraint")
limit.MinSize = Vector2.new(190, 0)
limit.Parent = items[1]
dump("too long, F1 at least 190")
limit:Destroy()
list.HorizontalFlex = Enum.UIFlexAlignment.None
item = Instance.new("UIFlexItem")
item.FlexMode = Enum.UIFlexMode.Custom
item.ShrinkRatio = 3
item.Parent = items[2]
other = Instance.new("UIFlexItem")
other.FlexMode = Enum.UIFlexMode.Shrink
other.Parent = items[1]
dump("too long, shrink 1 and 3")
item:Destroy()
other:Destroy()
dump("too long, no flex")

sizes(150, 200, 100)
list.Wraps = true
list.Padding = UDim.new(0, 10)
dump("wraps, padded")
list.Padding = UDim.new(0, 0)
list.HorizontalFlex = Enum.UIFlexAlignment.SpaceBetween
dump("wraps, SpaceBetween")
list.HorizontalFlex = Enum.UIFlexAlignment.Fill
dump("wraps, Fill")
list.HorizontalFlex = Enum.UIFlexAlignment.None
for _, name in { "Fill", "SpaceBetween", "SpaceEvenly" } do
	list.VerticalFlex = Enum.UIFlexAlignment[name]
	dump("wraps, across " .. name)
end
list.VerticalFlex = Enum.UIFlexAlignment.None
list.VerticalAlignment = Enum.VerticalAlignment.Bottom
list.HorizontalAlignment = Enum.HorizontalAlignment.Center
dump("wraps, bottom, centered")
list.FillDirection = Enum.FillDirection.Vertical
list.VerticalAlignment = Enum.VerticalAlignment.Top
list.HorizontalAlignment = Enum.HorizontalAlignment.Left
list.Wraps = false
list.VerticalFlex = Enum.UIFlexAlignment.SpaceBetween
list.HorizontalFlex = Enum.UIFlexAlignment.Fill
dump("vertical, SpaceBetween, across Fill")

screen:Destroy()
