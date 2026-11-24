-- Audio: the volume (a slider) and the output device (a selector).

local CorePackages = game:GetService("CorePackages")
local React = require(CorePackages.Packages.React)

local Modules = script.Parent.Parent.Parent.Parent
local Components = require(Modules.Components)
local PlayerSettings = require(Modules.PlayerSettings)

local e = React.createElement

local function OutputDevice(props: { small: boolean })
	-- Listed when the page is shown.
	local devices, setDevices = React.useState(PlayerSettings.outputDevices)
	return e(Components.Row, { title = "Output Device", layoutOrder = 7, small = props.small }, {
		Selector = e(Components.Selector, {
			choices = devices.names,
			index = devices.chosen,
			onChange = function(at)
				PlayerSettings.setOutputDevice(devices, at)
				local chosen = table.clone(devices)
				chosen.chosen = at
				setDevices(chosen)
			end,
		}),
	})
end

local function Volume(props: { small: boolean, useSetting: any })
	local volume = props.useSetting(PlayerSettings.volume)
	return e(Components.Row, { title = "Volume", layoutOrder = 3, small = props.small }, {
		Slider = e(Components.Slider, {
			value = math.round(volume * 10),
			onChange = function(at)
				PlayerSettings.setVolume(at / 10)
			end,
		}),
	})
end

return {
	title = "Audio",
	header = 2,
	divider = 8,
	rows = function(small, useSetting)
		return {
			VolumeFrame = e(Volume, { small = small, useSetting = useSetting }),
			["Output DeviceFrame"] = e(OutputDevice, { small = small }),
		}
	end,
}
