-- What protected calls get from a failing datatype method: text. Where the
-- text begins with the place it was raised, only what follows is logged.
local function caught(f, ...)
    local r = table.pack(pcall(f, ...))
    local e = r[2]
    if r[1] then
        return "no error"
    end
    return type(e), type(e) == "string" and (e:gsub("^[^:]*:%d+: ", "")) or tostring(e)
end
local v, c = Vector3.new(1, 2, 3), CFrame.new()

log("unknown Vector3 member", caught(function() return v.Foo end))
log("Vector3:Dot", caught(function() return v:Dot(1) end))
log("Vector3 assignment", caught(function() v.X = 1 end))
log("CFrame:ToWorldSpace", caught(function() return c:ToWorldSpace(1) end))
log("CFrame * number", caught(function() return c * 1 end))
log("CFrame.new", caught(function() return CFrame.new("x") end))
log("UDim2:Lerp", caught(function() return UDim2.new():Lerp(1, 2) end))
log("Color3:Lerp", caught(function() return Color3.new():Lerp(1, 2) end))
log("Ray:ClosestPoint", caught(function() return Ray.new(v, v):ClosestPoint(1) end))
log("TweenInfo.new", caught(function() return TweenInfo.new("x") end))

log("xpcall handler", xpcall(function()
    return v:Dot(1)
end, function(e)
    return type(e)
end))
do
    local co = coroutine.create(function()
        return v:Dot(1)
    end)
    local ok, e = coroutine.resume(co)
    log("coroutine.resume", ok, type(e))
end
log("error values pass as they are", pcall(error, { 1 }) == false, type(select(2, pcall(error, workspace))))
log("ypcall", type(select(2, ypcall(function()
    return v:Dot(1)
end))))
