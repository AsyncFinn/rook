-- The Help page: the keyboard and mouse controls a character has with
-- Roblox's PlayerModule (docs/behavior/characters/camera.md and
-- movement.md).

local CorePackages = game:GetService("CorePackages")
local React = require(CorePackages.Packages.React)

local Modules = script.Parent.Parent.Parent
local Components = require(Modules.Components)
local Theme = Components.Theme

local e = React.createElement

local CONTROLS = {
	{ "Move", "W A S D or the arrow keys" },
	{ "Jump", "Space" },
	{ "Turn the camera", "Right mouse button and drag" },
	{ "Zoom", "Mouse wheel, I and O" },
	{ "Menu", "Esc" },
}

return function(props: { small: boolean })
	local size = if props.small then Theme.row.smallTextSize else Theme.row.textSize
	local children = {
		Header = e(Components.SectionHeader, { title = "Controls", layoutOrder = 0, small = props.small }),
	}
	for i, control in CONTROLS do
		children["Control" .. i] = e("Frame", {
			BackgroundTransparency = 1,
			Size = UDim2.new(1, 0, 0, Theme.row.height),
			LayoutOrder = i,
		}, {
			Action = e(Components.Text, {
				Text = control[1],
				TextSize = size,
				TextXAlignment = Enum.TextXAlignment.Left,
				Position = UDim2.fromOffset(10, 0),
				Size = UDim2.new(0.45, -10, 1, 0),
			}),
			Keys = e(Components.Text, {
				Text = control[2],
				FontFace = Theme.font.regular,
				TextSize = size,
				TextXAlignment = Enum.TextXAlignment.Left,
				Position = UDim2.fromScale(0.45, 0),
				Size = UDim2.new(0.55, -10, 1, 0),
			}),
		})
	end
	return e(React.Fragment, nil, children)
end
