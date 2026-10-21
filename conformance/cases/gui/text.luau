-- Text: TextBounds and TextFits for sizes, fonts, lines, wrapping,
-- TextScaled and rich text; AutomaticSize growing to text; the Font and
-- FontSize views and TextSize's limits. Values Rook's per-glyph rounding
-- matches exactly (docs/behavior/gui-text.md#widths).
--! client
local player = game:GetService("Players").LocalPlayer
local screen = Instance.new("ScreenGui")
screen.Parent = player:WaitForChild("PlayerGui")

local function label(props)
	local l = Instance.new("TextLabel")
	l.Size = UDim2.fromOffset(400, 200)
	for k, value in props do
		l[k] = value
	end
	l.Parent = screen
	return l
end
local function show(name, l)
	log(name, l.TextBounds, l.TextFits)
end

local hello = "Hello, world!"
show("source sans 24", label({ Text = hello, TextSize = 24, Font = Enum.Font.SourceSans }))
show("legacy 48", label({ Text = hello, TextSize = 48, Size = UDim2.fromOffset(2000, 400) }))
show("legacy 13", label({ Text = "Hello", TextSize = 13 }))
show("ten H", label({ Text = string.rep("H", 10), TextSize = 100, Font = Enum.Font.SourceSans, Size = UDim2.fromOffset(3000, 400) }))
show("empty", label({ Text = "", TextSize = 24 }))
show("trailing spaces", label({ Text = "a   ", TextSize = 24 }))
show("two lines", label({ Text = "A\nB", TextSize = 24 }))
show("trailing newline", label({ Text = "A\n", TextSize = 24 }))
show("line height", label({ Text = "A\nB", TextSize = 24, LineHeight = 1.5 }))
local unwrapped = label({ Text = "The quick brown fox jumps over the lazy dog", TextSize = 24, Size = UDim2.fromOffset(150, 200) })
log("unwrapped", unwrapped.TextBounds.Y, unwrapped.TextFits)
local split = label({ Text = "Supercalifragilistic word", TextSize = 24, TextWrapped = true, Size = UDim2.fromOffset(60, 200) })
log("split word", split.TextBounds.Y, split.TextFits)
show("scaled", label({ Text = "Hi", TextScaled = true, Size = UDim2.fromOffset(200, 50) }))
show("scaled to 100", label({ Text = "Hi", TextScaled = true, Size = UDim2.fromOffset(600, 400) }))
show("rich size", label({ Text = 'A<font size="40">B</font>', TextSize = 24, RichText = true }))
show("rich escapes", label({ Text = "a &amp; b &lt;", TextSize = 24, RichText = true }))
log("content text", label({ Text = "<b>Hi</b> &amp; you", RichText = true }).ContentText)

local auto = label({ Text = hello, TextSize = 24, Font = Enum.Font.SourceSans, Size = UDim2.new(), AutomaticSize = Enum.AutomaticSize.XY })
log("automatic", auto.AbsoluteSize)

local l = label({})
l.Font = Enum.Font.SourceSansBold
log("font face", l.FontFace)
l.FontFace = Font.new("rbxasset://fonts/families/Arial.json", Enum.FontWeight.Bold)
log("font", l.Font)
l.FontFace = Font.fromId(12187365364)
log("other font", l.Font)
log("from enum", Font.fromEnum(Enum.Font.GothamBlack))
l.TextSize = 13
log("font size", l.FontSize)
l.FontSize = Enum.FontSize.Size36
log("text size", l.TextSize)
l.TextSize = 500
log("too big", l.TextSize)
l.TextSize = -3
log("too small", l.TextSize)

screen:Destroy()
