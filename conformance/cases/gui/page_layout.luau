-- UIPageLayout: where its pages are, CurrentPage, the jumps, and Stopped
-- after a turn; positions are read once each turn has settled.
--! client
local player = game:GetService("Players").LocalPlayer
local screen = Instance.new("ScreenGui")
screen.Parent = player:WaitForChild("PlayerGui")

local function v(p)
	return string.format("%g,%g", math.round(p.X * 100) / 100 + 0, math.round(p.Y * 100) / 100 + 0)
end
local box = Instance.new("Frame")
box.Size = UDim2.fromOffset(400, 300)
box.Parent = screen
local layout = Instance.new("UIPageLayout")
layout.SortOrder = Enum.SortOrder.LayoutOrder
layout.Animated = false
layout.Parent = box
local pages = {}
for i, s in { { 0.5, 0.5 }, { 1, 1 }, { 0.25, 0.5 }, { 0.5, 0.25 } } do
	local f = Instance.new("Frame")
	f.Name = "P" .. i
	f.LayoutOrder = i
	f.Size = UDim2.fromScale(s[1], s[2])
	f.Parent = box
	pages[i] = f
end
local stopped = {}
layout.Stopped:Connect(function(page)
	table.insert(stopped, page.Name)
end)
local function dump(tag)
	local out = {}
	for _, f in pages do
		table.insert(out, f.Name .. "@" .. v(f.AbsolutePosition - box.AbsolutePosition) .. "/" .. v(f.AbsoluteSize))
	end
	log(tag, tostring(layout.CurrentPage), table.concat(out, " "), "content", v(layout.AbsoluteContentSize))
end

dump("start")
layout:Next()
dump("next")
layout:JumpToIndex(3)
layout:Next()
dump("next at the end")
layout:Previous()
dump("previous")
layout:JumpToIndex(10)
dump("index out of range")
layout:JumpTo(pages[1])
dump("jump to P1")
layout:Previous()
dump("previous at the start")
layout.Circular = true
layout:Previous()
dump("circular previous")
layout.Circular = false
layout.HorizontalAlignment = Enum.HorizontalAlignment.Center
layout.VerticalAlignment = Enum.VerticalAlignment.Center
layout.Padding = UDim.new(0.1, 0)
dump("centered, padded")
layout.FillDirection = Enum.FillDirection.Vertical
dump("vertical")
pages[3].Visible = false
layout:JumpToIndex(2)
dump("P3 hidden, index 2")
pages[3].Visible = true
layout.FillDirection = Enum.FillDirection.Horizontal
layout.Padding = UDim.new(0, 0)

layout.Animated = true
layout.TweenTime = 0.3
layout:JumpToIndex(1)
log("current at once", layout.CurrentPage)
task.wait(1)
dump("after a turn")
log("stopped", table.concat(stopped, ","))

screen:Destroy()
