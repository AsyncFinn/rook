-- The in-game menu's state and what opening it does: games hear it
-- through GuiService (MenuIsOpen, MenuOpened, MenuClosed; creator-docs),
-- the mouse is freed, and movement keys stop reaching the game. The top
-- bar's button and Escape open it. MenuApp draws it.

local CorePackages = game:GetService("CorePackages")
local ContextActionService = game:GetService("ContextActionService")
local GuiService = game:GetService("GuiService")
local UserInputService = game:GetService("UserInputService")

local React = require(CorePackages.Packages.React)

local Menu = {}

export type State = {
	open: boolean,
	-- The Leave confirmation shows in place of the pages.
	leaving: boolean,
	page: string,
}

-- Roblox's menu opens on People.
local state: State = { open = false, leaving = false, page = "People" }
local listeners: { [() -> ()]: true } = {}

local function update(patch)
	local changed = false
	local new = table.clone(state)
	for k, v in patch do
		if new[k] ~= v then
			new[k] = v
			changed = true
		end
	end
	if not changed then
		return
	end
	state = table.freeze(new)
	for f in listeners do
		f()
	end
end

function Menu.state(): State
	return state
end

-- Calls `f` after each change; returns a function that stops it.
function Menu.subscribe(f: () -> ()): () -> ()
	listeners[f] = true
	return function()
		listeners[f] = nil
	end
end

-- The menu's state in a component, rendering again when it changes.
function Menu.useState(): State
	local current, set = React.useState(state)
	React.useEffect(function()
		set(state)
		return Menu.subscribe(function()
			set(state)
		end)
	end, {})
	return current
end

local SINK = "RookMenuSink"

function Menu.setOpen(open: boolean)
	if open == state.open then
		return
	end
	if open then
		-- Keys don't reach the game's controls under the menu.
		pcall(function()
			ContextActionService:BindCoreActionAtPriority(SINK, function()
				return Enum.ContextActionResult.Sink
			end, false, 3000, Enum.KeyCode.W, Enum.KeyCode.A, Enum.KeyCode.S, Enum.KeyCode.D,
				Enum.KeyCode.Space, Enum.KeyCode.Up, Enum.KeyCode.Down, Enum.KeyCode.Left,
				Enum.KeyCode.Right)
		end)
	else
		pcall(function()
			ContextActionService:UnbindCoreAction(SINK)
		end)
	end
	update({ open = open, leaving = false })
	-- Games hear it open at once, and closed once it's slid away
	-- (MenuApp calls Menu.closed).
	if open then
		pcall(function()
			GuiService:SetMenuIsOpen(true, "RookMenu")
		end)
	end
end

-- The menu has finished closing.
function Menu.closed()
	if not state.open then
		pcall(function()
			GuiService:SetMenuIsOpen(false, "RookMenu")
		end)
	end
end

function Menu.toggle()
	Menu.setOpen(not state.open)
end

function Menu.setLeaving(on: boolean)
	update({ leaving = on })
end

function Menu.setPage(page: string)
	update({ page = page })
end

function Menu.leave()
	pcall(function()
		game:Shutdown()
	end)
end

-- Put the menu's ScreenGui in CoreGui and draw it there.
function Menu.start()
	local ReactRoblox = require(CorePackages.Packages.ReactRoblox)
	local MenuApp = require(script.MenuApp)
	local gui = Instance.new("ScreenGui")
	gui.Name = "RookMenu"
	gui.ResetOnSpawn = false
	gui.IgnoreGuiInset = true
	gui.ZIndexBehavior = Enum.ZIndexBehavior.Sibling
	gui.DisplayOrder = 10
	gui.Parent = game:GetService("CoreGui")
	ReactRoblox.createRoot(gui):render(React.createElement(MenuApp))
end

-- Escape opens and closes the menu (or backs out of Leave), L asks to
-- leave and Enter confirms.
UserInputService.InputBegan:Connect(function(input, processed)
	if input.UserInputType ~= Enum.UserInputType.Keyboard then
		return
	end
	local key = input.KeyCode
	if key == Enum.KeyCode.Escape then
		-- Escape that ended typing in a TextBox is the TextBox's.
		if processed then
			return
		end
		if state.leaving then
			Menu.setLeaving(false)
		else
			Menu.toggle()
		end
	elseif not state.open or processed then
		return
	elseif key == Enum.KeyCode.L and not state.leaving then
		Menu.setLeaving(true)
	elseif key == Enum.KeyCode.Return and state.leaving then
		Menu.leave()
	end
end)

return Menu
