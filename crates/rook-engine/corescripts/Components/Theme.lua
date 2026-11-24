-- Every size, color and font Rook's CoreScripts draw with, measured on
-- Roblox's menu in Studio 0.748 at 1760 by 990 and at 630 by 990
-- (docs/behavior/gui/menu/studio-0.748.json; the names below are the
-- instances each value was read from). Values marked Inference weren't
-- readable there: docs/behavior/gui/menu.md says why.

local FONT = "rbxasset://fonts/families/BuilderSans.json"

local Theme = {}

Theme.font = {
	regular = Font.new(FONT, Enum.FontWeight.Regular),
	medium = Font.new(FONT, Enum.FontWeight.Medium),
	bold = Font.new(FONT, Enum.FontWeight.Bold),
}

Theme.color = {
	white = Color3.new(1, 1, 1),
	black = Color3.new(0, 0, 0),
	-- IconHitArea.
	button = Color3.fromRGB(18, 18, 21),
	-- IconHitArea.StateOverlayRound while hovered; the bottom buttons'
	-- MenuButton (Leave, Respawn); DividerStroke.
	secondary = Color3.fromRGB(208, 217, 251),
	-- The Resume MenuButton.
	primary = Color3.fromRGB(51, 95, 255),
	-- A settings row (<name>Frame) and DropDownFrameButton.
	row = Color3.fromRGB(35, 37, 39),
	-- Slider steps (StepsContainer.Step<n>), lit and unlit.
	stepOn = Color3.fromRGB(217, 217, 217),
	stepOff = Color3.fromRGB(57, 59, 61),
	-- Selector and slider arrows (LeftButton, RightButton images).
	arrow = Color3.fromRGB(204, 204, 204),
	-- CameraSensitivityTextBox.
	field = Color3.fromRGB(51, 51, 51),
}

-- TopBarApp's IconHitArea, at (16, 12) on the screen.
Theme.topBar = {
	buttonPosition = UDim2.fromOffset(16, 12),
	buttonSize = 44,
	buttonTransparency = 0.016,
	hoverTransparency = 0.92,
	-- ScalingIcon: 24 closed, 30 while the menu is open. It springs
	-- between them, critically damped at this many cycles a second.
	logoSize = 24,
	logoOpenSize = 30,
	logoFrequency = 1 / 0.35,
}

Theme.menu = {
	-- DarkenBackground.
	backdropTransparency = 0.5,
	-- MenuContainer: 840 by 734, 10 below the middle, corner 10.
	panelTransparency = 0.06,
	panelSize = Vector2.new(840, 734),
	panelDrop = 10,
	panelCorner = 10,
	-- MenuContainer.Page.UIPadding.
	padding = 20,
	paddingBottom = 14,
	-- HubBar; HubBar at 630 wide is 54 tall, 10 down.
	tabBarHeight = 60,
	smallTabBarHeight = 54,
	smallTabBarTop = 10,
	-- PageViewClipper (800 by 600) and PageView in it (20 shorter, 2
	-- down), PageViewInnerFrame.UIPadding (5 at the top) and
	-- PageViewInnerFrame.Page.UIPadding (12 left, 11 right).
	pageHeight = 600,
	pageInset = 20,
	pageTopPadding = 5,
	pageLeftPadding = 12,
	pageRightPadding = 11,
	scrollBarThickness = 1,
	-- BottomButtonFrame: 60 tall (54 at 630 wide).
	bottomHeight = 60,
	smallBottomHeight = 54,
	-- Inference: the panel spans the window once it's narrower than the
	-- panel and its padding (measured full width at 630, not at 840).
	smallWidth = 880,
	-- At 630 by 990 the page is 724 tall: the window's height less 266.
	smallPageInset = 266,
	-- SettingsShield slides up from a screen and 36 below while
	-- DarkenBackground fades in, and slides back down to close.
	openSeconds = 0.5,
	closeSeconds = 0.4,
	offscreen = UDim2.new(0, 0, 1, 36),
}

-- HubBarContainer's tabs (PeopleTab, GameSettingsTab, ...).
Theme.tab = {
	-- TabLabel.Icon, then TabLabel.Title 10 after it.
	iconSize = 24,
	smallIconSize = 14,
	gap = 10,
	textSize = 19,
	-- An unselected tab's icon and title.
	unselectedTransparency = 0.5,
	-- TabSelection.
	underline = 2,
	-- HubBar.Frame, the line under the tabs.
	lineTransparency = 0.8,
}

-- A settings row: <name>Frame, <name>Label, the control.
Theme.row = {
	height = 50,
	corner = 8,
	-- <name>Label: 10 in, 0.4 of the row less 20; the control takes 0.6
	-- at the right.
	labelInset = 10,
	labelShare = 0.4,
	labelPadding = 10,
	controlShare = 0.6,
	textSize = 17,
	smallTextSize = 12,
	-- <name>Description, 8 under the name.
	descriptionSize = 12,
	descriptionTransparency = 0.25,
	descriptionGap = 8,
}

-- HeaderWrapper (33 tall, its text 25) and DividerWrapper (a 1-pixel
-- line, 4 above and 16 below), both 10 in. Inference: the header's font
-- and size, which a style sheet sets (TextBounds 25 tall).
Theme.section = {
	headerTextSize = 20,
	headerPaddingBottom = 8,
	inset = 10,
	dividerAbove = 4,
	dividerBelow = 16,
	dividerTransparency = 0.88,
}

-- Slider: 32 by 50 buttons with 30-pixel images either end, steps in
-- the rest less 100, each 0.1 of it less 4 and 24 tall.
Theme.slider = {
	buttonWidth = 32,
	iconSize = 30,
	stepHeight = 24,
	stepGap = 4,
	-- Camera Sensitivity's slider ends 80 short of the row, its box 10
	-- past it: CameraSensitivityTextBox, 60 wide, 0.8 of the slider's 44.
	boxWidth = 60,
	boxGap = 10,
	boxTextSize = 14,
	boxHeight = 44,
}

-- DropDownFrameButton: 40 tall, outlined white at 0.8, its text 15 in at
-- 0.3, DropDownImage 15 by 10 at 12 from the right; the list's
-- Selection<n> are 50 tall.
Theme.dropdown = {
	height = 40,
	strokeTransparency = 0.8,
	textInset = 15,
	textTransparency = 0.3,
	arrowInset = 12,
	itemHeight = 50,
}

-- BottomButtonFrame's MenuButton<n>: 48 tall, 8 apart; hovered, their
-- transparency is 0.92 of it.
Theme.button = {
	height = 48,
	corner = 8,
	gap = 8,
	textSize = 17,
	secondaryTransparency = 0.88,
	primaryTransparency = 0.6,
	hoverFactor = 0.92,
	-- Rook's: Respawn, which Rook can't do yet, shows faded.
	disabledTransparency = 0.94,
	disabledTextTransparency = 0.6,
}

-- A built-in image (crates/rook-content/build.rs).
function Theme.image(name: string): string
	return "rbxasset://rook/" .. name .. ".png"
end

function Theme.icon(name: string): string
	return Theme.image("icons/" .. name)
end

return Theme
