--!nocheck
-- Rook's in-game menu: the button at the top left and the menu it opens,
-- with the player's settings (docs/behavior/gui/menu.md). A client runs it
-- as a CoreScript when Roblox's own CoreScripts don't, so it lays out as
-- Roblox's menu does (sizes and colors measured in Studio), with Rook's
-- logo and no art of Roblox's.

local CoreGui = game:GetService("CoreGui")
local GuiService = game:GetService("GuiService")
local UserInputService = game:GetService("UserInputService")
local ContextActionService = game:GetService("ContextActionService")
local SoundService = game:GetService("SoundService")

local GameSettings = UserSettings().GameSettings

local WHITE = Color3.new(1, 1, 1)
local BLACK = Color3.new(0, 0, 0)
-- The top bar button's and the rows' hover color.
local BUTTON_DARK = Color3.fromRGB(18, 18, 21)
local ROW_HOVER = Color3.fromRGB(33, 36, 38)
local STEP_ON = Color3.fromRGB(217, 217, 217)
local STEP_OFF = Color3.fromRGB(57, 59, 61)
local SECONDARY = Color3.fromRGB(207, 217, 250)
local PRIMARY = Color3.fromRGB(51, 95, 255)

local FONT = "rbxasset://fonts/families/BuilderSans.json"
local REGULAR = Font.new(FONT, Enum.FontWeight.Regular)
local MEDIUM = Font.new(FONT, Enum.FontWeight.Medium)
local BOLD = Font.new(FONT, Enum.FontWeight.Bold)

local function new(class, props, children)
	local object = Instance.new(class)
	for k, v in props do
		if k ~= "Parent" then
			object[k] = v
		end
	end
	for _, child in children or {} do
		child.Parent = object
	end
	if props.Parent then
		object.Parent = props.Parent
	end
	return object
end

local function corner(radius)
	return new("UICorner", { CornerRadius = radius })
end

local function label(props)
	props.BackgroundTransparency = 1
	props.TextColor3 = props.TextColor3 or WHITE
	props.FontFace = props.FontFace or MEDIUM
	return new("TextLabel", props)
end

-- Rook's logo, the white rook of resources/rook.svg, from frames: its
-- battlements, tower (with its window) and base, tilted as the icon is.
-- `hole` colors the window.
local function logo(size, hole)
	local box = new("Frame", {
		Name = "Logo",
		BackgroundTransparency = 1,
		Size = UDim2.fromOffset(size, size),
		AnchorPoint = Vector2.new(0.5, 0.5),
		Position = UDim2.fromScale(0.5, 0.5),
		Rotation = 10,
	})
	-- In the SVG's units, its 320-unit square from (96, 96), centered on the
	-- rook (256, 256).
	local function piece(name, x, y, w, h, color)
		new("Frame", {
			Name = name,
			BorderSizePixel = 0,
			BackgroundColor3 = color or WHITE,
			Position = UDim2.fromScale((x - 96) / 320, (y - 96) / 320),
			Size = UDim2.fromScale(w / 320, h / 320),
			Parent = box,
		}, { corner(UDim.new(0.04, 0)) })
	end
	for i, x in { 150, 228, 306 } do
		piece("Merlon" .. i, x, 104, 56, 44)
	end
	piece("Battlements", 150, 136, 212, 56)
	piece("Tower", 181, 204, 150, 136)
	piece("Window", 236, 256, 40, 40, hole)
	piece("Base", 136, 352, 240, 56)
	return box
end

-- An arrow head pointing left (-1) or right (1), two bars meeting at a
-- point, in an 18 by 30 box.
local function chevron(direction)
	local box = new("Frame", {
		Name = "Chevron",
		BackgroundTransparency = 1,
		Size = UDim2.fromOffset(18, 30),
		AnchorPoint = Vector2.new(0.5, 0.5),
		Position = UDim2.fromScale(0.5, 0.5),
	})
	for _, sign in { -1, 1 } do
		new("Frame", {
			BorderSizePixel = 0,
			BackgroundColor3 = Color3.fromRGB(204, 204, 204),
			AnchorPoint = Vector2.new(0.5, 0.5),
			Position = UDim2.new(0.5, 0, 0.5, sign * 6),
			Size = UDim2.fromOffset(17, 3),
			Rotation = sign * direction * -56,
			Parent = box,
		}, { corner(UDim.new(1, 0)) })
	end
	return box
