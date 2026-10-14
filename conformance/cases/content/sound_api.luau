-- Sound's methods, events and property rules: where Play starts, what
-- Stop, Pause and Resume do in each state, ends, loops and seeks.
local s = Instance.new("Sound")
s.SoundId = "rbxasset://sounds/action_falling.ogg" -- 10 s
-- Whether Loaded comes before or after a Play right after parenting
-- depends on Studio's cache, so it's only checked to have fired.
local loaded
s.Loaded:Connect(function(id)
    loaded = id
end)
local function near(x, target)
    return math.abs(x - target) < 0.12
end
local function state(tag)
    log(tag, s.Playing, s.IsPlaying, s.IsPaused)
end

s.Parent = workspace
for _ = 1, 100 do
    if s.IsLoaded then
        break
    end
    task.wait(0.05)
end
log("Loaded fired with", loaded)
for _, name in { "Played", "Paused", "Resumed", "Stopped", "Ended", "Loaded", "DidLoop" } do
    s[name]:Connect(function(...)
        log("event", name, ...)
    end)
end
state("before Play")
s:Play()
state("played")
task.wait(0.5)
log("time advances", near(s.TimePosition, 0.5))

-- Play while playing restarts, without Played.
s:Play()
log("restart", near(s.TimePosition, 0))
task.wait(0.5)

-- Pause keeps the place; Pause again and Stop on a paused sound do nothing.
s:Pause()
local paused = s.TimePosition
state("paused")
s:Pause()
s:Stop()
log("stop while paused keeps place", s.TimePosition == paused)
s:Resume()
log("resume from place", near(s.TimePosition, paused))
s:Resume()

-- Play starts where a script last set TimePosition, even after Pause.
s.TimePosition = 3
task.wait(0.3)
s:Pause()
s:Play()
log("play from last set", near(s.TimePosition, 3))
s:Stop()
state("stopped")
log("stop rewinds", s.TimePosition)
s:Play()
log("play after stop", near(s.TimePosition, 0))

-- Setting Playing directly fires no events and keeps the place.
task.wait(0.3)
s.Playing = false
local t = s.TimePosition
s.Playing = true
log("Playing set directly", near(s.TimePosition, t))

-- A seek past the end ends it; a negative one plays from the start.
s.TimePosition = 20
task.wait(0.2)
log("seek past end", s.Playing, s.TimePosition)
s:Play()
s.TimePosition = -1
log("negative reads", s.TimePosition)
task.wait(0.3)
log("negative plays from 0", near(s.TimePosition, 0.3))

-- The end: Ended, stopped and rewound; Play then starts where a script
-- last set the position.
s.TimePosition = 9.7
task.wait(0.6)
log("ended", s.Playing, s.TimePosition)
s:Play()
log("play after end", near(s.TimePosition, 9.7))
s:Stop()

-- Looped: DidLoop instead of Ended.
s.Looped = true
s.TimePosition = 9.8
s:Play()
task.wait(0.5)
log("looped", s.Playing, near(s.TimePosition, 0.3))
s.Looped = false
s:Stop()

-- PlaybackSpeed scales time; Volume and PlaybackSpeed are clamped.
s.PlaybackSpeed = 2
s.TimePosition = 1
s:Play()
task.wait(1)
-- Twice as far as the 1 s waited (wait overshoots, doubled here).
log("double speed", s.TimePosition > 2.8 and s.TimePosition < 3.3)
s:Stop()
for _, v in { -1, 25, math.huge } do
    s.PlaybackSpeed = v
    log("PlaybackSpeed", v, s.PlaybackSpeed, s.Pitch)
end
for _, v in { -1, 11 } do
    s.Volume = v
    log("Volume", v, s.Volume)
end
log("server PlaybackLoudness", s.PlaybackLoudness)
s:Destroy()
