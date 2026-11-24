-- A wide button with a key's hint before its name, as along the bottom
-- of the menu (BottomButtonFrame's MenuButton<n>): `text`, `hint` (the
-- key's name), `primary` (else secondary), `disabled`, `onActivated`,
-- and the button's `AnchorPoint`, `Position`, `Size` and `LayoutOrder`. Hovered, it
-- shows more: its transparency is 0.92 of it.

local CorePackages = game:GetService("CorePackages")
local React = require(CorePackages.Packages.React)
local Theme = require(script.Parent.Theme)
local Text = require(script.Parent.Text)

local e = React.createElement

return function(props)
	local hovered, setHovered = React.useState(false)
	local B = Theme.button
	local base = if props.primary then B.primaryTransparency else B.secondaryTransparency
	local transparency = if props.disabled then B.disabledTransparency elseif hovered then base * B.hoverFactor else base
	local textTransparency = if props.disabled then B.disabledTextTransparency else 0
	return e("ImageButton", {
		AutoButtonColor = false,
		Active = not props.disabled,
		BorderSizePixel = 0,
		BackgroundColor3 = if props.primary then Theme.color.primary else Theme.color.secondary,
		BackgroundTransparency = transparency,
		AnchorPoint = props.AnchorPoint,
		Position = props.Position,
		Size = props.Size or UDim2.new(1, 0, 0, B.height),
		LayoutOrder = props.LayoutOrder,
		[React.Event.MouseEnter] = function()
			setHovered(true)
		end,
		[React.Event.MouseLeave] = function()
			setHovered(false)
		end,
		[React.Event.Activated] = function()
			if not props.disabled and props.onActivated then
				props.onActivated()
			end
		end,
	}, {
		Corner = e("UICorner", { CornerRadius = UDim.new(0, B.corner) }),
		Layout = e("UIListLayout", {
			FillDirection = Enum.FillDirection.Horizontal,
			HorizontalAlignment = Enum.HorizontalAlignment.Center,
			VerticalAlignment = Enum.VerticalAlignment.Center,
			SortOrder = Enum.SortOrder.LayoutOrder,
			Padding = UDim.new(0, 14),
		}),
		Hint = props.hint and e("Frame", {
			BackgroundTransparency = 1,
			Size = UDim2.fromOffset(24, 24),
			LayoutOrder = 1,
		}, {
			Corner = e("UICorner", { CornerRadius = UDim.new(0, 4) }),
			Stroke = e("UIStroke", { Color = Theme.color.white, Thickness = 1.5, Transparency = textTransparency }),
			Key = e(Text, {
				Text = props.hint,
				TextSize = if #props.hint > 1 then 9 else 13,
				TextTransparency = textTransparency,
				Size = UDim2.fromScale(1, 1),
			}),
		}),
		Title = e(Text, {
			Text = props.text,
			FontFace = Theme.font.bold,
			TextSize = B.textSize,
			TextTransparency = textTransparency,
			AutomaticSize = Enum.AutomaticSize.X,
			Size = UDim2.fromScale(0, 1),
			LayoutOrder = 2,
		}),
	})
end
