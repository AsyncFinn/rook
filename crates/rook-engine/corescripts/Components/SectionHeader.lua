-- A settings section's title (HeaderWrapper), and the line between two
-- sections (DividerWrapper): `divider = true`.

local CorePackages = game:GetService("CorePackages")
local React = require(CorePackages.Packages.React)
local Theme = require(script.Parent.Theme)
local Text = require(script.Parent.Text)

local e = React.createElement

return function(props: { title: string?, divider: boolean?, layoutOrder: number })
	if props.divider then
		return e("Frame", {
			BackgroundTransparency = 1,
			AutomaticSize = Enum.AutomaticSize.Y,
			Size = UDim2.fromScale(1, 0),
			LayoutOrder = props.layoutOrder,
		}, {
			Padding = e("UIPadding", {
				PaddingTop = UDim.new(0, Theme.section.dividerAbove),
				PaddingBottom = UDim.new(0, Theme.section.dividerBelow),
				PaddingLeft = UDim.new(0, Theme.section.inset),
			}),
			DividerStroke = e("Frame", {
				BorderSizePixel = 0,
				BackgroundColor3 = Theme.color.secondary,
				BackgroundTransparency = Theme.section.dividerTransparency,
				Size = UDim2.new(1, 0, 0, 1),
			}),
		})
	end
	return e(Text, {
		Text = props.title,
		FontFace = Theme.font.bold,
		TextSize = Theme.section.headerTextSize,
		TextXAlignment = Enum.TextXAlignment.Left,
		TextYAlignment = Enum.TextYAlignment.Top,
		AutomaticSize = Enum.AutomaticSize.Y,
		Size = UDim2.fromScale(1, 0),
		LayoutOrder = props.layoutOrder,
		children = {
			Padding = e("UIPadding", {
				PaddingLeft = UDim.new(0, Theme.section.inset),
				PaddingBottom = UDim.new(0, Theme.section.headerPaddingBottom),
			}),
		},
	})
end
