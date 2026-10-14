-- Sounds load once they're in the DataModel, report their TimeLength, and
-- a sound that never loads stays "playing" without time passing.
local function sound(id, parent)
    local s = Instance.new("Sound")
    s.SoundId = id
    s.Parent = parent
    return s
end

local ogg = sound("rbxasset://sounds/oof.ogg", workspace)
local mp3 = sound("rbxasset://sounds/action_jump.mp3", workspace)
local loose = sound("rbxasset://sounds/oof.ogg", nil)
local missing = sound("rbxasset://sounds/no_such_sound.wav", workspace)
log("before loading", ogg.IsLoaded, ogg.TimeLength)

for _ = 1, 100 do
    if ogg.IsLoaded and mp3.IsLoaded then
        break
    end
    task.wait(0.05)
end
log("ogg", ogg.IsLoaded, string.format("%.6f", ogg.TimeLength))
log("mp3", mp3.IsLoaded, string.format("%.6f", mp3.TimeLength))
log("unparented", loose.IsLoaded, loose.TimeLength)

missing:Play()
mp3:Play()
task.wait(0.15)
log("missing", missing.IsLoaded, missing.Playing, missing.TimePosition)
log("mp3 playing", mp3.Playing, mp3.TimePosition > 0.05)
task.wait(0.5)
-- Past its end (0.34 s): stopped and rewound.
log("mp3 ended", mp3.Playing, mp3.TimePosition)

for _, s in { ogg, mp3, loose, missing } do
    s:Destroy()
end
