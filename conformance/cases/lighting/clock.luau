-- Lighting's clock: GetSunDirection, GetMoonDirection, GetMoonPhase and
-- GetMinutesAfterMidnight at a few ClockTime and GeographicLatitude
-- values, what ClockTime, TimeOfDay, SetMinutesAfterMidnight and
-- GeographicLatitude read back after odd values, and which properties
-- report a change. docs/behavior/lighting/sun.md, sky.md. Every Lighting
-- property changed here is put back at the end.
local L = game:GetService("Lighting")
local saved = { TimeOfDay = L.TimeOfDay, GeographicLatitude = L.GeographicLatitude }

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

local function num(x, digits)
	if typeof(x) ~= "number" then
		return typeof(x)
	elseif x ~= x then
		return "nan"
	elseif x == math.huge then
		return "inf"
	elseif x == -math.huge then
		return "-inf"
	end
	local s = ("%." .. (digits or 3) .. "f"):format(x)
	return (s:match("^%-[0.]+$") and s:sub(2)) or s
end

local function vec3(v)
	if typeof(v) ~= "Vector3" then
		return typeof(v)
	end
	return num(v.X) .. "," .. num(v.Y) .. "," .. num(v.Z)
end

-- A direction, or the error asking for it raised.
local function dir(name)
	local ok, v = pcall(function()
		return L[name](L)
	end)
	if ok then
		return vec3(v)
	end
	return "error " .. (type(v) == "string" and (v:gsub("^[^:]*:%d+: ", "")) or tostring(v))
end

local function body()
	log("GetMinutesAfterMidnight", typeof(L:GetMinutesAfterMidnight()))

	-- Directions at a few times and latitudes.
	for _, lat in { 0, 41.733, -30 } do
		L.GeographicLatitude = lat
		for _, hours in { 0, 3, 6, 9, 12, 14, 18, 21.5 } do
			L.ClockTime = hours
			log(
				("lat %s t %s"):format(num(lat), num(hours)),
				"sun",
				dir("GetSunDirection"),
				"moon",
				dir("GetMoonDirection"),
				"min",
				num(L:GetMinutesAfterMidnight())
			)
		end
	end
	L.ClockTime = 14
	for _, lat in { 23.5, 90, -90, 120, 200, -400 } do
		L.GeographicLatitude = lat
		log(("lat %s t 14"):format(num(lat)), "read", num(L.GeographicLatitude), "sun", dir("GetSunDirection"), "moon", dir("GetMoonDirection"))
	end
	for _, lat in { 1e9, 0 / 0, math.huge } do
		L.GeographicLatitude = lat
		log("lat", num(lat), "read", num(L.GeographicLatitude), "sun", dir("GetSunDirection"))
	end
	L.GeographicLatitude = 41.733

	-- The moon's phase.
	for _, hours in { 0, 12 } do
		L.ClockTime = hours
		log("GetMoonPhase", num(hours), caught(function()
			return num(L:GetMoonPhase())
		end))
	end

	-- ClockTime read back.
	for _, hours in { 0, 13.999, 14.5, 23.9999, 24, 25.5, 48.25, -1, -0.25, 1e9, 1e-9, 0 / 0, math.huge, -math.huge } do
		local ok, e = pcall(function()
			L.ClockTime = hours
		end)
		log("ClockTime", num(hours, 4), ok and "ok" or e, num(L.ClockTime, 4), L.TimeOfDay, num(L:GetMinutesAfterMidnight(), 2))
	end
	log("ClockTime string", caught(function()
		L.ClockTime = "7.5"
		return num(L.ClockTime, 4), L.TimeOfDay
	end))
	log("ClockTime bad string", caught(function()
		L.ClockTime = "x"
		return num(L.ClockTime, 4)
	end))

	-- TimeOfDay strings.
	L.TimeOfDay = "14:00:00"
	for _, s in {
		"06:30:00",
		"6:30",
		"7",
		"12:30:45.5",
		"12:30:45.6",
		"25:00:00",
		"24:00:00",
		"-1:00:00",
		"12:61:00",
		"12:00:75",
		"1:2:3:4",
		" 08:00:00 ",
		"08:00:00am",
		"8h",
		"abc",
		"",
		"99999:00:00",
	} do
		L.TimeOfDay = "14:00:00"
		local ok, e = pcall(function()
			L.TimeOfDay = s
		end)
		log(("TimeOfDay %q"):format(s), ok and "ok" or e, L.TimeOfDay, num(L.ClockTime, 4))
	end
	log("TimeOfDay number", caught(function()
		L.TimeOfDay = 9
		return L.TimeOfDay, num(L.ClockTime, 4)
	end))

	-- SetMinutesAfterMidnight.
	for _, m in { 90, 0.5, 1439.99, 1440, 1500, -30, 1e7, 0 / 0 } do
		L.TimeOfDay = "14:00:00"
		local ok, e = pcall(function()
			L:SetMinutesAfterMidnight(m)
		end)
		log("SetMinutesAfterMidnight", num(m, 2), ok and "ok" or e, L.TimeOfDay, num(L.ClockTime, 4), num(L:GetMinutesAfterMidnight(), 2))
	end
	log("SetMinutesAfterMidnight string", caught(function()
		L:SetMinutesAfterMidnight("60")
		return L.TimeOfDay
	end))
	log("SetMinutesAfterMidnight nil", caught(function()
		L:SetMinutesAfterMidnight(nil)
	end))

	-- What reports a change: ClockTime, TimeOfDay or both.
	L.TimeOfDay = "14:00:00"
	task.wait()
	local changes = {}
	local signals = {}
	table.insert(signals, L.Changed:Connect(function(name)
		if name == "ClockTime" or name == "TimeOfDay" or name == "GeographicLatitude" then
			table.insert(changes, name)
		end
	end))
	local property = {}
	for _, name in { "ClockTime", "TimeOfDay" } do
		table.insert(signals, L:GetPropertyChangedSignal(name):Connect(function()
			table.insert(property, name)
		end))
	end
	local function report(label)
		task.wait()
		table.sort(changes)
		table.sort(property)
		log("changed", label, table.concat(changes, ","), "|", table.concat(property, ","))
		table.clear(changes)
		table.clear(property)
	end
	L.ClockTime = 10
	report("ClockTime")
	L.TimeOfDay = "11:00:00"
	report("TimeOfDay")
	L:SetMinutesAfterMidnight(12 * 60)
	report("SetMinutesAfterMidnight")
	L.ClockTime = 12
	report("same ClockTime")
	L.TimeOfDay = "12:00:00.4"
	report("TimeOfDay under a second")
	L.ClockTime = 12.00005
	report("ClockTime under a second")
	for _, c in signals do
		c:Disconnect()
	end
end

local ok, e = pcall(body)
L.TimeOfDay = saved.TimeOfDay
L.GeographicLatitude = saved.GeographicLatitude
if not ok then
	error(e, 0)
end
log("restored", L.TimeOfDay == saved.TimeOfDay, L.GeographicLatitude == saved.GeographicLatitude)
