-- The top bar: for now its one button, which opens the menu. Games hide
-- it with SetCore("TopbarEnabled", false) (creator-docs
-- StarterGui:SetCore); Escape still opens the menu then.

local CoreGui = game:GetService("CoreGui")
local CorePackages = game:GetService("CorePackages")

local React = require(CorePackages.Packages.React)
local ReactRoblox = require(CorePackages.Packages.ReactRoblox)

local Modules = script.Parent
local CoreApi = require(Modules.CoreApi)
local TopBarApp = require(script.TopBarApp)

local TopBar = {}

local enabled = true
local listeners = {}

-- Whether the top bar shows, in a component.
function TopBar.useEnabled(): boolean
	local current, set = React.useState(enabled)
	React.useEffect(function()
		set(enabled)
		listeners[set] = true
		return function()
			listeners[set] = nil
		end
	end, {})
	return current
end

function TopBar.start()
	CoreApi.register("TopBar", "TopbarEnabled", {
		set = function(on)
			enabled = on ~= false
			for set in listeners do
				set(enabled)
			end
		end,
		get = function()
			return enabled
		end,
	})
	local gui = Instance.new("ScreenGui")
	gui.Name = "TopBarApp"
	gui.ResetOnSpawn = false
	gui.IgnoreGuiInset = true
	gui.ZIndexBehavior = Enum.ZIndexBehavior.Sibling
	-- Over the menu, whose button stays bright while it's open.
	gui.DisplayOrder = 11
	gui.Parent = CoreGui
	local root = ReactRoblox.createRoot(gui)
	root:render(React.createElement(React.Fragment, nil, {
		MenuButton = React.createElement(TopBarApp, { useEnabled = TopBar.useEnabled }),
	}))
end

return TopBar
