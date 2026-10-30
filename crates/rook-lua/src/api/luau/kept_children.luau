local keys, instances, create = ...
local clone = table.clone
-- Per instance userdata (weak keys): the children array last filled,
-- and the DOM's version of the children it holds.
local arrays = setmetatable({}, { __mode = "k" })
local versions = setmetatable({}, { __mode = "k" })
return function(fast, general)
    return function(self, ...)
        local k = keys[self]
        if k == nil then
            return general(self, ...)
        end
        local fresh, version = fast(k, versions[self])
        if fresh == nil then
            return clone(arrays[self])
        end
        for i = 1, #fresh do
            local key = fresh[i]
            fresh[i] = instances[key] or create(key)
        end
        arrays[self] = fresh
        versions[self] = version
        -- Scripts may change what they get, never the kept array.
        return clone(fresh)
    end
end
