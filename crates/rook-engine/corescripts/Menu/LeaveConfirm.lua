-- Asking before leaving, in the panel in place of the pages: Escape or
-- Don't Leave goes back, Enter or Leave leaves.

local CorePackages = game:GetService("CorePackages")
local React = require(CorePackages.Packages.React)

local Modules = script.Parent.Parent
local Components = require(Modules.Components)
local Theme = Components.Theme
local Menu = require(script.Parent)

local e = React.createElement

return function(props: { small: boolean })
	local width = if props.small then 1 else 0
	return e("Frame", {
		BackgroundTransparency = 1,
		Size = UDim2.fromScale(1, 1),
	}, {
		Question = e(Components.Text, {
			Text = "Are you sure you want to leave?",
			TextSize = if props.small then 18 else 24,
			TextWrapped = true,
			AnchorPoint = Vector2.new(0.5, 1),
			Position = UDim2.new(0.5, 0, 0.5, -24),
			Size = UDim2.new(1, -80, 0, 60),
		}),
		Buttons = e("Frame", {
			BackgroundTransparency = 1,
			AnchorPoint = Vector2.new(0.5, 0),
			Position = UDim2.new(0.5, 0, 0.5, 0),
			Size = UDim2.new(width, if props.small then -2 * Theme.menu.margin else 540, 0, Theme.button.height),
		}, {
			StayButton = e(Components.Button, {
				text = "Don't Leave",
				hint = "Esc",
				Size = UDim2.new(0.5, -Theme.button.gap / 2, 1, 0),
				onActivated = function()
					Menu.setLeaving(false)
				end,
			}),
			LeaveButton = e(Components.Button, {
				text = "Leave",
				hint = "Enter",
				primary = true,
				Position = UDim2.new(0.5, Theme.button.gap / 2, 0, 0),
				Size = UDim2.new(0.5, -Theme.button.gap / 2, 1, 0),
				onActivated = Menu.leave,
			}),
		}),
	})
end
