-- The Settings page: Roblox's sections in Roblox's order, with a line
-- between each, holding the rows Rook acts on. Each row keeps its
-- LayoutOrder and name from Roblox's page (`<name>Frame`), so rows Rook
-- adds later slot in where Roblox has them. Roblox's Chat & Language
-- section has nothing Rook acts on yet, so it isn't shown.

local CorePackages = game:GetService("CorePackages")
local React = require(CorePackages.Packages.React)

local Modules = script.Parent.Parent.Parent
local PlayerSettings = require(Modules.PlayerSettings)
local Components = require(Modules.Components)

local e = React.createElement

local SECTIONS = {
	require(script.Audio),
	require(script.DisplayGraphics),
	require(script.ViewControls),
}

-- A setting's value in a component, read again when GameSettings
-- changes.
local function useSetting<T>(read: () -> T): T
	local value, setValue = React.useState(read)
	React.useEffect(function()
		setValue(read())
		local connection = PlayerSettings.onChanged(function()
			setValue(read())
		end)
		return function()
			if connection then
				connection:Disconnect()
			end
		end
	end, {})
	return value
end

return function(props: { small: boolean })
	local children = {}
	for i, section in SECTIONS do
		local rows = section.rows(props.small, useSetting)
		children[section.header .. "Header"] = e(Components.SectionHeader, {
			title = section.title,
			layoutOrder = section.header,
		})
		for name, row in rows do
			children[name] = row
		end
		if i < #SECTIONS then
			children[section.divider .. "Divider"] = e(Components.SectionHeader, {
				divider = true,
				layoutOrder = section.divider,
			})
		end
	end
	return e(React.Fragment, nil, children)
end
