-- A TextLabel with the theme's font and color and no background.

local CorePackages = game:GetService("CorePackages")
local React = require(CorePackages.Packages.React)
local Theme = require(script.Parent.Theme)

return function(props)
	local merged = {
		BackgroundTransparency = 1,
		TextColor3 = Theme.color.white,
		FontFace = Theme.font.medium,
	}
	local children = props.children
	for k, v in props do
		if k ~= "children" then
			merged[k] = v
		end
	end
	return React.createElement("TextLabel", merged, children)
end
