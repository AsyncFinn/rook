-- Decals and Textures: defaults of what the renderer reads, and the URL
-- rewrite Studio applies when a script sets an image.
local d = Instance.new("Decal")
log("Decal", d.Face, d.Color3, d.Transparency, d.ZIndex, string.format("%q", d.Texture))
local t = Instance.new("Texture")
log("Texture", t.Face, t.StudsPerTileU, t.StudsPerTileV, t.OffsetStudsU, t.OffsetStudsV)
local m = Instance.new("SpecialMesh")
log("SpecialMesh", string.format("%q", m.TextureId), m.VertexColor)

-- Setting Texture: which URL forms become rbxassetid://.
for _, url in {
    "http://www.roblox.com/asset/?id=5",
    "http://www.roblox.com/asset?id=5",
    "http://www.roblox.com/asset/?id=5&version=2",
    "http://www.roblox.com/asset?id=5&version=2",
    "http://www.roblox.com/Asset/?id=5",
    "http://www.roblox.com/asset/?ID=5",
    "https://www.roblox.com/asset/?id=5",
    "http://roblox.com/asset/?id=5",
    "http://www.roblox.com/asset/?version=2&id=5",
    "http://www.roblox.com/asset/?id=5abc",
    "http://www.roblox.com/asset/?id= 5",
    "http://example.com/asset/?id=5",
    "http://example.com/foo?id=5",
    "HTTP://WWW.ROBLOX.COM/ASSET/?ID=5",
    "http://www.roblox.com/asset/?id=5&id=6",
    "http://www.roblox.com//asset/?id=5",
    "http://www.roblox.com/asset/?id=",
    "http://www.roblox.com/asset.aspx?id=5",
    " http://www.roblox.com/asset/?id=5",
    "rbxasset://textures\\face.png",
} do
    d.Texture = url
    t.Texture = url
    m.TextureId = url
    log(url, d.Texture, t.Texture, m.TextureId)
end
d.ColorMap = "http://www.roblox.com/asset/?id=7"
log("ColorMap", d.ColorMap)

-- A SpecialMesh given a MeshId or TextureId becomes a FileMesh.
for _, property in { "TextureId", "MeshId" } do
    for _, value in { "rbxasset://x.png", "" } do
        local s = Instance.new("SpecialMesh")
        s.MeshType = Enum.MeshType.Head
        s[property] = value
        log(property, string.format("%q", value), s.MeshType)
        s:Destroy()
    end
end
local s = Instance.new("SpecialMesh")
s.TextureId = "rbxasset://x.png"
s.MeshType = Enum.MeshType.Sphere
log("type after texture", s.MeshType, s.TextureId)
s:Destroy()
d:Destroy()
t:Destroy()
m:Destroy()
