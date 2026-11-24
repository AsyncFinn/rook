-- The menu's tabs (HubBar), each an icon and a name, the selected one
-- underlined and the others faded: `tabs` ({ name, icon, id }), `selected`
-- (a name), `onSelect(name)`. Each tab takes a fifth of the bar, as
-- Roblox's five do. In a small window the icon shrinks and sits above
-- the name.

local CorePackages = game:GetService("CorePackages")
local React = require(CorePackages.Packages.React)
local Theme = require(script.Parent.Theme)
local Text = require(script.Parent.Text)
local Icon = require(script.Parent.Icon)

local e = React.createElement

type Tab = { name: string, icon: string, id: string }

return function(props: { tabs: { Tab }, selected: string, onSelect: (string) -> (), small: boolean? })
	local T = Theme.tab
	local tabs = {
		UIListLayout = e("UIListLayout", {
			FillDirection = Enum.FillDirection.Horizontal,
			HorizontalAlignment = Enum.HorizontalAlignment.Center,
			SortOrder = Enum.SortOrder.LayoutOrder,
		}),
	}
	for i, tab in props.tabs do
		local selected = tab.name == props.selected
		local fade = if selected then 0 else T.unselectedTransparency
		tabs[tab.id] = e("TextButton", {
			Text = "",
			AutoButtonColor = false,
			BackgroundTransparency = 1,
			Size = UDim2.fromScale(0.2, 1),
			LayoutOrder = i,
			ZIndex = 3,
			[React.Event.Activated] = function()
				props.onSelect(tab.name)
			end,
		}, {
			TabLabel = e("Frame", {
				BackgroundTransparency = 1,
				Size = UDim2.fromScale(1, 1),
			}, {
				Layout = e("UIListLayout", {
					FillDirection = if props.small then Enum.FillDirection.Vertical else Enum.FillDirection.Horizontal,
					HorizontalAlignment = Enum.HorizontalAlignment.Center,
					VerticalAlignment = Enum.VerticalAlignment.Center,
					SortOrder = Enum.SortOrder.LayoutOrder,
					Padding = UDim.new(0, if props.small then 0 else T.gap),
				}),
				Icon = e(Icon, {
					name = tab.icon,
					size = if props.small then T.smallIconSize else T.iconSize,
					AnchorPoint = Vector2.zero,
					Position = UDim2.new(),
					ImageTransparency = fade,
					LayoutOrder = 1,
				}),
				Title = e(Text, {
					Text = tab.name,
					TextSize = T.textSize,
					TextTransparency = fade,
					AutomaticSize = Enum.AutomaticSize.XY,
					Size = UDim2.new(),
					LayoutOrder = 2,
				}),
			}),
			TabSelection = e("Frame", {
				Visible = selected,
				BorderSizePixel = 0,
				BackgroundColor3 = Theme.color.white,
				Position = UDim2.new(0, 3, 1, -T.underline),
				Size = UDim2.new(1, -2, 0, T.underline),
			}),
		})
	end
	return e(React.Fragment, nil, {
		HubBarContainer = e("Frame", { BackgroundTransparency = 1, Size = UDim2.fromScale(1, 1) }, tabs),
		Frame = e("Frame", {
			BorderSizePixel = 0,
			BackgroundColor3 = Theme.color.white,
			BackgroundTransparency = T.lineTransparency,
			AnchorPoint = Vector2.new(0, 1),
			Position = UDim2.fromScale(0, 1),
			Size = UDim2.new(1, 0, 0, 1),
		}),
	})
end
