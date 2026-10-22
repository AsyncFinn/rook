-- ScrollingFrame: AbsoluteWindowSize and AbsoluteCanvasSize by canvas
-- size, bar insets, thickness, border and direction; CanvasPosition kept
-- within the canvas when set; children on the scrolled canvas; and
-- AutomaticCanvasSize. Each frame is 200 x 100 in a 400 x 300 box.
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

local function scroller(props)
	local f = Instance.new("ScrollingFrame")
	f.Size = UDim2.fromOffset(200, 100)
	f.BorderSizePixel = 0
	f.CanvasSize = props.CanvasSize or f.CanvasSize
	for k, value in props do
		f[k] = value
	end
	local c = Instance.new("Frame")
	c.Position = UDim2.fromOffset(10, 20)
	c.Size = UDim2.fromScale(0.5, 0.5)
	c.Parent = f
	f.Parent = box
	return f, c
end
local function sizes(name, props)
	local f, c = scroller(props)
	log(name, "window", v(f.AbsoluteWindowSize), "canvas", v(f.AbsoluteCanvasSize), "child", v(c.AbsolutePosition - f.AbsolutePosition), v(c.AbsoluteSize))
	f:Destroy()
end

local both = UDim2.fromOffset(300, 400)
sizes("default", {})
sizes("both", { CanvasSize = both })
sizes("vertical", { CanvasSize = UDim2.fromOffset(100, 400) })
sizes("horizontal", { CanvasSize = UDim2.fromOffset(300, 50) })
sizes("none", { CanvasSize = UDim2.fromOffset(50, 50) })
sizes("pushed", { CanvasSize = UDim2.fromOffset(195, 400) })
sizes("negative", { CanvasSize = UDim2.fromOffset(-100, -100) })
sizes("scaled", { CanvasSize = UDim2.fromScale(1, 2) })
sizes("inset", { CanvasSize = UDim2.fromOffset(100, 400), VerticalScrollBarInset = Enum.ScrollBarInset.ScrollBar })
sizes("always", {
	CanvasSize = UDim2.fromOffset(50, 50),
	VerticalScrollBarInset = Enum.ScrollBarInset.Always,
	HorizontalScrollBarInset = Enum.ScrollBarInset.Always,
})
sizes("thick", { CanvasSize = both, ScrollBarThickness = 30 })
sizes("thin", { CanvasSize = both, ScrollBarThickness = 0 })
sizes("border", { CanvasSize = both, BorderSizePixel = 3 })
sizes("inset border", { CanvasSize = both, BorderSizePixel = 4, BorderMode = Enum.BorderMode.Inset })
sizes("x only", { CanvasSize = both, ScrollingDirection = Enum.ScrollingDirection.X })
sizes("y only", { CanvasSize = both, ScrollingDirection = Enum.ScrollingDirection.Y })
sizes("disabled", { CanvasSize = both, ScrollingEnabled = false })
sizes("left", {
	CanvasSize = UDim2.fromOffset(100, 400),
	VerticalScrollBarPosition = Enum.VerticalScrollBarPosition.Left,
	VerticalScrollBarInset = Enum.ScrollBarInset.ScrollBar,
})

local function scrolled(name, at, props)
	local f, c = scroller(props or { CanvasSize = both })
	f.CanvasPosition = at
	log(name, "position", v(f.CanvasPosition), "child", v(c.AbsolutePosition - f.AbsolutePosition))
	f:Destroy()
end
scrolled("scrolled", Vector2.new(30, 40))
scrolled("past the end", Vector2.new(1000, 1000))
scrolled("before the start", Vector2.new(-50, -50))
scrolled("between pixels", Vector2.new(10.6, 20.25))
scrolled("nothing to scroll", Vector2.new(30, 40), { CanvasSize = UDim2.fromOffset(50, 50) })
scrolled("y only", Vector2.new(30, 40), { CanvasSize = both, ScrollingDirection = Enum.ScrollingDirection.Y })

local function automatic(name, axes, build)
	local f = Instance.new("ScrollingFrame")
	f.Size = UDim2.fromOffset(200, 100)
	f.CanvasSize = UDim2.new()
	f.AutomaticCanvasSize = axes
	build(f)
	f.Parent = box
	log(name, "window", v(f.AbsoluteWindowSize), "canvas", v(f.AbsoluteCanvasSize))
	f:Destroy()
end
local function kid(parent, x, y, w, h)
	local k = Instance.new("Frame")
	k.Position = UDim2.fromOffset(x, y)
	k.Size = UDim2.fromOffset(w, h)
	k.Parent = parent
	return k
end
automatic("auto y", Enum.AutomaticSize.Y, function(f)
	kid(f, 0, 300, 50, 50)
end)
automatic("auto y over 500", Enum.AutomaticSize.Y, function(f)
	f.CanvasSize = UDim2.fromOffset(0, 500)
	kid(f, 0, 300, 50, 50)
end)
automatic("auto xy", Enum.AutomaticSize.XY, function(f)
	kid(f, 250, 10, 100, 20)
	kid(f, 10, 150, 10, 30)
end)
automatic("auto y padded", Enum.AutomaticSize.Y, function(f)
	local p = Instance.new("UIPadding")
	p.PaddingTop = UDim.new(0, 10)
	p.PaddingBottom = UDim.new(0, 20)
	p.Parent = f
	kid(f, 0, 300, 50, 50)
end)
automatic("auto y hidden", Enum.AutomaticSize.Y, function(f)
	kid(f, 0, 300, 50, 50).Visible = false
	kid(f, 0, 100, 50, 50)
end)
automatic("auto y deep", Enum.AutomaticSize.Y, function(f)
	kid(kid(f, 0, 0, 50, 50), 0, 400, 50, 50)
end)
automatic("auto y list", Enum.AutomaticSize.Y, function(f)
	local l = Instance.new("UIListLayout")
	l.Padding = UDim.new(0, 5)
	l.Parent = f
	for _ = 1, 5 do
		kid(f, 0, 0, 150, 60)
	end
end)

-- Kept within the grown canvas when set after the children.
local f = Instance.new("ScrollingFrame")
f.Size = UDim2.fromOffset(200, 100)
f.CanvasSize = UDim2.new()
f.AutomaticCanvasSize = Enum.AutomaticSize.Y
Instance.new("UIListLayout").Parent = f
for _ = 1, 5 do
	kid(f, 0, 0, 100, 60)
end
f.Parent = box
f.CanvasPosition = Vector2.new(0, 1000)
log("auto position", v(f.CanvasPosition))
kid(f, 0, 0, 100, 60)
f.CanvasPosition = Vector2.new(0, 1000)
log("auto grown position", v(f.CanvasPosition))

screen:Destroy()
