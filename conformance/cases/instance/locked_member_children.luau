-- A member game scripts can't reach, and a child with the same name.
local names = {
    "Tags", "Attributes", "AttributesSerialize", "HistoryId", "DefinesCapabilities",
    "PropertyStatusStudio", "SerializedOverrides", "UniqueId", "SourceAssetId",
    "Sandboxed",
}
local function read(i, n)
    local ok, v = pcall(function()
        return i[n]
    end)
    return ok, if ok then typeof(v) else v
end
for _, n in names do
    local f = Instance.new("Folder")
    f.Name = "Holder"
    log(n, "alone", read(f, n))
    local c = Instance.new("Folder")
    c.Name = n
    c.Parent = f
    local ok, v = read(f, n)
    log(n, "child", ok, v)
    log(n, "write", pcall(function()
        f[n] = c
    end))
    f:Destroy()
end
local p = Instance.new("Part")
for _, n in { "CollisionGroupReplicate", "PhysicsRepRootPart", "ReplicationPV" } do
    log(n, "alone", read(p, n))
    local c = Instance.new("Folder")
    c.Name = n
    c.Parent = p
    log(n, "child", read(p, n))
end
p:Destroy()
