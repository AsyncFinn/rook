-- Error messages from engine members, as pcall sees them.
local function try(f)
    local ok, err = pcall(f)
    log(ok, err)
end
try(function() return workspace.Nope end)
try(function() Instance.new("Part").Anchored = "yes" end)
try(function() Instance.new("NotAClass") end)
try(function() workspace.Parent = nil end)
local f = Instance.new("Folder")
try(function() f.Parent = f end)
local g = Instance.new("Folder"); g.Parent = f
try(function() f.Parent = g end)
f:Destroy()
try(function() f.Parent = workspace end)
try(function() game:GetService("NotAService") end)
try(function() Instance.new("Part"):FindFirstChild() end)
try(function() local x = Instance.new("Part") x.FindFirstChild("a") end)
