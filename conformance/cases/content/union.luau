-- What scripts see of unions: the classes' defaults, that Size and
-- UsePartColor can be written, and that the geometry's asset isn't
-- readable or writable from game scripts.
for _, class in { "UnionOperation", "NegateOperation", "IntersectOperation" } do
    local u = Instance.new(class)
    log(class, u.Name, u.Size, u.UsePartColor, u.Color, u.Transparency)
    u:Destroy()
end
local u = Instance.new("UnionOperation")
u.Size = Vector3.new(8, 5, 2.5)
u.UsePartColor = true
log("after", u.Size, u.UsePartColor)
log("AssetId", pcall(function()
    return u.AssetId
end))
log("set AssetId", pcall(function()
    u.AssetId = "rbxassetid://396441598"
end))
log("MeshSize", pcall(function()
    return u.MeshSize
end))
log("IsA", u:IsA("PartOperation"), u:IsA("BasePart"), u:IsA("TriangleMeshPart"))
local copy = u:Clone()
log("clone", copy.Size, copy.UsePartColor)
copy:Destroy()
u:Destroy()
