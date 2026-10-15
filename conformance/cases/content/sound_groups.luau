-- What scripts see of SoundGroups, sound effects and SoundService:
-- defaults, limits, Sound.SoundGroup, and PlayLocalSound changing nothing
-- scripts can see. No sound here has a file, so nothing is heard.
local SS = game:GetService("SoundService")
log("SoundService", SS.AmbientReverb, SS.RespectFilteringEnabled)

local g = Instance.new("SoundGroup")
log("group volume", g.Volume)
g.Volume = 20
log("above 10", g.Volume)
g.Volume = -1
log("below 0", g.Volume)

local s = Instance.new("Sound")
log("no group", s.SoundGroup)
s.SoundGroup = g
log("in group", s.SoundGroup == g)
log("part as group", pcall(function()
	s.SoundGroup = Instance.new("Part")
end))

local function show(class, props)
	local e = Instance.new(class)
	local values = {}
	for _, p in ipairs(props) do
		table.insert(values, p .. "=" .. string.format("%.2f", e[p]))
	end
	log(class, e.Enabled, e.Priority, table.concat(values, " "))
	e:Destroy()
end
show("EqualizerSoundEffect", { "LowGain", "MidGain", "HighGain" })
show("ReverbSoundEffect", { "DecayTime", "Density", "Diffusion", "DryLevel", "WetLevel" })
show("DistortionSoundEffect", { "Level" })
show("PitchShiftSoundEffect", { "Octave" })
show("ChorusSoundEffect", { "Depth", "Mix", "Rate" })
show("EchoSoundEffect", { "Delay", "DryLevel", "Feedback", "WetLevel" })
show("CompressorSoundEffect", { "Attack", "GainMakeup", "Ratio", "Release", "Threshold" })
show("FlangeSoundEffect", { "Depth", "Mix", "Rate" })
show("TremoloSoundEffect", { "Depth", "Duty", "Frequency" })

-- Effect properties read back what was set, past the range they play in.
local eq = Instance.new("EqualizerSoundEffect")
eq.HighGain = 30
eq.LowGain = -200
local p = Instance.new("PitchShiftSoundEffect")
p.Octave = 5
log("unclamped", eq.HighGain, eq.LowGain, p.Octave)
eq:Destroy()
p:Destroy()

-- PlayLocalSound is for clients; cases run on the server.
log("PlayLocalSound", pcall(function()
	SS:PlayLocalSound(s)
end))
log("PlayLocalSound part", pcall(function()
	SS:PlayLocalSound(Instance.new("Part"))
end))
log("PlayLocalSound nil", pcall(function()
	SS:PlayLocalSound(nil)
end))
log("still", s.Playing, s.TimePosition)
s:Destroy()
g:Destroy()
