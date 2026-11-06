-- What arrives through bindables: copies, by rules of their own.
local event = Instance.new("BindableEvent")
local fn = Instance.new("BindableFunction")

-- What `send(...)` delivers to one listener.
local function send(...)
    local got
    local c = event.Event:Connect(function(...)
        got = table.pack(...)
    end)
    local ok, e = pcall(event.Fire, event, ...)
    task.wait()
    c:Disconnect()
    if not ok then
        return nil, e
    end
    return got
end
local function keys(t)
    local out = {}
    for k, x in pairs(t) do
        out[#out + 1] = typeof(k) .. ":" .. tostring(k):gsub("0x%x+", "0x") .. "=" .. tostring(x):gsub("0x%x+", "0x")
    end
    table.sort(out)
    return table.concat(out, " | ")
end
local function first(...)
    local got, e = send(...)
    if not got then
        return "error: " .. tostring(e)
    end
    return "#" .. #got[1] .. " " .. keys(got[1])
end

log("array", first({ 1, 2, 3 }))
log("array with a key", first({ 1, 2, x = 3 }))
log("a hole inside", first({ "a", nil, "c" }))
log("after a hole", first({ [1] = 1, [3] = 3 }))
log("mixed without 1", first({ [2] = "a", x = 1 }))
log("number keys", first({ [1.5] = "a", [-2.7] = "b", [1e20] = "c", [math.huge] = "d", [0] = "e" }))
log("Vector3 key", first({ [Vector3.new(1, 1, 1)] = 1 }))
log("table key", first({ [{}] = 1 }))
log("function key", first({ [print] = 1 }))
log("CFrame key", first({ [CFrame.new()] = 1 }))
log("Instance key", first({ [workspace] = 1 }))
log("BrickColor key", first({ [BrickColor.new("Medium stone grey")] = 1 }))
log("EnumItem key", first({ [Enum.Material.Wood] = 1 }))
log("bool key", first({ [true] = 1 }))
log("nested bool key", first({ { [false] = 1 } }))

do
    local t = {}
    t.self = t
    log("cycle", first(t))
    local shared = {}
    local got = send({ a = shared, b = shared })
    log("shared", got[1].a == got[1].b)
    local two = send(shared, shared)
    log("shared across arguments", two[1] == two[2], two.n)
end

do
    local got = send(setmetatable({ 1 }, { __index = { x = 1 } }))
    log("metatable", getmetatable(got[1]), got[1].x)
    log("frozen", table.isfrozen(send(table.freeze({ 1 }))[1]))
    local f = function() end
    log("function", send(f)[1] == f, send({ f })[1][1] == f)
    log("thread", send(coroutine.create(f))[1])
    local part = Instance.new("Part")
    local got2 = send(part, Vector3.new(1, 2, 3), nil, nil)
    log("values", got2[1] == part, got2[2], got2.n)
    part:Destroy()
end

do
    -- Made when fired, once per listener.
    local seen = {}
    local a = event.Event:Connect(function(t)
        seen[#seen + 1] = t.x
        t.x = 99
    end)
    local b = event.Event:Connect(function(t)
        seen[#seen + 1] = t.x
    end)
    local t = { x = 1 }
    event:Fire(t)
    t.x = 2
    task.wait()
    a:Disconnect()
    b:Disconnect()
    table.sort(seen)
    log("listeners", table.concat(seen, " "))
end

do
    local arg
    fn.OnInvoke = function(t)
        arg = t
        return t, { y = setmetatable({}, {}) }
    end
    local t = { 1, x = 2 }
    local back, other = fn:Invoke(t)
    log("invoke", arg == t, arg.x, back == arg, back.x, getmetatable(other.y))
    fn.OnInvoke = function()
        return { [true] = 1 }
    end
    log("invoke result", pcall(fn.Invoke, fn))
end
event:Destroy()
fn:Destroy()
