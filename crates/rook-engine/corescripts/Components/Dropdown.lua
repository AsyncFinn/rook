-- A box showing the current choice (DropDownFrameButton); clicking it
-- lists `choices` under it, and clicking one chooses it. `index`;
-- `onChange(index)`.

local CorePackages = game:GetService("CorePackages")
local React = require(CorePackages.Packages.React)
local Theme = require(script.Parent.Theme)
local Text = require(script.Parent.Text)
local Icon = require(script.Parent.Icon)

local e = React.createElement

return function(props: { choices: { string }, index: number, onChange: (number) -> () })
	local open, setOpen = React.useState(false)
	local item = Theme.dropdown.itemHeight
	local list = nil
	if open then
		local items = {
			Layout = e("UIListLayout", { SortOrder = Enum.SortOrder.LayoutOrder }),
		}
		for i, name in props.choices do
			items["Selection" .. i] = e("TextButton", {
				Text = "",
				AutoButtonColor = false,
				BackgroundColor3 = Theme.color.white,
				BackgroundTransparency = if i == props.index then 0.9 else 1,
				Size = UDim2.new(1, 0, 0, item),
				LayoutOrder = i,
				ZIndex = 10,
				[React.Event.Activated] = function()
					setOpen(false)
					props.onChange(i)
				end,
			}, {
				Label = e(Text, {
					Text = name,
					TextSize = Theme.row.textSize,
					TextXAlignment = Enum.TextXAlignment.Left,
					Position = UDim2.fromOffset(Theme.dropdown.textInset, 0),
					Size = UDim2.new(1, -2 * Theme.dropdown.textInset, 1, 0),
					ZIndex = 10,
				}),
			})
		end
		list = e("Frame", {
			BackgroundColor3 = Theme.color.row,
			Position = UDim2.new(0, 0, 1, 4),
			Size = UDim2.new(1, 0, 0, item * #props.choices),
			ZIndex = 10,
		}, items)
	end
	return e("ImageButton", {
		AutoButtonColor = false,
		BackgroundColor3 = Theme.color.row,
		AnchorPoint = Vector2.new(1, 0.5),
		Position = UDim2.fromScale(1, 0.5),
		Size = UDim2.new(1, 0, 0, Theme.dropdown.height),
		ZIndex = if open then 10 else 2,
		[React.Event.Activated] = function()
			setOpen(not open)
		end,
	}, {
		Corner = e("UICorner", { CornerRadius = UDim.new(0, Theme.row.corner) }),
		Border = e("UIStroke", {
			Color = Theme.color.white,
			Transparency = Theme.dropdown.strokeTransparency,
		}),
		DropDownFrameTextLabel = e(Text, {
			Text = props.choices[props.index] or "",
			TextSize = Theme.row.textSize,
			TextTransparency = Theme.dropdown.textTransparency,
			TextXAlignment = Enum.TextXAlignment.Left,
			ClipsDescendants = true,
			Position = UDim2.fromOffset(Theme.dropdown.textInset, 0),
			Size = UDim2.new(1, -50, 1, 0),
		}),
		DropDownImage = e(Icon, {
			name = "chevron-down",
			size = 15,
			AnchorPoint = Vector2.new(1, 0.5),
			Position = UDim2.new(1, -Theme.dropdown.arrowInset, 0.5, 0),
		}),
		List = list,
	})
end
