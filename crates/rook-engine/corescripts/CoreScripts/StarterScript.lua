-- The CoreScript a client starts first, which starts the others, as
-- Roblox's StarterScript does: each piece runs as its own CoreScript
-- under RobloxGui, so one failing leaves the rest running.

local CoreGui = game:GetService("CoreGui")
local ScriptContext = game:GetService("ScriptContext")

local RobloxGui = CoreGui:WaitForChild("RobloxGui")

for _, name in { "TopBar", "Menu" } do
	local ok, err = pcall(function()
		ScriptContext:AddCoreScriptLocal("CoreScripts/" .. name, RobloxGui)
	end)
	if not ok then
		warn("Rook's CoreScripts: " .. name .. ": " .. tostring(err))
	end
end
