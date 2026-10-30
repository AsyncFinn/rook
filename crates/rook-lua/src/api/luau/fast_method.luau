local keys, instances, create = ...
local function result(r, tag)
    if tag == 1 then
        return instances[r] or create(r)
    end
    return r
end
return function(fast, general, takes_string)
    if takes_string then
        return function(self, a, ...)
            local k = keys[self]
            if k ~= nil and type(a) == "string" then
                return result(fast(k, a, (...)))
            end
            return general(self, a, ...)
        end
    end
    return function(self, ...)
        local k = keys[self]
        if k ~= nil then
            return result(fast(k))
        end
        return general(self, ...)
    end
end
