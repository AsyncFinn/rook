-- MemStorageService: strings by key that Roblox's own scripts share
-- within one client, and callbacks bound to a key. Only Roblox's scripts
-- may call it, so Studio can't be asked how it behaves; each rule here is
-- Inference from the members' names and types (the API dump): SetItem and
-- Fire call a key's callbacks with the value, BindAndFire calls the new
-- one at once with the stored value if there is one, and Call returns
-- what the key's first callback returns.
local std = ...
local task, table = std.task, std.table

local items = {}
local bound = {}

local api = {}

local function callbacks(key)
	local list = bound[key]
	return if list then table.clone(list) else {}
end

local function fire(key, value)
	for _, fn in callbacks(key) do
		task.spawn(fn, value)
	end
end

function api.get(key, default)
	local value = items[key]
	if value == nil then
		return default or ""
	end
	return value
end

function api.has(key)
	return items[key] ~= nil
end

function api.set(key, value)
	value = value or ""
	items[key] = value
	fire(key, value)
end

function api.remove(key)
	local had = items[key] ~= nil
	items[key] = nil
	return had
end

function api.fire(key, value)
	fire(key, value or "")
end

-- A MemStorageConnection: Disconnect unbinds the callback.
function api.bind(key, fn)
	local list = bound[key] or {}
	bound[key] = list
	table.insert(list, fn)
	local connection = {}
	function connection.Disconnect()
		local i = table.find(list, fn)
		if i then
			table.remove(list, i)
		end
	end
	return connection
end

function api.bind_and_fire(key, fn)
	local connection = api.bind(key, fn)
	local value = items[key]
	if value ~= nil then
		task.spawn(fn, value)
	end
	return connection
end

function api.call(key, input)
	local list = bound[key]
	if list and list[1] then
		return list[1](input)
	end
	return nil
end

return api
