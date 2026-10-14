local error, type, typeof, tostring, pairs, setmetatable = ...
local stores = setmetatable({}, { __mode = "k" })
local ds = {}
function ds.data(store)
    local t = stores[store]
    if not t then
        t = {}
        stores[store] = t
    end
    return t
end
function ds.key(key)
    if type(key) ~= "string" and type(key) ~= "number" then
        error("Argument 1 missing or nil", 3)
    end
    return tostring(key)
end
-- Data stores keep copies of plain data only (creator-docs Data
-- stores: tables, strings, numbers, booleans).
function ds.copy(value, checking, seen)
    local t = type(value)
    if t == "table" then
        seen = seen or {}
        if seen[value] then
            error("Tables with cycles cannot be stored", 0)
        end
        seen[value] = true
        local out = {}
        for k, v in pairs(value) do
            out[ds.copy(k, checking, seen)] = ds.copy(v, checking, seen)
        end
        seen[value] = nil
        return out
    elseif t == "string" or t == "number" or t == "boolean" or t == "nil" then
        return value
    elseif checking then
        error(("Cannot store %s in data store. Data stores can only accept valid UTF-8 characters."):format(typeof(value)), 0)
    end
    return nil
end
return ds
