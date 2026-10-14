local keys, methods, index, newindex, index_other, newindex_other,
    fast_index, member_ids, instances, create = ...
-- Keys some class has as a method. Property reads skip the
-- per-instance cache lookup below, which costs a table miss
-- per instance when a script reads thousands of them.
local method_names = {}
local function get(self, key)
    local k = keys[self]
    if k == nil or type(key) ~= "string" then
        return index_other(self, key)
    end
    if method_names[key] then
        local method = methods[self][key]
        if method ~= nil then
            return method
        end
    end
    -- A plain stored property read before on this class:
    -- read it by id. Instances come back as their keys.
    local id = member_ids[key]
    if id ~= nil then
        local value, tag = fast_index(k, id)
        if tag == nil then
            return value
        elseif tag == 1 then
            return instances[value] or create(value)
        end
    end
    local value, err, is_method, fast_id = index(k, key)
    if err then
        error(err, 0)
    end
    if is_method then
        method_names[key] = true
        methods[self][key] = value
    elseif fast_id ~= nil then
        member_ids[key] = fast_id
    end
    return value
end
local function set(self, key, value)
    local k = keys[self]
    if k == nil or type(key) ~= "string" then
        newindex_other(self, key, value)
        return
    end
    local err = newindex(k, key, value)
    if err then
        error(err, 0)
    end
end
return get, set
