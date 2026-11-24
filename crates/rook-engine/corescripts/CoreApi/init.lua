-- The one module that registers SetCore and GetCore names with
-- StarterGui, so two pieces can't take the same name: a piece calls
-- CoreApi.register for each name Names.lua gives it.

local StarterGui = game:GetService("StarterGui")

local Names = require(script.Names)

local CoreApi = {}

local registered = {}

-- Answer `name` for `owner`: `set(value)` for SetCore, `get()` for
-- GetCore (either may be nil).
function CoreApi.register(owner: string, name: string, handlers: { set: ((any) -> ())?, get: (() -> any)? })
	local entry = Names[name]
	assert(entry, `SetCore name {name} isn't listed in CoreApi.Names`)
	assert(entry.owner == owner, `{owner} can't register {name}: it belongs to {entry.owner}`)
	assert(not registered[name], `{name} is already registered`)
	registered[name] = true
	if handlers.set then
		StarterGui:RegisterSetCore(name, handlers.set)
	end
	if handlers.get then
		StarterGui:RegisterGetCore(name, handlers.get)
	end
end

return CoreApi
