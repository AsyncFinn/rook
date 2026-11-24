-- An icon: a glyph of Roblox's Builder Icons font, which Roblox's own
-- CoreScripts draw their icons with (a TextLabel whose text is the
-- glyph's name, such as "gear"). The font comes from the player's install
-- or Rook's downloaded content packages; nothing of it ships with Rook.
-- Props: `name` (the glyph), `size`, and any other TextLabel props.

local CorePackages = game:GetService("CorePackages")
local React = require(CorePackages.Packages.React)
local Theme = require(script.Parent.Theme)

return function(props)
	local size = props.size or 24
	local merged = {
		BackgroundTransparency = 1,
		FontFace = Theme.font.icons,
		Text = props.name,
		TextColor3 = Theme.color.white,
		TextScaled = true,
		Size = UDim2.fromOffset(size, size),
		AnchorPoint = Vector2.new(0.5, 0.5),
		Position = UDim2.fromScale(0.5, 0.5),
	}
	for k, v in props do
		if k ~= "name" and k ~= "size" then
			merged[k] = v
		end
	end
	return React.createElement("TextLabel", merged)
end