end

-- A circled plus (or minus), the slider's ends.
local function circled(plus)
	local box = new("Frame", {
		BackgroundTransparency = 1,
		Size = UDim2.fromOffset(26, 26),
		AnchorPoint = Vector2.new(0.5, 0.5),
		Position = UDim2.fromScale(0.5, 0.5),
	}, {
		corner(UDim.new(1, 0)),
		new("UIStroke", { Color = WHITE, Thickness = 2 }),
	})
	local bar = function(w, h)
		new("Frame", {
			BorderSizePixel = 0,
			BackgroundColor3 = WHITE,
			AnchorPoint = Vector2.new(0.5, 0.5),
			Position = UDim2.fromScale(0.5, 0.5),
			Size = UDim2.fromOffset(w, h),
			Parent = box,
		})
	end
	bar(12, 2)
	if plus then
		bar(2, 12)
	end
	return box
end

local gui = new("ScreenGui", {
	Name = "RookMenu",
	ResetOnSpawn = false,
	IgnoreGuiInset = true,
	ZIndexBehavior = Enum.ZIndexBehavior.Sibling,
	DisplayOrder = 10,
})

-- The top bar's button, where Roblox has its logo. It stays bright over
-- the open menu, as Roblox's does.
local menuButton = new("ImageButton", {
	Name = "MenuButton",
	AutoButtonColor = false,
	BackgroundColor3 = BUTTON_DARK,
	BackgroundTransparency = 0.02,
	Position = UDim2.fromOffset(16, 12),
	Size = UDim2.fromOffset(44, 44),
	ZIndex = 2,
	Parent = gui,
}, { corner(UDim.new(1, 0)) })
local menuButtonLogo = logo(24, BUTTON_DARK)
menuButtonLogo.Parent = menuButton
local menuButtonHover = new("Frame", {
	Name = "Hover",
	BackgroundColor3 = WHITE,
	BackgroundTransparency = 1,
	Size = UDim2.fromScale(1, 1),
	Parent = menuButton,
}, { corner(UDim.new(1, 0)) })

-- The menu: the darkened screen, and the panel on it.
local shield = new("Frame", {
	Name = "Shield",
	Active = true,
	Visible = false,
	BackgroundColor3 = BLACK,
	BackgroundTransparency = 0.5,
	Size = UDim2.fromScale(1, 1),
	Parent = gui,
}, {
	-- Frees the mouse while the menu is open.
	new("TextButton", {
		Name = "Modal",
		Modal = true,
		Text = "",
		BackgroundTransparency = 1,
		Size = UDim2.fromScale(0, 0),
	}),
})

local panel = new("Frame", {
	Name = "MenuContainer",
	Active = true,
	BackgroundColor3 = BLACK,
	BackgroundTransparency = 0.06,
	AnchorPoint = Vector2.new(0.5, 0.5),
	Position = UDim2.fromScale(0.5, 0.5),
	Size = UDim2.new(1, -40, 1, -40),
	Parent = shield,
}, {
	corner(UDim.new(0, 10)),
	new("UISizeConstraint", { MaxSize = Vector2.new(840, 734) }),
})

-- The main view: tabs, the page and the buttons under it.
local main = new("Frame", {
	Name = "Main",
	BackgroundTransparency = 1,
	Size = UDim2.fromScale(1, 1),
	Parent = panel,
})

local hubBar = new("Frame", {
	Name = "HubBar",
	BackgroundTransparency = 1,
	Position = UDim2.fromOffset(20, 0),
	Size = UDim2.new(1, -40, 0, 60),
	Parent = main,
}, {
	new("Frame", {
		Name = "Divider",
		BorderSizePixel = 0,
		BackgroundColor3 = WHITE,
		BackgroundTransparency = 0.8,
		Position = UDim2.new(0, 0, 1, -1),
		Size = UDim2.new(1, 0, 0, 1),
	}),
})

