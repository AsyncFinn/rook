-- How a property assignment casts what it's given, recorded in
-- content/sound_effects and moved here: SoundEffect.Priority, an int,
-- set past int32 and to NaN, and CompressorSoundEffect.SideChain, a
-- reference, set to a string. Nothing here is parented or played.
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

local function num(x)
	if typeof(x) ~= "number" then
		return tostring(x)
	elseif x ~= x then
		return "nan"
	elseif x == math.huge then
		return "inf"
	elseif x == -math.huge then
		return "-inf"
	end
	local s = ("%.4g"):format(x)
	return s == "-0" and "0" or s
end

local chorus = Instance.new("ChorusSoundEffect")
for _, v in { 2 ^ 31, -2 ^ 31 - 1, 1e12, 0 / 0 } do
	local ok = pcall(function()
		chorus.Priority = v
	end)
	log("Priority", num(v), ok, chorus.Priority)
end
chorus:Destroy()

local compressor = Instance.new("CompressorSoundEffect")
log("SideChain string", caught(function()
	compressor.SideChain = "x"
end))
compressor:Destroy()
