-- Functions bound to the render step: each frame, before RenderStepped
-- fires, they run by priority, lowest first, each on its own thread so
-- one failing doesn't stop the rest (creator-docs
-- RunService:BindToRenderStep). Binding a name again replaces it.
local std = ...
local task, table = std.task, std.table

local bindings = {}
local count = 0

local api = {}

function api.unbind(name)
	for i, b in bindings do
		if b.name == name then
			table.remove(bindings, i)
			return
		end
	end
end

function api.bind(name, priority, fn)
	api.unbind(name)
	count += 1
	table.insert(bindings, { name = name, priority = priority, fn = fn, order = count })
	-- Equal priorities keep the order they were bound in.
	table.sort(bindings, function(a, b)
		if a.priority ~= b.priority then
			return a.priority < b.priority
		end
		return a.order < b.order
	end)
end

function api.step(dt)
	if #bindings == 0 then
		return
	end
	for _, b in table.clone(bindings) do
		task.spawn(b.fn, dt)
	end
end

return api
