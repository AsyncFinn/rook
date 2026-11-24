-- The top bar's GUI.

local CorePackages = game:GetService("CorePackages")
local React = require(CorePackages.Packages.React)

local MenuButton = require(script.Parent.MenuButton)

return function(props: { useEnabled: () -> boolean })
	local enabled = props.useEnabled()
	return React.createElement(MenuButton, { visible = enabled })
end
