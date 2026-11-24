-- View & Controls: the camera's inversion (a selector) and sensitivity
-- (a slider with a box to type it in).

local CorePackages = game:GetService("CorePackages")
local React = require(CorePackages.Packages.React)

local Modules = script.Parent.Parent.Parent.Parent
local Components = require(Modules.Components)
local PlayerSettings = require(Modules.PlayerSettings)

local e = React.createElement

-- The slider's ten steps are 0.4 apart up to 4, and its box shows 0.1 at
-- the bottom (Studio 0.748). What Roblox's box takes beyond that wasn't
-- recorded, so a typed value is kept to the slider's range.
local STEP = 0.4
local LEAST, MOST = 0.1, 4

local function Inverted(props)
	local inverted = props.useSetting(PlayerSettings.cameraInverted)
	return e(Components.Row, { title = "Camera Inverted", layoutOrder = 45, small = props.small }, {
		Selector = e(Components.Selector, {
			choices = { "Off", "On" },
			index = if inverted then 2 else 1,
			onChange = function(at)
				PlayerSettings.setCameraInverted(at == 2)
			end,
		}),
	})
end

local function Sensitivity(props)
	local sensitivity = props.useSetting(PlayerSettings.cameraSensitivity)
	return e(Components.Row, { title = "Camera Sensitivity", layoutOrder = 47, small = props.small }, {
		Slider = e(Components.Slider, {
			value = math.clamp(math.floor(sensitivity / STEP + 1e-6), 0, 10),
			onChange = function(at)
				PlayerSettings.setCameraSensitivity(if at == 0 then LEAST else at * STEP)
			end,
			box = {
				text = tostring(math.round(sensitivity * 1000) / 1000),
				onSubmit = function(text)
					local v = tonumber(text)
					if v and v == v then
						PlayerSettings.setCameraSensitivity(math.clamp(v, LEAST, MOST))
					end
				end,
			},
		}),
	})
end

return {
	title = "View & Controls",
	header = 43,
	rows = function(small, useSetting)
		return {
			["Camera InvertedFrame"] = e(Inverted, { small = small, useSetting = useSetting }),
			["Camera SensitivityFrame"] = e(Sensitivity, { small = small, useSetting = useSetting }),
		}
	end,
}
