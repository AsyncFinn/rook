--!nocheck
--[[
	Recording of Roblox's in-game menu: the top bar's menu button and the
	SettingsHub menu it opens (RobloxGui.SettingsClippingShield), as a
	running Studio client draws them. Its output is studio-<version>.json
	beside this file.

	It records only measured values: properties read off the live
	instances, and per-frame samples of the open and close animation. It
	reads no script source and records no image or texture content ids
	(an image's presence is recorded as `hasImage`, with its color,
	transparency, scale type and slice rect).

	How it runs
	-----------
	Run in the Play-mode *client* DataModel of Studio, from a context that
	can read CoreGui (the Studio MCP server's `execute_luau` with
	datamodel_type = "Client" can; a game LocalScript can't). Running this
	file installs its functions as `shared.MenuDump` and returns a short
	help text; every later step is a one-line call. State is kept in
	`shared.MenuDump.result` between calls, so the steps can be separate
	`execute_luau` runs, with the inputs below sent in between (the MCP's
	`user_keyboard_input` / `user_mouse_input`; clicks can target an
	instance path such as
	`game.CoreGui.RobloxGui.SettingsClippingShield.SettingsShield.MenuContainer.Page.HubBar.TabHeaderContainer.HubBarContainer.GameSettingsTab`).
	Scripts in this context can't open the menu themselves:
	`GuiService:SetMenuIsOpen` and `VirtualInputManager` need RobloxScript.

	The sequence that made studio-0.748.json (Studio 0.748.0.7481526,
	Studio's viewport 1760x990, a baseplate place, Play Solo; P is
	game.CoreGui.RobloxGui.SettingsClippingShield.SettingsShield.MenuContainer.Page):

	  [menu open]  M.reset()  M.armAnimation("close")  [Escape]
	               M.collectAnimation("close")
	  M.snapshot("closed", M.ROOTS_CLOSED)
	  [mouse to the menu button, TopBarApp...IconHitArea]
	               M.snapshot("closed-icon-hover", M.ROOTS_ICON)
	  [mouse to 1700, 600]  M.armAnimation("open")  [Escape]
	               M.collectAnimation("open")
	  M.snapshot("open-people", M.ROOTS_OPEN)     -- it opens on People
	  [mouse to P.BottomButtonFrame.MenuButtons.MenuButtonContainer1.MenuButton]
	               M.snapshot("hover-button-leave", {{M.PATHS.menuContainer .. "/Page/BottomButtonFrame"}})
	  [click P...HubBarContainer.GameSettingsTab, mouse out of the panel]
	               M.snapshot("open-settings", M.ROOTS_OPEN)
	               M.settingsRows("settings-rows")
	  [mouse onto the Output Device row]
	               M.snapshot("hover-row-output-device", {{M.PATHS.pageInner .. "/Page/Output DeviceFrame"}})
	  [click CapturesTab]     M.snapshot("open-captures", M.ROOTS_OPEN)
	  [click ReportAbuseTab]  M.snapshot("open-report", M.ROOTS_OPEN)
	  [click HelpTab]         M.snapshot("open-help", M.ROOTS_OPEN)
	  [Escape]  M.setSmallWindow(630, 990)
	  M.armAnimation("small-open")  [Escape]  M.collectAnimation("small-open")
	  M.snapshot("small-open-people", M.ROOTS_OPEN)
	  [click GameSettingsTab]  M.snapshot("small-open-settings", M.ROOTS_OPEN)
	                           M.settingsRows("small-settings-rows")
	  [Escape]  M.restoreWindow(630, 990)
	  M.note(...)  (the notes in the JSON)
	  M.emit()  -> the JSON (M.chunk(i, n) returns it in pieces for
	              transports that cap a call's output; the MCP's is 100,000
	              characters)

	studio-<version>.json is M.emit()'s output re-indented, its keys in the
	order M.emit() wrote them, each element of a "nodes", "rows" or
	"events" list and each class in "classDefaults" on one line:

	  import json, sys
	  ONE = {"nodes", "rows", "events"}
	  def fmt(v, ind, key=None, one=False):
	      if one or not isinstance(v, (dict, list)) or key == "columns" or (
	              isinstance(v, list) and not any(isinstance(x, (dict, list)) for x in v)):
	          return json.dumps(v, ensure_ascii=False)
	      pad, inner = "  " * ind, "  " * (ind + 1)
	      if isinstance(v, list):
	          return "[\n" + ",\n".join(inner + fmt(x, ind + 1, one=key in ONE) for x in v) + "\n" + pad + "]"
	      return "{\n" + ",\n".join(inner + json.dumps(k) + ": " + fmt(x, ind + 1, k, one=key == "classDefaults")
	                                for k, x in v.items()) + "\n" + pad + "}"
	  open(sys.argv[2], "w", encoding="utf-8", newline="\n").write(
	      fmt(json.load(open(sys.argv[1], encoding="utf-8")), 0) + "\n")

	Coordinates: AbsolutePosition is in GUI space, which starts under the
	58-pixel top bar inset (guiInsetTopLeft) even for IgnoreGuiInset
	ScreenGuis, so the screen's top edge is y = -58. mouseLocation
	(UserInputService:GetMouseLocation) is in screen space.

	Known gap: styled values. Parts of the menu are Foundation components
	styled through StyleSheets (the subtrees beside a
	FoundationCursorContainer: the section headers and dividers of the
	Settings page, the bottom buttons' contents, the People, Report, Gallery
	and Help pages, the top bar's menu button holder). Reading a property
	gives the instance's own value, not what a style rule sets, so for
	those the recorded FontFace/TextSize/TextColor3/background values can
	be the class defaults while something else is drawn. Absolute*,
	TextBounds and layout results are computed after styling and are right
	everywhere; the SettingsHub's own rows, tabs, panel and sliders set
	their properties directly and are unaffected. Setting M.STYLED = true
	before snapshotting reads each property through Instance:GetStyled
	instead and lists the properties where it differs under `styled`;
	that path hadn't been run when studio-0.748.json was made (Studio was
	closed before it could be).

	Format of a node (one per instance, depth first, children in
	GetChildren order):
	  path   CoreGui-relative, "/"-separated; a second sibling of the same
	         name is "Name#2"
	  class  ClassName
	  shown  Visible here and in every ancestor, and its LayerCollector
	         enabled
	  ...    properties. Settable properties equal to a fresh
	         Instance.new(class)'s are left out; those defaults are in
	         `classDefaults`, read the same way. Computed ones (Absolute*,
	         TextBounds, TextFits, Visible) are always present.
	Values: Color3 as [r, g, b] 0-255; UDim2 as [xScale, xOffset, yScale,
	yOffset]; UDim as [scale, offset]; Vector2 as [x, y]; Rect as [minX,
	minY, maxX, maxY]; enums by name; Font as {Family, Weight, Style};
	sequences as keypoint lists; numbers rounded to 4 places.

	Redacted: the local player's name, display name and user id, and the
	audio output device names, are replaced by placeholders wherever they
	occur in a Text or a Name.
]]

local CoreGui = game:GetService("CoreGui")
local GuiService = game:GetService("GuiService")
local Players = game:GetService("Players")
local RunService = game:GetService("RunService")
local UserInputService = game:GetService("UserInputService")
local Lighting = game:GetService("Lighting")

local M = shared.MenuDump or {}
shared.MenuDump = M

-- Roots of each kind of snapshot, CoreGui-relative.
local SHIELD = "RobloxGui/SettingsClippingShield"
local MENU = SHIELD .. "/SettingsShield/MenuContainer"
M.PATHS = {
	shield = SHIELD,
	settingsShield = SHIELD .. "/SettingsShield",
	menuContainer = MENU,
	hubBar = MENU .. "/Page/HubBar",
	pageInner = MENU .. "/Page/PageViewClipper/PageView/PageViewInnerFrame",
	menuIcon = "TopBarApp/TopBarApp/MenuIconHolder",
}
-- {path, maxDepth}; maxDepth 0 is the instance alone.
M.ROOTS_CLOSED = {
	{ "RobloxGui", 0 },
	{ "TopBarApp/TopBarApp" },
	{ "TopBarApp/TopBarScrim" },
	{ SHIELD },
}
M.ROOTS_ICON = { { "TopBarApp/TopBarApp/MenuIconHolder" } }
M.ROOTS_OPEN = {
	{ "RobloxGui", 0 },
	{ "TopBarApp/TopBarApp/MenuIconHolder" },
	{ "TopBarApp/TopBarScrim" },
	{ SHIELD },
	-- the dropdown lists the Settings page opens (they exist, hidden, while shut)
	{ "RobloxGui/DropDownFullscreenFrame" },
	{ "RobloxGui/DropDownFullscreenFrame#2" },
	{ "RobloxGui/DropDownFullscreenFrame#3" },
}

---------------------------------------------------------------------------
-- Values

local function round(n)
	if n ~= n then
		return "nan"
	elseif n == math.huge then
		return "inf"
	elseif n == -math.huge then
		return "-inf"
	end
	local r = math.floor(n * 10000 + 0.5) / 10000
	if r == 0 then
		r = 0 -- no -0
	end
	return r
end

local function color(c)
	return { math.floor(c.R * 255 + 0.5), math.floor(c.G * 255 + 0.5), math.floor(c.B * 255 + 0.5) }
end

local function value(v)
	local t = typeof(v)
	if t == "number" then
		return round(v)
	elseif t == "boolean" or t == "string" then
		return v
	elseif t == "Color3" then
		return color(v)
	elseif t == "UDim2" then
		return { round(v.X.Scale), v.X.Offset, round(v.Y.Scale), v.Y.Offset }
	elseif t == "UDim" then
		return { round(v.Scale), v.Offset }
	elseif t == "Vector2" then
		return { round(v.X), round(v.Y) }
	elseif t == "Vector3" then
		return { round(v.X), round(v.Y), round(v.Z) }
	elseif t == "Rect" then
		return { round(v.Min.X), round(v.Min.Y), round(v.Max.X), round(v.Max.Y) }
	elseif t == "EnumItem" then
		return v.Name
	elseif t == "Font" then
		return { Family = v.Family, Weight = v.Weight.Name, Style = v.Style.Name }
	elseif t == "ColorSequence" then
		local out = {}
		for _, k in v.Keypoints do
			table.insert(out, { round(k.Time), color(k.Value) })
		end
		return out
	elseif t == "NumberSequence" then
		local out = {}
		for _, k in v.Keypoints do
			table.insert(out, { round(k.Time), round(k.Value), round(k.Envelope) })
		end
		return out
	elseif t == "NumberRange" then
		return { round(v.Min), round(v.Max) }
	elseif t == "nil" then
		return nil
	end
	return tostring(v)
end

---------------------------------------------------------------------------
-- JSON, keys sorted (a few first), so the output is stable

local FIRST = { "_about", "label", "path", "class", "shown", "pruned" }
local FIRST_RANK = {}
for i, k in FIRST do
	FIRST_RANK[k] = i
end

local function keyLess(a, b)
	local ra, rb = FIRST_RANK[a], FIRST_RANK[b]
	if ra and rb then
		return ra < rb
	elseif ra then
		return true
	elseif rb then
		return false
	end
	return a < b
end

local ESC =
	{ ['"'] = '\\"', ["\\"] = "\\\\", ["\b"] = "\\b", ["\f"] = "\\f", ["\n"] = "\\n", ["\r"] = "\\r", ["\t"] = "\\t" }
local function str(s)
	return '"' .. s:gsub('[%c"\\]', function(c)
		return ESC[c] or string.format("\\u%04x", string.byte(c))
	end) .. '"'
end

local function isArray(t)
	local n = #t
	if n == 0 then
		return next(t) == nil
	end
	local count = 0
	for _ in t do
		count += 1
	end
	return count == n
end

local function encode(v, buf)
	local t = type(v)
	if t == "table" then
		if isArray(v) then
			table.insert(buf, "[")
			for i, x in v do
				if i > 1 then
					table.insert(buf, ",")
				end
				encode(x, buf)
			end
			table.insert(buf, "]")
		else
			local keys = {}
			for k in v do
				table.insert(keys, tostring(k))
			end
			table.sort(keys, keyLess)
			table.insert(buf, "{")
			for i, k in keys do
				if i > 1 then
					table.insert(buf, ",")
				end
				table.insert(buf, str(k))
				table.insert(buf, ":")
				encode(v[k], buf)
			end
			table.insert(buf, "}")
		end
	elseif t == "string" then
		table.insert(buf, str(v))
	elseif t == "number" then
		if v == math.floor(v) and math.abs(v) < 2 ^ 53 then
			table.insert(buf, string.format("%d", v))
		else
			local s = string.format("%.4f", v):gsub("0+$", ""):gsub("%.$", "")
			table.insert(buf, s)
		end
	elseif t == "boolean" then
		table.insert(buf, tostring(v))
	else
		table.insert(buf, "null")
	end
end

local function toJson(v)
	local buf = {}
	encode(v, buf)
	return table.concat(buf)
end

---------------------------------------------------------------------------
-- Redaction of personal values in Text and Name

local function redactions()
	local list = {}
	local lp = Players.LocalPlayer
	if lp then
		table.insert(list, { lp.Name, "<PlayerName>" })
		table.insert(list, { lp.DisplayName, "<DisplayName>" })
		if lp.UserId > 0 then
			table.insert(list, { tostring(lp.UserId), "<UserId>" })
		end
	end
	-- Output device names: the Output Device row's choices.
	local selector = M.find(M.PATHS.pageInner .. "/Page/Output DeviceFrame/Selector")
	if selector then
		local n = 0
		for _, c in selector:GetChildren() do
			if c:IsA("TextLabel") and c.Name:match("^Selection") then
				local inner = c.Text:match("^Default %((.*)%)$")
				if not inner then
					n += 1
					table.insert(list, { c.Text, "<output device " .. n .. ">" })
				end
			end
		end
	end
	table.sort(list, function(a, b)
		return #a[1] > #b[1]
	end)
	return list
end

local function redact(s, list)
	for _, r in list do
		if r[1] ~= "" then
			local i = 1
			while true do
				local a, b = string.find(s, r[1], i, true)
				if not a then
					break
				end
				s = s:sub(1, a - 1) .. r[2] .. s:sub(b + 1)
				i = a + #r[2]
			end
		end
	end
	return s
end

---------------------------------------------------------------------------
-- Instances

-- CoreGui-relative path with "#k" for the k-th same-named sibling.
function M.find(path)
	local node = CoreGui
	for part in string.gmatch(path, "[^/]+") do
		local name, k = part:match("^(.-)#(%d+)$")
		name = name or part
		k = tonumber(k) or 1
		local found
		local seen = 0
		for _, c in node:GetChildren() do
			if c.Name == name then
				seen += 1
				if seen == k then
					found = c
					break
				end
			end
		end
		if not found then
			return nil
		end
		node = found
	end
	return node
end

-- Properties by class (IsA). COMPUTED are always written.
local PROPS = {
	{ "GuiBase2d", { "AbsolutePosition", "AbsoluteSize", "AbsoluteRotation" } },
	{ "LayerCollector", { "Enabled", "ResetOnSpawn", "ZIndexBehavior" } },
	{
		"ScreenGui",
		{ "DisplayOrder", "IgnoreGuiInset", "ScreenInsets", "SafeAreaCompatibility", "ClipToDeviceSafeArea" },
	},
	{
		"GuiObject",
		{
			"Visible",
			"Size",
			"Position",
			"AnchorPoint",
			"BackgroundColor3",
			"BackgroundTransparency",
			"BorderSizePixel",
			"BorderColor3",
			"BorderMode",
			"ZIndex",
			"LayoutOrder",
			"ClipsDescendants",
			"AutomaticSize",
			"Rotation",
			"SizeConstraint",
			"Active",
			"Interactable",
		},
	},
	{ "GuiButton", { "AutoButtonColor", "Modal", "Selected", "Style" } },
	{
		"TextLabel",
		{
			"Text",
			"FontFace",
			"TextSize",
			"TextColor3",
			"TextTransparency",
			"TextXAlignment",
			"TextYAlignment",
			"TextScaled",
			"TextWrapped",
			"TextTruncate",
			"RichText",
			"LineHeight",
			"TextStrokeColor3",
			"TextStrokeTransparency",
			"TextDirection",
			"TextBounds",
			"TextFits",
		},
	},
	{ "TextBox", { "PlaceholderText", "PlaceholderColor3", "ClearTextOnFocus", "TextEditable", "MultiLine" } },
	{
		"ImageLabel",
		{
			"ImageColor3",
			"ImageTransparency",
			"ScaleType",
			"SliceCenter",
			"SliceScale",
			"TileSize",
			"ResampleMode",
			"ImageRectOffset",
			"ImageRectSize",
		},
	},
	{
		"ScrollingFrame",
		{
			"CanvasSize",
			"CanvasPosition",
			"AutomaticCanvasSize",
			"ScrollBarThickness",
			"ScrollBarImageColor3",
			"ScrollBarImageTransparency",
			"ScrollingDirection",
			"ScrollingEnabled",
			"VerticalScrollBarInset",
			"HorizontalScrollBarInset",
			"VerticalScrollBarPosition",
			"ElasticBehavior",
			"AbsoluteCanvasSize",
			"AbsoluteWindowSize",
		},
	},
	{ "CanvasGroup", { "GroupTransparency", "GroupColor3" } },
	{ "UICorner", { "CornerRadius" } },
	{
		"UIStroke",
		{
			"Color",
			"Thickness",
			"Transparency",
			"ApplyStrokeMode",
			"LineJoinMode",
			"Enabled",
			"BorderStrokePosition",
			"BorderOffset",
			"StrokeSizingMode",
			"ZIndex",
		},
	},
	{ "UIPadding", { "PaddingTop", "PaddingBottom", "PaddingLeft", "PaddingRight" } },
	{
		"UIGridStyleLayout",
		{ "FillDirection", "SortOrder", "HorizontalAlignment", "VerticalAlignment", "AbsoluteContentSize" },
	},
	{ "UIListLayout", { "Padding", "HorizontalFlex", "VerticalFlex", "ItemLineAlignment", "Wraps" } },
	{
		"UIGridLayout",
		{ "CellSize", "CellPadding", "FillDirectionMaxCells", "StartCorner", "AbsoluteCellSize", "AbsoluteCellCount" },
	},
	{
		"UIPageLayout",
		{
			"Animated",
			"Circular",
			"EasingDirection",
			"EasingStyle",
			"Padding",
			"TweenTime",
			"GamepadInputEnabled",
			"ScrollWheelInputEnabled",
			"TouchInputEnabled",
		},
	},
	{ "UITableLayout", { "FillEmptySpaceColumns", "FillEmptySpaceRows", "MajorAxis", "Padding" } },
	{ "UISizeConstraint", { "MinSize", "MaxSize" } },
	{ "UITextSizeConstraint", { "MinTextSize", "MaxTextSize" } },
	{ "UIAspectRatioConstraint", { "AspectRatio", "AspectType", "DominantAxis" } },
	{ "UIGradient", { "Color", "Transparency", "Rotation", "Offset", "Enabled" } },
	{ "UIScale", { "Scale" } },
	{ "UIFlexItem", { "FlexMode", "GrowRatio", "ShrinkRatio", "ItemLineAlignment" } },
	{ "ValueBase", { "Value" } },
}
local COMPUTED = {
	AbsolutePosition = true,
	AbsoluteSize = true,
	AbsoluteRotation = true,
	Visible = true,
	TextBounds = true,
	TextFits = true,
	AbsoluteContentSize = true,
	AbsoluteCanvasSize = true,
	AbsoluteWindowSize = true,
	AbsoluteCellSize = true,
	AbsoluteCellCount = true,
}
-- TextButton and TextBox read TextLabel's list; ImageButton ImageLabel's.
local ALIAS = { TextButton = "TextLabel", TextBox = "TextLabel", ImageButton = "ImageLabel" }

local function propNames(inst)
	local names = {}
	for _, entry in PROPS do
		local cls = entry[1]
		local applies = inst:IsA(cls)
		if not applies then
			for from, to in ALIAS do
				if to == cls and inst:IsA(from) then
					applies = true
				end
			end
		end
		if applies then
			for _, p in entry[2] do
				table.insert(names, p)
			end
		end
	end
	return names
end

local function recorded(inst)
	return inst:IsA("GuiBase2d") or inst:IsA("UIBase") or inst:IsA("Folder") or inst:IsA("ValueBase")
end

-- M.STYLED: read through Instance:GetStyled (see "Known gap" above).
M.STYLED = M.STYLED or false

local function readProps(inst)
	local out = {}
	local styled = {}
	for _, p in propNames(inst) do
		local ok, v = pcall(function()
			return inst[p]
		end)
		if ok then
			out[p] = value(v)
		end
		if M.STYLED then
			local sok, sv = pcall(function()
				return inst:GetStyled(p)
			end)
			if sok and sv ~= nil then
				local s = value(sv)
				if not ok or toJson(s) ~= toJson(out[p]) then
					table.insert(styled, p)
				end
				out[p] = s
			end
		end
	end
	if #styled > 0 then
		table.sort(styled)
		out.styled = styled
	end
	if inst:IsA("ImageLabel") or inst:IsA("ImageButton") then
		local has = false
		pcall(function()
			has = inst.Image ~= ""
		end)
		pcall(function()
			has = has or inst.ImageContent.SourceType ~= Enum.ContentSourceType.None
		end)
		out.hasImage = has
	end
	if inst:IsA("ImageButton") then
		out.hasHoverImage = inst.HoverImage ~= ""
		out.hasPressedImage = inst.PressedImage ~= ""
	end
	return out
end

-- Defaults of a fresh Instance.new(class), keyed by class; nil if it
-- can't be created.
local function defaultsFor(class)
	M.result.classDefaults = M.result.classDefaults or {}
	local d = M.result.classDefaults[class]
	if d == nil then
		local ok, inst = pcall(Instance.new, class)
		if ok and inst then
			d = readProps(inst)
			for k in COMPUTED do
				d[k] = nil
			end
			d.styled = nil
			d.hasImage = nil
			d.hasHoverImage = nil
			d.hasPressedImage = nil
			inst:Destroy()
		else
			d = false
		end
		M.result.classDefaults[class] = d
	end
	return d or nil
end

local function same(a, b)
	return toJson(a) == toJson(b)
end

local function shownSelf(inst)
	if inst:IsA("GuiObject") then
		return inst.Visible
	elseif inst:IsA("LayerCollector") then
		return inst.Enabled
	end
	return true
end

local function ancestorsShown(inst)
	local p = inst.Parent
	while p and p ~= CoreGui do
		if not shownSelf(p) then
			return false
		end
		p = p.Parent
	end
	return true
end

-- One node per recorded instance under root, depth first.
local function walk(root, rootPath, maxDepth, nodes, list)
	local function visit(inst, path, depth, parentShown)
		if not recorded(inst) then
			return
		end
		local shown = parentShown and shownSelf(inst)
		local node = readProps(inst)
		local d = defaultsFor(inst.ClassName)
		if d then
			for k, v in node do
				if not COMPUTED[k] and d[k] ~= nil and same(d[k], v) then
					node[k] = nil
				end
			end
		end
		if type(node.Text) == "string" then
			node.Text = redact(node.Text, list)
		end
		if type(node.PlaceholderText) == "string" then
			node.PlaceholderText = redact(node.PlaceholderText, list)
		end
		node.path = redact(path, list)
		node.class = inst.ClassName
		node.shown = shown
		table.insert(nodes, node)
		if maxDepth and depth >= maxDepth then
			return
		end
		-- A page in the page view that isn't showing is kept as one node.
		if inst.Parent and inst.Parent.Name == "PageViewInnerFrame" and inst:IsA("GuiObject") and not inst.Visible then
			if #inst:GetChildren() > 0 then
				node.pruned = true
			end
			return
		end
		local counts = {}
		for _, c in inst:GetChildren() do
			counts[c.Name] = (counts[c.Name] or 0) + 1
			local part = c.Name
			if counts[c.Name] > 1 then
				part ..= "#" .. counts[c.Name]
			end
			visit(c, path .. "/" .. part, depth + 1, shown)
		end
	end
	visit(root, rootPath, 0, ancestorsShown(root))
end

local function viewInfo()
	local inset0, inset1 = GuiService:GetGuiInset()
	local lp = Players.LocalPlayer
	local info = {
		viewport = value(workspace.CurrentCamera.ViewportSize),
		guiInsetTopLeft = value(inset0),
		guiInsetBottomRight = value(inset1),
		menuIsOpen = GuiService.MenuIsOpen,
		clock = round(os.clock()),
	}
	pcall(function()
		info.topbarInset = value(GuiService.TopbarInset)
	end)
	pcall(function()
		info.preferredTextSize = GuiService.PreferredTextSize.Name
	end)
	pcall(function()
		info.preferredTransparency = round(GuiService.PreferredTransparency)
	end)
	pcall(function()
		info.reducedMotion = GuiService.ReducedMotionEnabled
	end)
	pcall(function()
		info.touchEnabled = UserInputService.TouchEnabled
		info.keyboardEnabled = UserInputService.KeyboardEnabled
		info.mouseEnabled = UserInputService.MouseEnabled
		info.gamepadEnabled = UserInputService.GamepadEnabled
	end)
	-- The selected tab, by its underline.
	local hub = M.find(M.PATHS.hubBar .. "/TabHeaderContainer/HubBarContainer")
	if hub then
		for _, tab in hub:GetChildren() do
			local sel = tab:FindFirstChild("TabSelection")
			if sel and sel.Visible then
				info.selectedTab = tab.Name
			end
		end
	end
	local mouse = UserInputService:GetMouseLocation()
	info.mouseLocation = value(mouse)
	return info
end

---------------------------------------------------------------------------
-- Snapshots

function M.reset()
	M.result = { snapshots = {}, animations = {}, notes = {}, classDefaults = {} }
	return "reset"
end
M.result = M.result or { snapshots = {}, animations = {}, notes = {}, classDefaults = {} }

local function store(kind, label, entry)
	entry.label = label
	local list = M.result[kind]
	for i, e in list do
		if e.label == label then
			list[i] = entry
			return
		end
	end
	table.insert(list, entry)
end

-- roots: list of {path, maxDepth?}. Returns a one-line summary.
function M.snapshot(label, roots, note)
	local list = redactions()
	local nodes = {}
	local missing = {}
	for _, r in roots do
		local inst = M.find(r[1])
		if inst then
			walk(inst, r[1], r[2], nodes, list)
		else
			table.insert(missing, r[1])
		end
	end
	local entry = viewInfo()
	entry.nodes = nodes
	if #missing > 0 then
		entry.missingRoots = missing
	end
	entry.note = note
	store("snapshots", label, entry)
	return string.format(
		"%s: %d nodes, viewport %s, tab %s, missing %d",
		label,
		#nodes,
		tostring(workspace.CurrentCamera.ViewportSize),
		tostring(entry.selectedTab),
		#missing
	)
end

function M.note(key, text)
	M.result.notes[key] = text
	return key
end

---------------------------------------------------------------------------
-- The Settings page, row by row

local function textOf(inst)
	return inst and (inst:IsA("TextLabel") or inst:IsA("TextButton") or inst:IsA("TextBox")) and inst.Text or nil
end

function M.settingsRows(label, pageName)
	local list = redactions()
	local page = M.find(M.PATHS.pageInner .. "/" .. (pageName or "Page"))
	if not page then
		return "no settings page"
	end
	local items = {}
	for _, c in page:GetChildren() do
		if c:IsA("GuiObject") then
			table.insert(items, c)
		end
	end
	table.sort(items, function(a, b)
		if a.LayoutOrder ~= b.LayoutOrder then
			return a.LayoutOrder < b.LayoutOrder
		end
		return a.AbsolutePosition.Y < b.AbsolutePosition.Y
	end)
	local rows = {}
	local section
	for _, it in items do
		local row = {
			name = it.Name,
			layoutOrder = it.LayoutOrder,
			visible = it.Visible,
			absolutePosition = value(it.AbsolutePosition),
			absoluteSize = value(it.AbsoluteSize),
		}
		if it.Name == "HeaderWrapper" then
			local t = it:FindFirstChild("Text", true)
			row.kind = "header"
			row.text = textOf(t)
			if it.Visible then
				section = row.text
			end
		elseif it.Name == "DividerWrapper" then
			row.kind = "divider"
		else
			row.kind = "row"
			row.section = section
			local base = it.Name:gsub("Frame$", "")
			row.label = textOf(it:FindFirstChild(base .. "Label", true))
			row.description = textOf(it:FindFirstChild(base .. "Description", true))
			local slider = it:FindFirstChild("Slider")
			local selector = it:FindFirstChild("Selector")
			local dropdown = it:FindFirstChild("DropDownFrameButton")
			local textbox = it:FindFirstChildWhichIsA("TextBox", true)
			if slider then
				row.control = "slider"
				local steps = slider:FindFirstChild("StepsContainer", true)
				local n, filled = 0, 0
				local colors = {}
				if steps then
					for _, s in steps:GetChildren() do
						if s.Name:match("^Step%d+$") then
							n += 1
							local key = toJson(color(s.BackgroundColor3))
							colors[key] = (colors[key] or 0) + 1
						end
					end
					-- filled steps are the light ones
					for i = 1, n do
						local s = steps:FindFirstChild("Step" .. i)
						if s and s.BackgroundColor3.R > 0.5 then
							filled += 1
						end
					end
				end
				row.steps = n
				row.stepsFilled = filled
				local labels = slider:FindFirstChild("SliderLabels")
				if labels then
					row.endLabels = {
						textOf(labels:FindFirstChild("LeftLabel")),
						textOf(labels:FindFirstChild("RightLabel")),
					}
				end
				local left = slider:FindFirstChild("LeftButton", true)
				local right = slider:FindFirstChild("RightButton", true)
				row.minusShown = left and left.Visible or false
				row.plusShown = right and right.Visible or false
				if textbox then
					row.control = "slider+textbox"
					row.textboxText = textbox.Text
					row.textboxPlaceholder = textbox.PlaceholderText
				end
			elseif selector then
				row.control = "selector"
				local options = {}
				for _, s in selector:GetChildren() do
					local idx = tonumber(s.Name:match("^Selection(%d+)$"))
					if idx then
						options[idx] = redact(s.Text, list)
						if s.Visible then
							row.current = redact(s.Text, list)
						end
					end
				end
				row.options = options
				for _, s in it:GetChildren() do
					if
						s:IsA("TextLabel") and s.Name:match("Override")
						or (s:IsA("TextLabel") and s.Name:match("^PerformanceStatsLabel$"))
					then
						row.overrideLabel = { text = s.Text, visible = s.Visible }
					end
				end
			elseif dropdown then
				row.control = "dropdown"
				row.current = redact(textOf(dropdown:FindFirstChild("DropDownFrameTextLabel")) or "", list)
				local arrow = dropdown:FindFirstChild("DropDownImage")
				row.arrowShown = arrow and arrow.Visible or false
			else
				local button = it:FindFirstChildWhichIsA("GuiButton")
				if button and textOf(button:FindFirstChildWhichIsA("TextLabel")) then
					row.control = "button"
					row.buttonText = textOf(button:FindFirstChildWhichIsA("TextLabel"))
				else
					row.control = "unknown"
				end
			end
		end
		table.insert(rows, row)
	end
	store(
		"snapshots",
		label,
		{ kind = "settingsRows", rows = rows, viewport = value(workspace.CurrentCamera.ViewportSize) }
	)
	local shown = 0
	for _, r in rows do
		if r.kind == "row" and r.visible then
			shown += 1
		end
	end
	return string.format("%s: %d items, %d visible rows", label, #rows, shown)
end

---------------------------------------------------------------------------
-- Animation: per-frame samples from arming until the menu settles

local WATCH = {
	{ "shield", SHIELD, { "Visible" } },
	{
		"settingsShield",
		SHIELD .. "/SettingsShield",
		{
			"Visible",
			"Position",
			"AbsolutePosition",
			"BackgroundTransparency",
			"BackgroundColor3",
			"ImageTransparency",
		},
	},
	{
		"menuContainer",
		MENU,
		{ "Visible", "Position", "AbsolutePosition", "AbsoluteSize", "BackgroundTransparency", "ImageTransparency" },
	},
	{ "menuPage", MENU .. "/Page", { "Position", "AbsolutePosition" } },
	{ "darkenBackground", SHIELD .. "/DarkenBackground", { "Visible", "BackgroundTransparency", "ImageTransparency" } },
	{ "canvasGroup", SHIELD .. "/CanvasGroup", { "Visible", "GroupTransparency", "Position" } },
	{ "innerShow", M.PATHS.pageInner .. "/InnerCanvasGroupShow", { "Visible", "GroupTransparency", "Position" } },
	{ "innerHide", M.PATHS.pageInner .. "/InnerCanvasGroupHide", { "Visible", "GroupTransparency", "Position" } },
	{ "topBarScrim", "TopBarApp/TopBarScrim/TopBarScrim", { "Visible", "BackgroundTransparency" } },
	{
		"menuIcon",
		"TopBarApp/TopBarApp/MenuIconHolder/TriggerPoint/IconHitArea",
		{ "BackgroundTransparency", "BackgroundColor3", "AbsolutePosition" },
	},
}

local function blurEffects()
	local out = {}
	for _, parent in { workspace.CurrentCamera, Lighting } do
		if parent then
			for _, c in parent:GetChildren() do
				if c:IsA("BlurEffect") then
					table.insert(out, c)
				end
			end
		end
	end
	return out
end

function M.armAnimation(label, maxSeconds, settleSeconds)
	maxSeconds = maxSeconds or 6
	settleSeconds = settleSeconds or 1
	if M._anim and M._anim.conn then
		M._anim.conn:Disconnect()
	end
	local columns = { "t" }
	local getters = {}
	for _, w in WATCH do
		local inst = M.find(w[2])
		for _, p in w[3] do
			table.insert(columns, w[1] .. "." .. p)
			table.insert(getters, function()
				if not inst then
					return "missing"
				end
				local ok, v = pcall(function()
					return inst[p]
				end)
				if not ok then
					return "unreadable"
				end
				return value(v)
			end)
		end
	end
	table.insert(columns, "menuIsOpen")
	table.insert(getters, function()
		return GuiService.MenuIsOpen
	end)
	table.insert(columns, "blurSizes")
	table.insert(getters, function()
		local sizes = {}
		for _, b in blurEffects() do
			table.insert(sizes, b.Enabled and round(b.Size) or 0)
		end
		return sizes
	end)
	local a = {
		label = label,
		columns = columns,
		rows = {},
		events = {},
		start = os.clock(),
		done = false,
	}
	M._anim = a
	local function row()
		local r = { round(os.clock() - a.start) }
		for i, g in getters do
			r[i + 1] = g()
		end
		return r
	end
	local last = toJson({ table.unpack(row(), 2) })
	table.insert(a.rows, row())
	a.lastChange = nil
	a.inputConn = UserInputService.InputBegan:Connect(function(input)
		table.insert(
			a.events,
			{ round(os.clock() - a.start), "InputBegan " .. input.KeyCode.Name .. " " .. input.UserInputType.Name }
		)
	end)
	a.openConn = GuiService.MenuOpened:Connect(function()
		table.insert(a.events, { round(os.clock() - a.start), "GuiService.MenuOpened" })
	end)
	a.closeConn = GuiService.MenuClosed:Connect(function()
		table.insert(a.events, { round(os.clock() - a.start), "GuiService.MenuClosed" })
	end)
	a.conn = RunService.RenderStepped:Connect(function()
		local r = row()
		local key = toJson({ table.unpack(r, 2) })
		local now = os.clock()
		-- Every frame is kept (unchanged ones too, so the frame times are
		-- all there); before the first change, only the last three.
		table.insert(a.rows, r)
		if key ~= last then
			last = key
			a.lastChange = now
			a.firstChange = a.firstChange or now
		elseif not a.firstChange and #a.rows > 3 then
			table.remove(a.rows, 1)
		end
		if (a.lastChange and now - a.lastChange > settleSeconds) or now - a.start > maxSeconds + 30 then
			a.done = true
			a.conn:Disconnect()
			a.inputConn:Disconnect()
			a.openConn:Disconnect()
			a.closeConn:Disconnect()
		end
	end)
	return "armed " .. label
end

function M.collectAnimation(label, timeout)
	local a = M._anim
	if not a or a.label ~= label then
		return "not armed: " .. tostring(label)
	end
	local t0 = os.clock()
	while not a.done and os.clock() - t0 < (timeout or 8) do
		task.wait()
	end
	if not a.done then
		a.conn:Disconnect()
		a.inputConn:Disconnect()
		a.openConn:Disconnect()
		a.closeConn:Disconnect()
	end
	-- Trim trailing unchanged frames past the last change, keeping one.
	local lastChangeT = a.lastChange and round(a.lastChange - a.start)
	local rows = {}
	for _, r in a.rows do
		table.insert(rows, r)
		if lastChangeT and r[1] > lastChangeT then
			break
		end
	end
	local entry = {
		columns = a.columns,
		rows = rows,
		events = a.events,
		firstChangeT = a.firstChange and round(a.firstChange - a.start),
		lastChangeT = lastChangeT,
		settled = a.done,
		frames = #rows,
	}
	store("animations", label, entry)
	M._anim = nil
	return string.format(
		"%s: %d rows, first change %s, last change %s",
		label,
		#rows,
		tostring(entry.firstChangeT),
		tostring(entry.lastChangeT)
	)
end

---------------------------------------------------------------------------
-- Small window through the Device Simulator (a custom device). Errors are
-- returned, not thrown.

function M.setSmallWindow(width, height)
	local svc = game:GetService("StudioDeviceSimulatorService")
	local ok, err = pcall(function()
		local name = "MenuDump_" .. width .. "x" .. height
		local exists = false
		for _, id in svc:GetDeviceListAsync() do
			if id == name then
				exists = true
			end
		end
		if not exists then
			svc:CreateDeviceAsync({
				Name = name,
				Width = width,
				Height = height,
				PixelDensity = 96,
				DeviceForm = Enum.DeviceForm.Desktop,
			})
		end
		M._previousDevice = M._previousDevice or svc:GetDeviceAsync()
		svc:SetDeviceAsync(name)
	end)
	task.wait(0.5)
	return ok and ("viewport " .. tostring(workspace.CurrentCamera.ViewportSize)) or ("failed: " .. tostring(err))
end

function M.restoreWindow(width, height)
	local svc = game:GetService("StudioDeviceSimulatorService")
	local ok, err = pcall(function()
		-- Studio's own viewport is the device "studio"; "default" is no
		-- simulation.
		local prev = M._previousDevice
		if prev and prev ~= "default" then
			svc:SetDeviceAsync(prev)
		else
			svc:StopSimulationAsync()
		end
		M._previousDevice = nil
		if width then
			svc:RemoveDeviceAsync("MenuDump_" .. width .. "x" .. height)
		end
	end)
	task.wait(0.5)
	return (ok and "restored, viewport " or ("failed: " .. tostring(err) .. ", viewport "))
		.. tostring(workspace.CurrentCamera.ViewportSize)
end

---------------------------------------------------------------------------
-- Output

function M.emit()
	local r = M.result
	r._about =
		"Roblox's in-game menu (top bar menu button and SettingsHub), recorded by dump.luau beside this file from a running Studio client. Measured values only; see dump.luau's header for the format."
	r.studioVersion = version()
	return toJson(r)
end

function M.chunk(i, size)
	size = size or 90000
	if i == 1 or not M._emitted then
		M._emitted = M.emit()
	end
	local s = M._emitted
	local n = math.ceil(#s / size)
	return string.format("<<<CHUNK %d/%d>>>", i, n) .. s:sub((i - 1) * size + 1, i * size) .. "<<<END>>>"
end

return "MenuDump installed as shared.MenuDump: reset, snapshot, settingsRows, armAnimation, collectAnimation, setSmallWindow, restoreWindow, emit, chunk"
