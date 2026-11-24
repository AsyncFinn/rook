-- A settings row: its name (and a description under it, if it has one)
-- at the left, its control (the children) at the right. The row the
-- mouse last entered is lit; leaving it for anything but another row
-- leaves it lit, as Roblox's are.

local CorePackages = game:GetService("CorePackages")
local React = require(CorePackages.Packages.React)
local Theme = require(script.Parent.Theme)
local Text = require(script.Parent.Text)

local e = React.createElement

-- The lit row, shared by every row.
local lit = nil
local listeners = {}
local function light(id)
	lit = id
	for f in listeners do
		f(id)
	end
end

type Props = {
	title: string,
	description: string?,
	layoutOrder: number,
	small: boolean?,
	children: any,
}

return function(props: Props)
	local id = React.useRef({}).current
	local isLit, setLit = React.useState(lit == id)
	React.useEffect(function()
		local function f(now)
			setLit(now == id)
		end
		listeners[f] = true
		return function()
			listeners[f] = nil
		end
	end, {})

	local textSize = if props.small then Theme.row.smallTextSize else Theme.row.textSize
	local labelSize = UDim2.new(Theme.row.labelShare, -2 * Theme.row.labelInset, 0, Theme.row.height)
	local label
	if props.description then
		label = e("Frame", {
			BackgroundTransparency = 1,
			AutomaticSize = Enum.AutomaticSize.Y,
			Position = UDim2.fromOffset(Theme.row.labelInset, 0),
			Size = labelSize,
		}, {
			Layout = e("UIListLayout", {
				SortOrder = Enum.SortOrder.LayoutOrder,
				VerticalAlignment = Enum.VerticalAlignment.Center,
				Padding = UDim.new(0, Theme.row.descriptionGap),
			}),
			Padding = e("UIPadding", {
				PaddingTop = UDim.new(0, Theme.row.labelPadding),
				PaddingBottom = UDim.new(0, Theme.row.labelPadding),
			}),
			Label = e(Text, {
				Text = props.title,
				TextSize = textSize,
				TextWrapped = true,
				TextXAlignment = Enum.TextXAlignment.Left,
				AutomaticSize = Enum.AutomaticSize.Y,
				Size = UDim2.fromScale(1, 0),
				LayoutOrder = 1,
			}),
			Description = e(Text, {
				Text = props.description,
				TextSize = Theme.row.descriptionSize,
				TextTransparency = Theme.row.descriptionTransparency,
				TextWrapped = true,
				TextXAlignment = Enum.TextXAlignment.Left,
				AutomaticSize = Enum.AutomaticSize.Y,
				Size = UDim2.fromScale(1, 0),
				LayoutOrder = 2,
			}),
		})
	else
		label = e(Text, {
			Text = props.title,
			TextSize = textSize,
			TextWrapped = true,
			TextXAlignment = Enum.TextXAlignment.Left,
			AutomaticSize = Enum.AutomaticSize.Y,
			Position = UDim2.fromOffset(Theme.row.labelInset, 0),
			Size = UDim2.new(Theme.row.labelShare, -2 * Theme.row.labelInset, 1, 0),
			children = {
				Padding = e("UIPadding", {
					PaddingTop = UDim.new(0, Theme.row.labelPadding),
					PaddingBottom = UDim.new(0, Theme.row.labelPadding),
				}),
			},
		})
	end

	return e("ImageButton", {
		AutoButtonColor = false,
		Active = false,
		BorderSizePixel = 0,
		BackgroundColor3 = Theme.color.row,
		BackgroundTransparency = if isLit then 0 else 1,
		AutomaticSize = Enum.AutomaticSize.Y,
		Size = UDim2.new(1, 0, 0, Theme.row.height),
		LayoutOrder = props.layoutOrder,
		[React.Event.MouseEnter] = function()
			light(id)
		end,
	}, {
		Corner = e("UICorner", { CornerRadius = UDim.new(0, Theme.row.corner) }),
		Label = label,
		Control = e("Frame", {
			BackgroundTransparency = 1,
			AnchorPoint = Vector2.new(1, 0.5),
			Position = UDim2.fromScale(1, 0.5),
			Size = UDim2.new(Theme.row.controlShare, 0, 0, Theme.row.height),
		}, props.children),
	})
end
