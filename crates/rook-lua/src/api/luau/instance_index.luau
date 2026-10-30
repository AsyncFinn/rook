local keys, methods, index, newindex, index_other, newindex_other,
    fast_index, member_ids, instances, create, class_names, names,
    verify_names = ...
-- Keys some class has as a method. Property reads skip the
-- per-instance cache lookup below, which costs a table miss
-- per instance when a script reads thousands of them.
local method_names = {}
local function get(self, key)
    -- The most read property, kept per instance and dropped inside
    -- every rename (`names.rs`), so walks over thousands of instances
    -- never leave Luau. Only instances have entries.
    if key == "Name" then
        local name = names[self]
        if name ~= nil then
            if verify_names then
                local now, err = index(keys[self], key)
                if err == nil and now ~= name then
                    error(`[rook] kept Name {name} is stale: {now}`, 0)
                end
            end
            return name
        end
    elseif key == "ClassName" or key == "className" then
        local name = class_names[self]
        if name ~= nil then
            return name
        end
    end
    local k = keys[self]
    if k == nil or type(key) ~= "string" then
        return index_other(self, key)
    end
    -- An instance's class never changes, and old places check the
    -- class of thousands of instances a frame: keep it (weak keys).
    if key == "ClassName" or key == "className" then
        local name, err = index(k, key)
        if err then
            error(err, 0)
        end
        class_names[self] = name
        return name
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
        elseif tag == 2 then
            names[self] = value
            return value
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
