-- A built-in icon, tinted: `name` (resources/icons), `size`, and any
-- other ImageLabel props.

local CorePackages = game:GetService("CorePackages")
local React = require(CorePackages.Packages.React)
local Theme = require(script.Parent.Theme)

return function(props)
	local size = props.size or 24
	local merged = {
		BackgroundTransparency = 1,
		Image = Theme.icon(props.name),
		ImageColor3 = Theme.color.white,
		Size = UDim2.fromOffset(size, size),
		AnchorPoint = Vector2.new(0.5, 0.5),
		Position = UDim2.fromScale(0.5, 0.5),
	}
	for k, v in props do
		if k ~= "name" and k ~= "size" then
			merged[k] = v
		end
	end
	return React.createElement("ImageLabel", merged)
end
