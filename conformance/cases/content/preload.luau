-- ContentProvider on a server, and how content properties store strings.
local CP = game:GetService("ContentProvider")
log("RequestQueueSize", CP.RequestQueueSize)

-- Nothing to fetch: returns at once, without yielding.
local folder = Instance.new("Folder")
CP:PreloadAsync({})
CP:PreloadAsync({ "", folder }, function(id) log("callback", id) end)
log("nothing to fetch returned")
folder:Destroy()

-- Something to fetch: a server never finishes.
local finished = false
task.spawn(function()
    CP:PreloadAsync({ "abc" }, function(id, status) log("callback", id, status) end)
    finished = true
end)
task.wait(1)
log("server preload finished", finished)

for _, case in {
    { "string", "abc" },
    { "number", 5 },
    { "bool element", { true } },
    { "number element", { 5 } },
    { "Vector3 element", { Vector3.zero } },
} do
    log(case[1], pcall(CP.PreloadAsync, CP, case[2]))
end
log("nil", pcall(CP.PreloadAsync, CP))
log("number callback", pcall(CP.PreloadAsync, CP, {}, 5))

-- Content properties: backslashes in rbxasset:// paths become slashes,
-- case is kept, other strings are kept as they are.
local mesh = Instance.new("SpecialMesh")
local sound = Instance.new("Sound")
for _, value in {
    "rbxasset://Fonts\\Head.mesh",
    "rbxassetid://12221462",
    "http://www.roblox.com/asset/?id=12221462",
    "  rbxassetid://5 ",
    "abc",
    "",
} do
    mesh.MeshId = value
    sound.SoundId = value
    log(string.format("%q -> %q %q", value, mesh.MeshId, sound.SoundId))
end
mesh:Destroy()
sound:Destroy()
