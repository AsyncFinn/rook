-- UIListLayout, UIGridLayout and UIPadding: where they put children and
-- what they report, in a fixed 400 x 300 box.
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
box.Position = UDim2.fromOffset(10, 20)
box.Parent = screen
local function dump(tag, list)
	local out = {}
	for _, f in list do
		table.insert(out, f.Name .. "@" .. v(f.AbsolutePosition - box.AbsolutePosition) .. "/" .. v(f.AbsoluteSize))
	end
	log(tag, table.concat(out, " "))
end
local function frame(name, size, order)
	local f = Instance.new("Frame")
	f.Name = name
	f.Size = size
	f.LayoutOrder = order or 0
	f.Parent = box
	return f
end

local list = Instance.new("UIListLayout")
list.SortOrder = Enum.SortOrder.LayoutOrder
list.Parent = box
local c = frame("C", UDim2.fromOffset(50, 40), 2)
local a = frame("A", UDim2.fromOffset(50, 40), 1)
a.Position = UDim2.fromOffset(99, 99)
a.AnchorPoint = Vector2.new(1, 1)
local b = frame("B", UDim2.fromScale(0.5, 0.1), 1)
local items = { a, b, c }
list.Padding = UDim.new(0.1, 5)
dump("vertical", items)
log("content", v(list.AbsoluteContentSize))
list.HorizontalAlignment = Enum.HorizontalAlignment.Center
list.VerticalAlignment = Enum.VerticalAlignment.Center
dump("centered", items)
list.HorizontalAlignment = Enum.HorizontalAlignment.Right
list.VerticalAlignment = Enum.VerticalAlignment.Bottom
list.FillDirection = Enum.FillDirection.Horizontal
dump("horizontal, right, bottom", items)
log("content", v(list.AbsoluteContentSize))
list.HorizontalAlignment = Enum.HorizontalAlignment.Left
list.VerticalAlignment = Enum.VerticalAlignment.Top
c.Visible = false
dump("C hidden", items)
log("content", v(list.AbsoluteContentSize))
c.Visible = true

local z = frame("Z", UDim2.fromOffset(10, 10), 3)
local y = frame("Y", UDim2.fromOffset(10, 10), 3)
list.SortOrder = Enum.SortOrder.Name
local lower = frame("a", UDim2.fromOffset(10, 10))
dump("by name", { a, b, c, y, z, lower })
z:Destroy()
y:Destroy()
lower:Destroy()
list.SortOrder = Enum.SortOrder.LayoutOrder

b.Size = UDim2.fromOffset(500, 10)
list.HorizontalAlignment = Enum.HorizontalAlignment.Right
local padding = Instance.new("UIPadding")
padding.PaddingLeft = UDim.new(0, 7)
padding.PaddingTop = UDim.new(0.1, 0)
padding.PaddingRight = UDim.new(0, 13)
padding.PaddingBottom = UDim.new(0, 3)
padding.Parent = box
dump("padded", items)
log("content", v(list.AbsoluteContentSize))
list:Destroy()
b.Size = UDim2.fromScale(0.5, 0.5)
b.Position = UDim2.fromScale(0.5, 0)
dump("padded, no list", { b })
padding:Destroy()
for _, f in items do
	f:Destroy()
end

local grid = Instance.new("UIGridLayout")
grid.SortOrder = Enum.SortOrder.Name
grid.Parent = box
local cells = {}
for i = 1, 5 do
	cells[i] = frame("K" .. i, UDim2.fromOffset(50, 40), 6 - i)
end
dump("grid", cells)
log("cells", v(grid.AbsoluteCellSize), v(grid.AbsoluteCellCount), v(grid.AbsoluteContentSize))
grid.CellSize = UDim2.new(0.3, 0, 0, 50)
grid.CellPadding = UDim2.new(0, 10, 0.1, 0)
grid.FillDirectionMaxCells = 2
grid.StartCorner = Enum.StartCorner.BottomRight
dump("two a row, from bottom right", cells)
log("cells", v(grid.AbsoluteCellSize), v(grid.AbsoluteCellCount), v(grid.AbsoluteContentSize))
grid.StartCorner = Enum.StartCorner.TopLeft
grid.FillDirection = Enum.FillDirection.Vertical
grid.FillDirectionMaxCells = 0
grid.HorizontalAlignment = Enum.HorizontalAlignment.Center
grid.VerticalAlignment = Enum.VerticalAlignment.Center
dump("vertical, centered", cells)
log("cells", v(grid.AbsoluteCellCount), v(grid.AbsoluteContentSize))
local second = Instance.new("UIListLayout")
second.Parent = box
dump("a list after the grid", cells)

screen:Destroy()