local settingsTab = new("Frame", {
	Name = "SettingsTab",
	BackgroundTransparency = 1,
	Size = UDim2.fromOffset(160, 60),
	Parent = hubBar,
}, {
	new("Frame", {
		Name = "TabSelection",
		BorderSizePixel = 0,
		BackgroundColor3 = WHITE,
		Position = UDim2.new(0, 3, 1, -2),
		Size = UDim2.new(1, -2, 0, 2),
	}),
})
local tabLogo = logo(22, BLACK)
tabLogo.AnchorPoint = Vector2.new(0, 0.5)
tabLogo.Position = UDim2.new(0, 44, 0.5, 0)
tabLogo.Parent = settingsTab
label({
	Name = "Title",
	Text = "Settings",
	TextSize = 14,
	TextXAlignment = Enum.TextXAlignment.Left,
	Position = UDim2.fromOffset(76, 0),
	Size = UDim2.new(1, -76, 1, 0),
	Parent = settingsTab,
})

local page = new("ScrollingFrame", {
	Name = "Page",
	BackgroundTransparency = 1,
	BorderSizePixel = 0,
	Position = UDim2.fromOffset(20, 72),
	Size = UDim2.new(1, -40, 1, -72 - 80),
	CanvasSize = UDim2.new(),
	AutomaticCanvasSize = Enum.AutomaticSize.Y,
	ScrollBarThickness = 6,
	ScrollBarImageColor3 = WHITE,
	ScrollBarImageTransparency = 0.7,
	ScrollingDirection = Enum.ScrollingDirection.Y,
	Parent = main,
}, {
	new("UIListLayout", { SortOrder = Enum.SortOrder.LayoutOrder }),
})

local order = 0
local function nextOrder()
	order += 1
	return order
end

local function section(title)
	local header = label({
		Name = title .. "Header",
		Text = title,
		FontFace = BOLD,
		TextSize = 20,
		TextXAlignment = Enum.TextXAlignment.Left,
		TextYAlignment = Enum.TextYAlignment.Bottom,
		Size = UDim2.new(1, -23, 0, 44),
		LayoutOrder = nextOrder(),
		Parent = page,
	})
	new("UIPadding", {
		PaddingLeft = UDim.new(0, 10),
		PaddingBottom = UDim.new(0, 8),
		Parent = header,
	})
end

-- A row: its label at the left and `area`, where its control goes.
local function row(title)
	local frame = new("Frame", {
		Name = title .. "Frame",
		BackgroundColor3 = ROW_HOVER,
		BackgroundTransparency = 1,
		Size = UDim2.new(1, -23, 0, 50),
		LayoutOrder = nextOrder(),
		Parent = page,
	}, { corner(UDim.new(0, 8)) })
	label({
		Name = "Label",
		Text = title,
		TextSize = 17,
		TextXAlignment = Enum.TextXAlignment.Left,
		TextTruncate = Enum.TextTruncate.AtEnd,
		Position = UDim2.fromOffset(10, 0),
		Size = UDim2.new(0, 290, 1, 0),
		Parent = frame,
	})
	local area = new("Frame", {
		Name = "Control",
		BackgroundTransparency = 1,
		Position = UDim2.fromOffset(310, 0),
		Size = UDim2.new(1, -311, 1, 0),
		Parent = frame,
	})
	frame.MouseEnter:Connect(function()
		frame.BackgroundTransparency = 0
	end)
	frame.MouseLeave:Connect(function()
		frame.BackgroundTransparency = 1
	end)
	return frame, area
end

local function arrowButton(name, direction, parent)
	local button = new("TextButton", {
		Name = name,
		Text = "",
		AutoButtonColor = false,
		BackgroundTransparency = 1,
		AnchorPoint = Vector2.new(direction == 1 and 1 or 0, 0),
		Position = UDim2.fromScale(direction == 1 and 1 or 0, 0),
		Size = UDim2.new(0, 32, 1, 0),
		Parent = parent,
	})
	chevron(direction).Parent = button
	return button
