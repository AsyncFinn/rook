-- The round button at the top left (Roblox's IconHitArea), with Rook's
-- logo where Roblox has its own: it opens and closes the menu. Hovered,
-- an overlay lightens it; the logo grows while the menu is open.

local CorePackages = game:GetService("CorePackages")
local React = require(CorePackages.Packages.React)

local Modules = script.Parent.Parent
local Theme = require(Modules.Components.Theme)
local Menu = require(Modules.Menu)

local e = React.createElement

return function(props: { visible: boolean })
	local hovered, setHovered = React.useState(false)
	local menu = Menu.useState()
	local T = Theme.topBar
	local logo = if menu.open then T.logoOpenSize else T.logoSize
	return e("ImageButton", {
		AutoButtonColor = false,
		Visible = props.visible,
		BackgroundColor3 = Theme.color.button,
		BackgroundTransparency = T.buttonTransparency,
		Position = T.buttonPosition,
		Size = UDim2.fromOffset(T.buttonSize, T.buttonSize),
		[React.Event.Activated] = function()
			Menu.toggle()
		end,
		[React.Event.MouseEnter] = function()
			setHovered(true)
		end,
		[React.Event.MouseLeave] = function()
			setHovered(false)
		end,
	}, {
		UICorner = e("UICorner", { CornerRadius = UDim.new(1, 0) }),
		ScalingIcon = e("ImageLabel", {
			BackgroundTransparency = 1,
			Image = Theme.image("logo-mark"),
			AnchorPoint = Vector2.new(0.5, 0.5),
			Position = UDim2.fromScale(0.5, 0.5),
			Size = UDim2.fromOffset(logo, logo),
		}),
		StateOverlayRound = e("Frame", {
			BackgroundColor3 = Theme.color.secondary,
			BackgroundTransparency = if hovered then T.hoverTransparency else 1,
			Size = UDim2.fromScale(1, 1),
			ZIndex = 2,
		}, {
			UICorner = e("UICorner", { CornerRadius = UDim.new(1, 0) }),
		}),
	})
end
