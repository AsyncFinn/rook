-- The menu's pages by tab name, and the tabs in order with their icons.

local Pages = {
	People = require(script.People),
	Settings = require(script.Settings),
	Help = require(script.Help),
}

-- Roblox's tabs are People, Settings, Gallery, Report and Help; Rook
-- has no screenshots to show or reports to send, so it leaves out
-- Gallery and Report (docs/behavior/gui/menu.md). `id` is the tab's name
-- in Roblox's HubBar.
Pages.tabs = {
	{ name = "People", icon = "two-people", id = "PeopleTab" },
	{ name = "Settings", icon = "gear", id = "GameSettingsTab" },
	{ name = "Help", icon = "circle-question", id = "HelpTab" },
}

return Pages