end

-- A row choosing among `choices` with arrows either side; `get` gives the
-- current choice's index, `set` takes a new one. Returns a function that
-- shows the current choice again.
local function selector(title, choices, get, set)
	local _, area = row(title)
	local value = label({
		Name = "Selection",
		FontFace = REGULAR,
		TextSize = 17,
		Position = UDim2.fromOffset(32, 0),
		Size = UDim2.new(1, -64, 1, 0),
		Parent = area,
	})
	local function show()
		local names = type(choices) == "function" and choices() or choices
		value.Text = names[get()] or ""
	end
	local function step(by)
		local names = type(choices) == "function" and choices() or choices
		local count = #names
		if count == 0 then
			return
		end
		local at = ((get() or 1) - 1 + by) % count + 1
		set(at)
		show()
	end
	arrowButton("LeftButton", -1, area).Activated:Connect(function()
		step(-1)
	end)
	arrowButton("RightButton", 1, area).Activated:Connect(function()
		step(1)
	end)
	local middle = new("TextButton", {
		Name = "Next",
		Text = "",
		BackgroundTransparency = 1,
		Position = UDim2.fromOffset(32, 0),
		Size = UDim2.new(1, -64, 1, 0),
		Parent = area,
	})
	middle.Activated:Connect(function()
		step(1)
	end)
	show()
	return show
end

