local keys, instances, create, arrays, verify = ...
local clone = table.clone
-- `arrays` (weak keys) holds each parent's filled children array, dropped
-- inside every change to its children (`kept.rs`); the DOM's version of
-- the children each holds is kept for debug builds' check.
local versions = setmetatable({}, { __mode = "k" })
return function(fast, general)
    return function(self, ...)
        local kept = arrays[self]
        if kept ~= nil then
            if verify and fast(keys[self], versions[self]) ~= nil then
                error("[rook] kept children are stale", 0)
            end
            -- Scripts may change what they get, never the kept array.
            return clone(kept)
        end
        local k = keys[self]
        if k == nil then
            return general(self, ...)
        end
        local fresh, version = fast(k, nil)
        for i = 1, #fresh do
            local key = fresh[i]
            fresh[i] = instances[key] or create(key)
        end
        arrays[self] = fresh
        versions[self] = version
        return clone(fresh)
    end
end
