-- The People page: everyone in the server, each by display name and
-- @name, the local player first.

local CorePackages = game:GetService("CorePackages")
local Players = game:GetService("Players")
local React = require(CorePackages.Packages.React)

local Modules = script.Parent.Parent.Parent
local Components = require(Modules.Components)
local Theme = Components.Theme

local e = React.createElement

local function usePlayers()
	local list, setList = React.useState(Players:GetPlayers())
	React.useEffect(function()
		local function update()
			setList(Players:GetPlayers())
		end
		update()
		local added = Players.PlayerAdded:Connect(update)
		local removing = Players.PlayerRemoving:Connect(function()
			task.defer(update)
		end)
		return function()
			added:Disconnect()
			removing:Disconnect()
		end
	end, {})
	return list
end

return function(props: { small: boolean })
	local players = usePlayers()
	local me = Players.LocalPlayer
	table.sort(players, function(a, b)
		if a == me or b == me then
			return a == me
		end
		return a.DisplayName:lower() < b.DisplayName:lower()
	end)
	local children = {}
	for i, player in players do
		children["Player" .. player.UserId] = e("Frame", {
			BackgroundTransparency = 1,
			Size = UDim2.new(1, 0, 0, Theme.row.height),
			LayoutOrder = i,
		}, {
			DisplayName = e(Components.Text, {
				Text = player.DisplayName,
				TextSize = if props.small then Theme.row.smallTextSize else Theme.row.textSize,
				TextXAlignment = Enum.TextXAlignment.Left,
				Position = UDim2.fromOffset(10, 4),
				Size = UDim2.new(1, -20, 0.5, -4),
			}),
			Name = e(Components.Text, {
				Text = "@" .. player.Name,
				FontFace = Theme.font.regular,
				TextSize = if props.small then Theme.row.smallTextSize else 14,
				TextTransparency = 0.3,
				TextXAlignment = Enum.TextXAlignment.Left,
				Position = UDim2.new(0, 10, 0.5, 0),
				Size = UDim2.new(1, -20, 0.5, -4),
			}),
		})
	end
	return e(React.Fragment, nil, children)
end
