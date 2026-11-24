-- Choosing among `choices` with arrows either side (LeftButton,
-- RightButton, 32 by 50); clicking between them steps forward too
-- (AutoSelectButton). `index` is the current choice; `onChange(index)`.

local CorePackages = game:GetService("CorePackages")
local React = require(CorePackages.Packages.React)
local Theme = require(script.Parent.Theme)
local Text = require(script.Parent.Text)
local Icon = require(script.Parent.Icon)

local e = React.createElement

return function(props: { choices: { string }, index: number, onChange: (number) -> () })
	local count = #props.choices
	local width = Theme.slider.buttonWidth
	local function step(by)
		if count > 0 then
			props.onChange(((props.index or 1) - 1 + by) % count + 1)
		end
	end
	local function arrow(icon, direction)
		return e("ImageButton", {
			AutoButtonColor = false,
			BackgroundTransparency = 1,
			AnchorPoint = Vector2.new(if direction == 1 then 1 else 0, 0.5),
			Position = UDim2.fromScale(if direction == 1 then 1 else 0, 0.5),
			Size = UDim2.new(0, width, 0, Theme.row.height),
			ZIndex = 3,
			[React.Event.Activated] = function()
				step(direction)
			end,
		}, {
			Icon = e(Icon, { name = icon, size = 24, TextColor3 = Theme.color.arrow }),
		})
	end
	return e("Frame", {
		BackgroundTransparency = 1,
		Size = UDim2.fromScale(1, 1),
	}, {
		LeftButton = arrow("chevron-large-left", -1),
		RightButton = arrow("chevron-large-right", 1),
		Selection = e(Text, {
			Text = props.choices[props.index] or "",
			FontFace = Theme.font.regular,
			TextSize = Theme.row.textSize,
			TextWrapped = true,
			Position = UDim2.fromOffset(width, 0),
			Size = UDim2.new(1, -2 * width, 1, 0),
		}),
		AutoSelectButton = e("ImageButton", {
			AutoButtonColor = false,
			BackgroundTransparency = 1,
			Position = UDim2.fromOffset(width, 0),
			Size = UDim2.new(1, -2 * width, 1, 0),
			[React.Event.Activated] = function()
				step(1)
			end,
		}),
	})
end
