-- UITableLayout: where it puts two rows of cells and what it reports, in
-- a fixed 400 x 300 box.
--! client
local player = game:GetService("Players").LocalPlayer
local screen = Instance.new("ScreenGui")
screen.Parent = player:WaitForChild("PlayerGui")

local function v(p)
	return string.format("%g,%g", math.round(p.X * 100) / 100 + 0, math.round(p.Y * 100) / 100 + 0)
end
-- A new table each time: Studio uses stale sizes when a live table's
-- MajorAxis changes.
local function table_of(tag, setup)
	local box = Instance.new("Frame")
	box.Size = UDim2.fromOffset(400, 300)
	box.Parent = screen
	local layout = Instance.new("UITableLayout")
	layout.SortOrder = Enum.SortOrder.LayoutOrder
	setup(layout)
	layout.Parent = box
	local all = {}
	for r, row in { { { 50, 20 }, { 80, 30 }, { 20, 10 } }, { { 70, 25 }, { 30, 40 } } } do
		local rf = Instance.new("Frame")
		rf.Name = "R" .. r
		rf.LayoutOrder = r
		rf.Size = UDim2.fromOffset(10, 5)
		rf.Parent = box
		table.insert(all, rf)
		for c, size in row do
			local cf = Instance.new("Frame")
			cf.Name = "C" .. r .. c
			cf.LayoutOrder = c
			cf.Size = if r == 1 and c == 1 and tag == "scaled cell"
				then UDim2.fromScale(0.5, 0.5)
				else UDim2.fromOffset(size[1], size[2])
			cf.Parent = rf
			table.insert(all, cf)
		end
	end
	local out = {}
	for _, f in all do
		table.insert(out, f.Name .. "@" .. v(f.AbsolutePosition - box.AbsolutePosition) .. "/" .. v(f.AbsoluteSize))
	end
	log(tag, table.concat(out, " "), "content", v(layout.AbsoluteContentSize))
	box:Destroy()
end

table_of("rows", function() end)
table_of("padded", function(t)
	t.Padding = UDim2.fromOffset(5, 7)
end)
table_of("fill columns", function(t)
	t.FillEmptySpaceColumns = true
end)
table_of("fill rows", function(t)
	t.FillEmptySpaceRows = true
end)
table_of("columns", function(t)
	t.MajorAxis = Enum.TableMajorAxis.ColumnMajor
end)
table_of("horizontal", function(t)
	t.FillDirection = Enum.FillDirection.Horizontal
end)
table_of("centered, bottom", function(t)
	t.HorizontalAlignment = Enum.HorizontalAlignment.Center
	t.VerticalAlignment = Enum.VerticalAlignment.Bottom
end)
table_of("scaled cell", function() end)

screen:Destroy()
