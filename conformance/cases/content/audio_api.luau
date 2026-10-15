-- The audio API as a server script sees it: defaults and ranges, pins,
-- which Wires connect, attenuation curves, AudioPlayer's playback rules,
-- and analyzers reading nothing on a server. Nothing is wired to an
-- AudioDeviceOutput, so nothing is heard.
local folder = Instance.new("Folder")
folder.Parent = workspace

local function near(x, target)
	return math.abs(x - target) < 0.12
end

-- Defaults and the ranges properties keep to.
local p = Instance.new("AudioPlayer")
log("player", p.Asset, p.Volume, p.PlaybackSpeed, p.TimePosition, p.Looping, p.AutoLoad, p.AutoPlay)
log("player state", p.IsPlaying, p.IsReady, p.TimeLength)
for _, v in { -1, 11, 0 / 0 } do
	p.Volume = v
	log("Volume", v, p.Volume)
end
p.PlaybackSpeed = 25
log("PlaybackSpeed 25", p.PlaybackSpeed)
p.TimePosition = -1
log("TimePosition -1", p.TimePosition)
p.Asset = "rbxasset://sounds\\swordslash.wav"
log("Asset", p.Asset)
log("write IsPlaying", pcall(function()
	p.IsPlaying = true
end))
p:Destroy()

local function range(class, props)
	local e = Instance.new(class)
	local parts = {}
	for _, name in props do
		local default = e[name]
		e[name] = -1e9
		local low = e[name]
		e[name] = 1e9
		table.insert(parts, string.format("%s=%g[%g,%g]", name, default, low, e[name]))
	end
	log(class, e.Bypass, table.concat(parts, " "))
	e:Destroy()
end
range("AudioFader", { "Volume" })
range("AudioEqualizer", { "LowGain", "MidGain", "HighGain" })
range("AudioDistortion", { "Level" })
range("AudioPitchShifter", { "Pitch" })
range("AudioReverb", { "DecayTime", "Density", "Diffusion", "DryLevel", "WetLevel", "HighCutFrequency" })
range("AudioChorus", { "Depth", "Mix", "Rate" })
range("AudioEcho", { "DelayTime", "DryLevel", "Feedback", "WetLevel" })
range("AudioCompressor", { "Attack", "MakeupGain", "Ratio", "Release", "Threshold" })
local eq = Instance.new("AudioEqualizer")
log("MidRange", eq.MidRange)
eq.MidRange = NumberRange.new(10, 30000)
log("MidRange 10-30000", eq.MidRange)
eq:Destroy()

-- Pins.
for _, class in { "AudioPlayer", "AudioEmitter", "AudioListener", "AudioDeviceOutput", "AudioAnalyzer", "AudioFader", "AudioCompressor" } do
	local i = Instance.new(class)
	log("pins", class, table.concat(i:GetInputPins(), ","), table.concat(i:GetOutputPins(), ","))
	i:Destroy()
end

