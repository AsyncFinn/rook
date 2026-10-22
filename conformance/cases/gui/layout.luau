-- Where GUI objects go: UDim2 against the parent, AnchorPoint,
-- SizeConstraint, Folders, Rotation, UIScale, the size constraints and
-- AutomaticSize. Positions are logged from a fixed 400 x 300 box, so the
-- window's size doesn't matter.
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
local function rect(name, f)
	log(name, v(f.AbsolutePosition - box.AbsolutePosition), v(f.AbsoluteSize))
end
local function frame(parent, size, position)
	local f = Instance.new("Frame")
	f.Size = size
	f.Position = position or UDim2.new()
	f.Parent = parent
	return f
end

local camera = workspace.CurrentCamera
local top = game:GetService("GuiService"):GetGuiInset()
log("inset", v(top), "screen height", camera.ViewportSize.Y - screen.AbsoluteSize.Y)

local f = frame(box, UDim2.new(0.5, 10, 0.25, -4), UDim2.new(0.1, 3, 0.2, 5))
f.AnchorPoint = Vector2.new(0.5, 1)
rect("anchored", f)
f.Size = UDim2.fromOffset(-100, 50)
rect("negative", f)
f:Destroy()

local folder = Instance.new("Folder")
folder.Parent = box
rect("in folder", frame(folder, UDim2.fromScale(0.5, 0.5), UDim2.fromScale(0.5, 0)))
folder:Destroy()

local yy = frame(box, UDim2.fromScale(0.5, 0.5))
yy.SizeConstraint = Enum.SizeConstraint.RelativeYY
rect("RelativeYY", yy)
yy.SizeConstraint = Enum.SizeConstraint.RelativeXX
yy.Position = UDim2.fromScale(0.5, 0.5)
rect("RelativeXX", yy)
yy:Destroy()

local r = frame(box, UDim2.fromOffset(100, 50), UDim2.fromOffset(100, 100))
r.Rotation = 90
local inside = frame(r, UDim2.fromOffset(10, 10))
inside.Rotation = 15
rect("turned child", inside)
log("rotation", inside.AbsoluteRotation)
r.AnchorPoint = Vector2.new(1, 1)
rect("turned child, anchor 1,1", inside)
r:Destroy()

local c = frame(box, UDim2.fromScale(0.5, 0.5), UDim2.fromScale(0.5, 0.5))
c.AnchorPoint = Vector2.new(0.5, 0.5)
local sc = Instance.new("UISizeConstraint")
sc.MinSize = Vector2.new(10, 160)
sc.MaxSize = Vector2.new(120, 1000)
sc.Parent = c
rect("size constraint", c)
sc.MinSize = Vector2.new(200, 0)
sc.MaxSize = Vector2.new(100, 100)
rect("min above max", c)
sc:Destroy()
local ar = Instance.new("UIAspectRatioConstraint")
ar.AspectRatio = 0.5
ar.Parent = c
rect("aspect fit", c)
ar.AspectType = Enum.AspectType.ScaleWithParentSize
rect("aspect scale width", c)
ar.AspectRatio = 4
ar.DominantAxis = Enum.DominantAxis.Height
rect("aspect scale height", c)
ar:Destroy()

local grandchild = frame(c, UDim2.fromScale(0.5, 0.5), UDim2.fromScale(0.5, 0))
local scale = Instance.new("UIScale")
scale.Scale = 2
scale.Parent = c
rect("scaled", c)
rect("scaled child", grandchild)
scale.Parent = box
log("scaled box", v(box.AbsoluteSize))
rect("child of scaled box", grandchild)
scale:Destroy()
c:Destroy()

local auto = frame(box, UDim2.fromOffset(20, 10), UDim2.fromOffset(5, 5))
auto.AutomaticSize = Enum.AutomaticSize.XY
local a1 = frame(auto, UDim2.fromOffset(100, 30), UDim2.fromOffset(10, 50))
rect("automatic", auto)
auto.AutomaticSize = Enum.AutomaticSize.X
rect("automatic X", auto)
auto.AutomaticSize = Enum.AutomaticSize.XY
a1.AnchorPoint = Vector2.new(1, 0)
rect("automatic, anchored child", auto)
a1.AnchorPoint = Vector2.zero
local padding = Instance.new("UIPadding")
padding.PaddingLeft = UDim.new(0, 4)
padding.PaddingRight = UDim.new(0, 6)
padding.PaddingTop = UDim.new(0, 2)
padding.Parent = auto
rect("automatic, padded", auto)
rect("padded child", a1)
padding:Destroy()

-- An Inset border pads what's inside it; a Middle one by half.
auto.BorderSizePixel = 4
for _, mode in { Enum.BorderMode.Inset, Enum.BorderMode.Middle } do
	auto.BorderMode = mode
	rect(mode.Name .. " border, automatic", auto)
	rect(mode.Name .. " border child", a1)
end

screen:Destroy()