-- A row with a slider of ten steps; `get` and `set` take 0 to 10. With
-- `typed`, a box at the right end takes any number typed in (`typed.get`,
-- `typed.set`, refusing what's below `typed.min`), as Roblox's Camera
-- Sensitivity row has.
local function slider(title, get, set, typed)
	local _, area = row(title)
	local boxWidth = typed and 68 or 0
	local steps = new("Frame", {
		Name = "Steps",
		BackgroundTransparency = 1,
		Position = UDim2.fromOffset(50, 0),
		Size = UDim2.new(1, -100 - boxWidth, 1, 0),
		Parent = area,
	})
	local box
	if typed then
		box = new("TextBox", {
			Name = "Value",
			ClearTextOnFocus = true,
			FontFace = REGULAR,
			TextSize = 15,
			TextColor3 = WHITE,
			PlaceholderColor3 = Color3.fromRGB(150, 150, 150),
			BackgroundColor3 = WHITE,
			BackgroundTransparency = 0.92,
			AnchorPoint = Vector2.new(1, 0.5),
			Position = UDim2.fromScale(1, 0.5),
			Size = UDim2.fromOffset(60, 34),
			Parent = area,
		}, { corner(UDim.new(0, 8)) })
	end
	local blocks = {}
	local function show()
		if box then
			box.Text = tostring(math.round(typed.get() * 1000) / 1000)
			box.PlaceholderText = box.Text
		end
		local at = get()
		for i, block in blocks do
			local color = i <= at and STEP_ON or STEP_OFF
			block.BackgroundColor3 = color
			for _, filler in block:GetChildren() do
				if filler:IsA("Frame") then
					filler.BackgroundColor3 = color
				end
			end
		end
	end
	local function choose(at)
		set(math.clamp(at, 0, 10))
		show()
	end
	for i = 1, 10 do
		local block = new("TextButton", {
			Name = "Step" .. i,
			Text = "",
			AutoButtonColor = false,
			BorderSizePixel = 0,
			AnchorPoint = Vector2.new(0.5, 0.5),
			Position = UDim2.new((i - 0.5) / 10, 0, 0.5, 0),
			Size = UDim2.new(0.1, -4, 0, 24),
			Parent = steps,
		})
		-- The ends are round on the outside only.
		if i == 1 or i == 10 then
			corner(UDim.new(0, 8)).Parent = block
			new("Frame", {
				Name = "Filler",
				BorderSizePixel = 0,
				Position = UDim2.fromScale(i == 1 and 0.5 or 0, 0),
				Size = UDim2.fromScale(0.5, 1),
				Parent = block,
			})
		end
		block.Activated:Connect(function()
			choose(i)
		end)
		blocks[i] = block
	end
	for _, side in { { "Less", -1 }, { "More", 1 } } do
		local button = new("TextButton", {
			Name = side[1],
			Text = "",
			AutoButtonColor = false,
			BackgroundTransparency = 1,
			AnchorPoint = Vector2.new(side[2] == 1 and 1 or 0, 0),
			Position = side[2] == 1 and UDim2.new(1, -boxWidth, 0, 0) or UDim2.new(),
			Size = UDim2.new(0, 32, 1, 0),
			Parent = area,
		})
		circled(side[2] == 1).Parent = button
		button.Activated:Connect(function()
			choose(get() + side[2])
		end)
	end
	if box then
		box.FocusLost:Connect(function()
			local v = tonumber(box.Text)
			if v and v == v and v >= typed.min and v < math.huge then
				typed.set(v)
			end
			show()
		end)
	end
	show()
	return show
end

-- The settings, each read and written through pcall: a setting this
-- engine lacks shows its default rather than stopping the menu.
local function read(get, default)
	local ok, value = pcall(get)
	if ok and value ~= nil then
		return value
	end
	return default
end

local function write(set)
	local ok, err = pcall(set)
	if not ok then
		warn("Rook menu: " .. tostring(err))
	end
end

section("Audio")

slider("Volume", function()
	return math.round(read(function()
		return GameSettings.MasterVolume
	end, 1) * 10)
end, function(at)
	write(function()
		GameSettings.MasterVolume = at / 10
	end)
end)

-- The output devices as SoundService lists them, the system's default
-- first.
local devices = { names = {}, ids = {}, chosen = 1 }
local function readDevices()
	local ok, names, ids, chosen = pcall(function()
		return SoundService:GetOutputDevices()
	end)
	if not ok or type(names) ~= "table" or #names == 0 then
		devices = { names = { "Default" }, ids = { "" }, chosen = 1 }
		return
	end
	local shown = table.clone(names)
	if ids[1] == "" then
		shown[1] = "Default (" .. names[1] .. ")"
	end
	devices = { names = shown, raw = names, ids = ids, chosen = chosen or 1 }
end
readDevices()
selector("Output Device", function()
	return devices.names
end, function()
	return devices.chosen
end, function(at)
	devices.chosen = at
	write(function()
		SoundService:SetOutputDevice(devices.raw and devices.raw[at] or "", devices.ids[at])
	end)
end)

section("Display & Graphics")

local showFullscreen = selector("Fullscreen", { "Off", "On" }, function()
	return read(function()
		return GameSettings.Fullscreen
	end, false) and 2 or 1
end, function(at)
	local on = read(function()
		return GameSettings.Fullscreen
	end, false)
	if on ~= (at == 2) then
		write(function()
			GuiService:ToggleFullscreen()
		end)
	end
end)
pcall(function()
	GameSettings.FullscreenChanged:Connect(showFullscreen)
end)

-- -1 is the display's rate and 0 none (docs/behavior/user-settings.md).
local rates = { -1, 30, 60, 120, 144, 240, 0 }
local rateNames = { "Match display", "30 FPS", "60 FPS", "120 FPS", "144 FPS", "240 FPS", "Unlimited" }
selector("Maximum Frame Rate", rateNames, function()
	local cap = read(function()
		return GameSettings.FramerateCap
	end, -1)
	return table.find(rates, cap) or table.find(rates, cap < 0 and -1 or cap) or 1
end, function(at)
	write(function()
		GameSettings.FramerateCap = rates[at]
	end)
end)

section("Camera")

-- The slider's steps are 0.4 apart, up to Roblox's 4, and its bottom is
-- 0.1, Roblox's least; any value from 0.01 can be typed in its box,
-- beyond 4 too.
local function sensitivity()
	return read(function()
		return GameSettings.MouseSensitivityThirdPerson.X
	end, 1)
end
local function setSensitivity(v)
	write(function()
		GameSettings.MouseSensitivity = v
		GameSettings.MouseSensitivityFirstPerson = Vector2.new(v, v)
		GameSettings.MouseSensitivityThirdPerson = Vector2.new(v, v)
	end)
end
slider("Camera Sensitivity", function()
	return math.clamp(math.floor(sensitivity() / 0.4 + 1e-6), 0, 10)
end, function(at)
	setSensitivity(at == 0 and 0.1 or at * 0.4)
end, { get = sensitivity, set = setSensitivity, min = 0.01 })

selector("Camera Inverted", { "Off", "On" }, function()
	return read(function()
		return GameSettings.CameraYInverted
	end, false) and 2 or 1
end, function(at)
	write(function()
		GameSettings.CameraYInverted = at == 2
	end)
end)

-- The buttons along the bottom: a key's hint, then the name.
local function keyHint(key, transparency)
	return new("Frame", {
		Name = "Hint",
		BackgroundTransparency = 1,
		Size = UDim2.fromOffset(24, 24),
		LayoutOrder = 1,
	}, {
		corner(UDim.new(0, 4)),
		new("UIStroke", { Color = WHITE, Thickness = 1.5, Transparency = transparency }),
		label({
			Text = key,
			FontFace = MEDIUM,
			TextSize = #key > 1 and 9 or 13,
			TextTransparency = transparency,
			Size = UDim2.fromScale(1, 1),
		}),
	})
end

local function bottomButton(parent, name, key, color, transparency, index, count)
	local gap = 8
	local button = new("TextButton", {
		Name = name .. "Button",
		Text = "",
		AutoButtonColor = false,
		BackgroundColor3 = color,
		BackgroundTransparency = transparency,
		Position = UDim2.new((index - 1) / count, (index - 1) * gap / count, 0, 0),
		Size = UDim2.new(1 / count, -gap * (count - 1) / count, 0, 48),
		Parent = parent,
	}, {
		corner(UDim.new(0, 8)),
		new("UIListLayout", {
			FillDirection = Enum.FillDirection.Horizontal,
			HorizontalAlignment = Enum.HorizontalAlignment.Center,
			VerticalAlignment = Enum.VerticalAlignment.Center,
			SortOrder = Enum.SortOrder.LayoutOrder,
			Padding = UDim.new(0, 14),
		}),
	})
	keyHint(key, 0).Parent = button
	label({
		Name = "Title",
		Text = name,
		FontFace = BOLD,
		TextSize = 17,
		AutomaticSize = Enum.AutomaticSize.X,
		Size = UDim2.fromScale(0, 1),
		LayoutOrder = 2,
		Parent = button,
	})
	button.MouseEnter:Connect(function()
		if button.Active then
			button.BackgroundTransparency = transparency - 0.08
		end
	end)
	button.MouseLeave:Connect(function()
		button.BackgroundTransparency = transparency
	end)
	return button
end

local bottom = new("Frame", {
	Name = "BottomButtons",
	BackgroundTransparency = 1,
	AnchorPoint = Vector2.new(0, 1),
	Position = UDim2.new(0, 20, 1, -20),
	Size = UDim2.new(1, -40, 0, 48),
	Parent = main,
})
local leaveButton = bottomButton(bottom, "Leave", "L", SECONDARY, 0.88, 1, 3)
local respawnButton = bottomButton(bottom, "Respawn", "R", SECONDARY, 0.88, 2, 3)
local resumeButton = bottomButton(bottom, "Resume", "Esc", PRIMARY, 0.6, 3, 3)

-- Respawning isn't offered yet: Rook has no death to respawn from
-- (docs/behavior/gui/menu.md).
respawnButton.Active = false
respawnButton.BackgroundTransparency = 0.94
respawnButton.Title.TextTransparency = 0.6
respawnButton.Hint.UIStroke.Transparency = 0.6
respawnButton.Hint.TextLabel.TextTransparency = 0.6

-- The leave confirmation, in the panel instead of the main view.
local confirm = new("Frame", {
	Name = "LeaveConfirmation",
	Visible = false,
	BackgroundTransparency = 1,
	Size = UDim2.fromScale(1, 1),
	Parent = panel,
})
label({
	Name = "Question",
	Text = "Are you sure you want to leave?",
	FontFace = MEDIUM,
	TextSize = 24,
	TextWrapped = true,
	AnchorPoint = Vector2.new(0.5, 1),
	Position = UDim2.new(0.5, 0, 0.5, -24),
	Size = UDim2.new(1, -80, 0, 60),
	Parent = confirm,
})
local confirmButtons = new("Frame", {
	Name = "Buttons",
	BackgroundTransparency = 1,
	AnchorPoint = Vector2.new(0.5, 0),
	Position = UDim2.new(0.5, 0, 0.5, 0),
	Size = UDim2.new(0, 540, 0, 48),
	Parent = confirm,
})
local stayButton = bottomButton(confirmButtons, "Don't Leave", "Esc", SECONDARY, 0.88, 1, 2)
local confirmLeaveButton = bottomButton(confirmButtons, "Leave", "Enter", PRIMARY, 0.6, 2, 2)

-- Opening and closing.
local isOpen = false
local SINK = "RookMenuSink"

local function showConfirm(on)
	main.Visible = not on
	confirm.Visible = on
end

local function setOpen(open)
	if open == isOpen then
		return
	end
	isOpen = open
	shield.Visible = open
	showConfirm(false)
	if open then
		readDevices()
		-- Keys don't reach the game's controls under the menu.
		pcall(function()
			ContextActionService:BindCoreActionAtPriority(SINK, function()
				return Enum.ContextActionResult.Sink
			end, false, 3000, Enum.KeyCode.W, Enum.KeyCode.A, Enum.KeyCode.S, Enum.KeyCode.D,
				Enum.KeyCode.Space, Enum.KeyCode.Up, Enum.KeyCode.Down, Enum.KeyCode.Left,
				Enum.KeyCode.Right)
		end)
	else
		pcall(function()
			ContextActionService:UnbindCoreAction(SINK)
		end)
	end
	pcall(function()
		GuiService:SetMenuIsOpen(open, "RookMenu")
	end)
end

local function leave()
	write(function()
		game:Shutdown()
	end)
end

menuButton.Activated:Connect(function()
	setOpen(not isOpen)
end)
menuButton.MouseEnter:Connect(function()
	menuButtonHover.BackgroundTransparency = 0.9
end)
menuButton.MouseLeave:Connect(function()
	menuButtonHover.BackgroundTransparency = 1
end)
resumeButton.Activated:Connect(function()
	setOpen(false)
end)
leaveButton.Activated:Connect(function()
	showConfirm(true)
end)
stayButton.Activated:Connect(function()
	showConfirm(false)
end)
confirmLeaveButton.Activated:Connect(leave)

UserInputService.InputBegan:Connect(function(input, processed)
	if input.UserInputType ~= Enum.UserInputType.Keyboard then
		return
	end
	local key = input.KeyCode
	if key == Enum.KeyCode.Escape then
		-- Escape that ended typing in a TextBox is the TextBox's.
		if processed then
			return
		end
		if confirm.Visible then
			showConfirm(false)
		else
			setOpen(not isOpen)
		end
	elseif not isOpen or processed then
		return
	elseif key == Enum.KeyCode.L and not confirm.Visible then
		showConfirm(true)
	elseif key == Enum.KeyCode.Return and confirm.Visible then
		leave()
	end
end)

-- Games hide the top bar with `SetCore("TopbarEnabled", false)`, which
-- hides the button; Escape still opens the menu (creator-docs
-- StarterGui:SetCore).
local StarterGui = game:GetService("StarterGui")
pcall(function()
	StarterGui:RegisterSetCore("TopbarEnabled", function(enabled)
		menuButton.Visible = enabled ~= false
	end)
	StarterGui:RegisterGetCore("TopbarEnabled", function()
		return menuButton.Visible
	end)
end)

gui.Parent = CoreGui
