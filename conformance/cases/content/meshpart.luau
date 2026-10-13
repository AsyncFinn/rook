-- What scripts see of a MeshPart's size and mesh: MeshSize is the size the
-- mesh was imported at (InitialSize in files) and doesn't follow Size;
-- MeshId can't be written by game scripts.
local mp = Instance.new("MeshPart")
log("new", mp.Size, mp.MeshSize, string.format("%q", mp.MeshId))
log("set MeshId", pcall(function()
    mp.MeshId = "rbxassetid://12221462"
end))
log("MeshId after", string.format("%q", mp.MeshId))
log("InitialSize", pcall(function()
    return mp.InitialSize
end))
mp.Size = Vector3.new(2, 3, 4)
log("after Size", mp.Size, mp.MeshSize)
log("set MeshSize", pcall(function()
    mp.MeshSize = Vector3.one
end))
log("MeshSize after", mp.MeshSize)
local copy = mp:Clone()
log("clone", copy.Size, copy.MeshSize)
copy:Destroy()
mp:Destroy()

-- File meshes: defaults of the mesh classes.
for _, class in { "SpecialMesh", "FileMesh" } do
    local m = Instance.new(class)
    log(class, string.format("%q", m.MeshId), m.Scale, m.Offset, m.VertexColor)
    m:Destroy()
end
log("SpecialMesh type", Instance.new("SpecialMesh").MeshType)
