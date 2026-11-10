-- Content: the datatype modern asset properties (ImageContent,
-- AudioContent...) hold. Its members, the global `Content` table's
-- constructors and their errors, tostring, equality and typeof, and how
-- properties take it. Objects (EditableImage) are left out.
local function caught(f, ...)
    local r = table.pack(pcall(f, ...))
    if r[1] then
        local s = {}
        for i = 2, r.n do
            s[#s + 1] = tostring(r[i])
        end
        return "ok", table.concat(s, " ")
    end
    local e = r[2]
    return "error", type(e) == "string" and (e:gsub("^[^:]*:%d+: ", "")) or tostring(e)
end

local u, n = Content.fromUri("rbxasset://d.png"), Content.none
log("table", typeof(Content), table.isfrozen(Content))
local keys = {}
for k, v in pairs(Content) do
    keys[#keys + 1] = k .. ":" .. typeof(v)
end
table.sort(keys)
log("keys", table.concat(keys, " "))
log("uri", u, typeof(u), type(u), u.SourceType, u.Uri, u.Object)
log("none", n, typeof(n), n.SourceType, n.Uri, n.Object)
log("same none", rawequal(n, Content.none))
log("equal", u == Content.fromUri("rbxasset://d.png"), u == n, n == Content.fromUri(""))
log("not raw equal", rawequal(u, Content.fromUri("rbxasset://d.png")))
log("other types", n == nil, Content.fromUri("a") == "a")

log("unknown member", caught(function() return u.Foo end))
log("number key", caught(function() return u[1] end))
log("assign member", caught(function() u.Uri = "x" end))
log("table lookup", caught(function() return Content.foo end))
log("table assign", caught(function() Content.x = 1 end))
log("Content.new", caught(function() return Content.new() end))
log("compare", caught(function() return n < n end))
log("concat", caught(function() return "x" .. n end))
log("length", caught(function() return #n end))
log("call", caught(function() return n() end))

local function try(name, ...)
    log(name, caught(Content[name:match("^%a+")], ...))
end
try("fromUri no args")
try("fromUri nil", nil)
try("fromUri empty", "")
try("fromUri spaces", "  a  ")
try("fromUri number", 5)
try("fromUri fraction", 1.5)
try("fromUri boolean", true)
try("fromUri table", {})
try("fromUri two", "a", "b")
try("fromAssetId no args")
try("fromAssetId 5", 5)
try("fromAssetId 5.5", 5.5)
try("fromAssetId 4.5", 4.5)
try("fromAssetId -1", -1)
try("fromAssetId 0", 0)
try("fromAssetId 2^53", 2 ^ 53)
try("fromAssetId 2.5", 2.5)
try("fromAssetId -0.5", -0.5)
try("fromAssetId -1.5", -1.5)
try("fromAssetId 2^62", 2 ^ 62)
try("fromAssetId 2^63", 2 ^ 63)
try("fromAssetId 1e19", 1e19)
try("fromAssetId 1e20", 1e20)
try("fromAssetId -2^63", -2 ^ 63)
try("fromAssetId -1e20", -1e20)
try("fromAssetId nan", 0 / 0)
try("fromAssetId inf", math.huge)
try("fromAssetId string", "7")
try("fromAssetId bad string", "abc")
try("fromAssetId table", {})
try("fromObject no args")
try("fromObject nil", nil)
try("fromObject string", "x")
try("fromObject Part", Instance.new("Part"))
try("fromObject Content", Content.none)
log("method call", caught(function() return Content:fromUri("x") end))

local label = Instance.new("ImageLabel")
log("property default", label.ImageContent, label.ImageContent == Content.none, ("%q"):format(label.Image))
label.ImageContent = u
log("property set", label.ImageContent, label.Image, rawequal(label.ImageContent, label.ImageContent))
label.Image = "rbxassetid://5"
log("legacy set", label.ImageContent)
label.Image = ""
log("legacy empty", label.ImageContent)
label.ImageContent = Content.fromUri("")
log("empty uri", label.ImageContent, ("%q"):format(label.Image))
log("assign string", caught(function() label.ImageContent = "rbxasset://x.png" end))
log("assign nil", caught(function() label.ImageContent = nil end))
log("assign Vector3", caught(function() label.ImageContent = Vector3.zero end))
log("legacy takes Content", caught(function() label.Image = u end))
label:Destroy()
