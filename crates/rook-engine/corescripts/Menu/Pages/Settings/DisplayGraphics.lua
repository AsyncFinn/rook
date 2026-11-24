-- Display & Graphics: fullscreen (a selector) and the maximum frame rate
-- (a dropdown, with a description under its name).

local CorePackages = game:GetService("CorePackages")
local React = require(CorePackages.Packages.React)

local Modules = script.Parent.Parent.Parent.Parent
local Components = require(Modules.Components)
local PlayerSettings = require(Modules.PlayerSettings)

local e = React.createElement

-- Roblox's choices: its default, then fixed rates (Studio 0.748 on a
-- 240 Hz display listed these, each up to the display's rate). -1 is
-- Rook's default, the display's rate (docs/behavior/user-settings.md).
local RATES = { -1, 60, 120, 144, 160, 165, 180, 200, 240 }
local RATE_NAMES = { "Default", "60 FPS", "120 FPS", "144 FPS", "160 FPS", "165 FPS", "180 FPS", "200 FPS", "240 FPS" }

local function Fullscreen(props)
	local fullscreen = props.useSetting(PlayerSettings.fullscreen)
	return e(Components.Row, { title = "Fullscreen", layoutOrder = 26, small = props.small }, {
		Selector = e(Components.Selector, {
			choices = { "Off", "On" },
			index = if fullscreen then 2 else 1,
			onChange = function(at)
				PlayerSettings.setFullscreen(at == 2)
			end,
		}),
	})
end

local function FrameRate(props)
	local cap = props.useSetting(PlayerSettings.framerateCap)
	local rate = table.find(RATES, cap) or 1
	return e(Components.Row, {
		title = "Maximum Frame Rate",
		description = "Caps how many frames are drawn each second. Higher rates use more power, and a display shows no more than its own refresh rate.",
		layoutOrder = 40,
		small = props.small,
	}, {
		DropDownFrameButton = e(Components.Dropdown, {
			choices = RATE_NAMES,
			index = rate,
			onChange = function(at)
				PlayerSettings.setFramerateCap(RATES[at])
			end,
		}),
	})
end

return {
	title = "Display & Graphics",
	header = 24,
	divider = 42,
	rows = function(small, useSetting)
		return {
			FullscreenFrame = e(Fullscreen, { small = small, useSetting = useSetting }),
			["Maximum Frame RateFrame"] = e(FrameRate, { small = small, useSetting = useSetting }),
		}
	end,
}
