-- What values property assignments accept, and what they turn into.
local function try(label, f)
    local ok, result = pcall(f)
    log(label, ok, result)
end
local p = Instance.new("Part")
for _, v in { "yes", "", 0, 1, false } do
    try("Anchored = " .. typeof(v) .. " " .. tostring(v), function()
        p.Anchored = v
        return p.Anchored
    end)
end
try("Anchored = nil", function() p.Anchored = nil return p.Anchored end)
try("Anchored = table", function() p.Anchored = {} return p.Anchored end)
try("Transparency = '0.5'", function() p.Transparency = "0.5" return p.Transparency end)
try("Transparency = 'abc'", function() p.Transparency = "abc" return p.Transparency end)
try("Transparency = true", function() p.Transparency = true return p.Transparency end)
try("Name = 5", function() p.Name = 5 return p.Name end)
try("Name = nil", function() p.Name = nil return p.Name end)
try("Name = true", function() p.Name = true return p.Name end)
try("Material = 'Neon'", function() p.Material = "Neon" return p.Material end)
try("Material = 288", function() p.Material = 288 return p.Material end)
try("Material = 'Nope'", function() p.Material = "Nope" return p.Material end)
try("Material = 9999", function() p.Material = 9999 return p.Material end)
try("Size = nil", function() p.Size = nil return p.Size end)
try("CFrame = Vector3", function() p.CFrame = Vector3.one return p.CFrame end)
p:Destroy()
