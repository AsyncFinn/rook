-- ContextActionService's bindings (creator-docs ContextActionService):
-- input GUI didn't take goes to the highest-priority binding for it,
-- the most recently bound first among equals, until one doesn't return
-- Enum.ContextActionResult.Pass. Binding a name again replaces it.
-- Roblox's own (core) bindings hear input before a game's (Inference:
-- a game can't take Escape from the menu).
local std = ...
local task, table, typeof, string = std.task, std.table, std.typeof, std.string

local DEFAULT_PRIORITY = 2000 -- Enum.ContextActionPriority.Default

local stacks = { core = {}, game = {} }
local count = 0

local api = {}

local function find(stack, name)
	for i, b in stack do
		if b.name == name then
			return i
		end
	end
	return nil
end

function api.unbind(kind, name)
	local stack = stacks[kind]
	local i = find(stack, name)
	if i then
		table.remove(stack, i)
	end
end

function api.unbind_all(kind)
	table.clear(stacks[kind])
end

function api.bind(kind, name, fn, touch, priority, ...)
	api.unbind(kind, name)
	if typeof(priority) == "EnumItem" then
		priority = priority.Value
	end
	count += 1
	local stack = stacks[kind]
	table.insert(stack, {
		name = name,
		fn = fn,
		touch = touch,
		priority = priority or DEFAULT_PRIORITY,
		inputs = { ... },
		order = count,
	})
	table.sort(stack, function(a, b)
		if a.priority ~= b.priority then
			return a.priority > b.priority
		end
		return a.order > b.order
	end)
end

-- What GetAllBoundActionInfo returns: each binding's info by name.
function api.info(kind)
	local all = {}
	for _, b in stacks[kind] do
		all[b.name] = {
			inputTypes = table.clone(b.inputs),
			priorityLevel = b.priority,
			stackOrder = b.order,
			createTouchButton = b.touch,
		}
	end
	return all
end

-- A binding's input is an Enum.KeyCode, an Enum.UserInputType, or a
-- one-character string naming a key.
local function matches(want, key, inputType)
	local t = typeof(want)
	if t == "EnumItem" then
		return want == key or want == inputType
	elseif t == "string" then
		return string.upper(want) == key.Name
	end
	return false
end

local function returned_pass(result)
	return typeof(result) == "EnumItem" and result:IsA("ContextActionResult") and result.Name == "Pass"
end

-- Offer an InputObject to the bindings; true if one took it. A callback
-- that yields takes it (Inference: its result isn't there to read yet).
function api.input(object)
	local key, inputType, state = object.KeyCode, object.UserInputType, object.UserInputState
	for _, kind in { "core", "game" } do
		for _, b in table.clone(stacks[kind]) do
			for _, want in b.inputs do
				if matches(want, key, inputType) then
					local result
					task.spawn(function()
						result = b.fn(b.name, state, object)
					end)
					if not returned_pass(result) then
						return true
					end
					break
				end
			end
		end
	end
	return false
end

return api