-- Which wires connect.
local player = Instance.new("AudioPlayer", folder)
local fader = Instance.new("AudioFader", folder)
local fader2 = Instance.new("AudioFader", folder)
local w = Instance.new("Wire")
log("wire", w.SourceName, w.TargetName, w.Connected)
w.Parent = folder
w.SourceInstance = player
log("source only", w.Connected)
w.TargetInstance = fader
log("player to fader", w.Connected, #player:GetConnectedWires("Output"), #fader:GetConnectedWires("Input"))
w.TargetName = "input"
log("pin case", w.Connected)
w.TargetName = "Input"
w.TargetInstance = Instance.new("AudioDeviceOutput")
log("target outside the DataModel", w.Connected)
w.TargetInstance = player
log("to itself", w.Connected)
w.SourceInstance = fader
w.TargetInstance = fader2
local back = Instance.new("Wire", folder)
back.SourceInstance = fader2
back.TargetInstance = fader
log("loop", w.Connected, back.Connected)
back:Destroy()
w.Parent = nil
log("wire outside the DataModel", w.Connected)
w:Destroy()

-- Attenuation curves.
local emitter = Instance.new("AudioEmitter")
log("emitter", emitter.DistanceAttenuationMode, emitter.DistanceAttenuationBounds, emitter.AudioInteractionGroup)
local function curve(t)
	local keys = {}
	for k, v in t do
		table.insert(keys, string.format("%g=%g", k, v))
	end
	table.sort(keys)
	return table.concat(keys, " ")
end
log("no curve", curve(emitter:GetDistanceAttenuation()))
emitter:SetDistanceAttenuation({ [10] = 1, [100] = 0.4, [300] = 0 })
log("curve", curve(emitter:GetDistanceAttenuation()))
emitter:SetDistanceAttenuation({ [5] = 2, [20] = -1 })
log("kept within 0 to 1", curve(emitter:GetDistanceAttenuation()))
log("string key", pcall(function()
	emitter:SetDistanceAttenuation({ a = 1 })
end))
emitter:SetDistanceAttenuation({})
log("cleared", curve(emitter:GetDistanceAttenuation()))
emitter:Destroy()

-- AudioPlayer playback.
p = Instance.new("AudioPlayer")
p.Asset = "rbxasset://sounds/action_falling.ogg" -- 10 s
for _, name in { "Ended", "Looped" } do
	p[name]:Connect(function(...)
		log("event", name, select("#", ...))
	end)
end
task.wait(0.3)
log("not loaded outside the DataModel", p.IsReady, p.TimeLength)
p.Parent = folder
for _ = 1, 100 do
	if p.IsReady then
		break
	end
	task.wait(0.05)
end
log("loaded", p.IsReady, p.TimeLength)
log("Play returns", p:Play())
log("playing", p.IsPlaying)
task.wait(0.5)
log("time advances", near(p.TimePosition, 0.5))
p:Play()
log("Play while playing keeps the place", near(p.TimePosition, 0.5))
p:Stop()
local stopped = p.TimePosition
log("Stop keeps the place", p.IsPlaying, near(stopped, 0.5))
task.wait(0.3)
log("stopped stays", p.TimePosition == stopped)
p:Play()
p.TimePosition = 9.8
task.wait(0.5)
log("ended", p.IsPlaying, p.TimePosition)
p.Looping = true
p.TimePosition = 9.7
p:Play()
task.wait(0.6)
log("looped", p.IsPlaying, near(p.TimePosition, 0.3))
p.Looping = false
p:Stop()
p.TimePosition = 20
p:Play()
task.wait(0.2)
log("played past the end", p.IsPlaying, p.TimePosition)
p.TimePosition = -3
p:Play()
task.wait(0.3)
log("played from -3", p.IsPlaying, near(p.TimePosition, 0.3))
p:Stop()
p.PlaybackSpeed = 0
p.TimePosition = 1
p:Play()
task.wait(0.3)
log("speed 0 holds", p.IsPlaying, p.TimePosition)
p:Stop()
log("Play(atTime)", pcall(function()
	p:Play(1)
end))
log("Stop(atTime)", pcall(function()
	p:Stop(1)
end))
log("Cancel", p:Cancel(1))
p:Destroy()

local bad = Instance.new("AudioPlayer")
bad.Asset = "rbxasset://sounds/nope.ogg"
bad.Parent = folder
task.wait(0.3)
bad:Play()
task.wait(0.2)
log("missing file", bad.IsReady, bad.TimeLength, bad.IsPlaying, bad.TimePosition)

local later = Instance.new("AudioPlayer")
later.AutoLoad = false
later.Asset = "rbxasset://sounds/oof.ogg"
later.Parent = folder
task.wait(0.3)
log("AutoLoad off", later.IsReady, later.TimeLength)
later:Play()
for _ = 1, 100 do
	if later.IsReady then
		break
	end
	task.wait(0.05)
end
log("loads on Play", later.IsReady, later.TimeLength)

-- Analyzers measure only on clients.
local analyzer = Instance.new("AudioAnalyzer", folder)
local feed = Instance.new("Wire", folder)
feed.SourceInstance = player
feed.TargetInstance = analyzer
player.Asset = "rbxasset://sounds/action_falling.ogg"
player.Looping = true
player:Play()
task.wait(0.5)
log("analyzer on a server", analyzer.RmsLevel, analyzer.PeakLevel, #analyzer:GetSpectrum(), analyzer.SpectrumEnabled, analyzer.WindowSize)
player:Stop()

folder:Destroy()
