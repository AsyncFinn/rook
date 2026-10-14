-- What scripts see of 3D sound: roll-off defaults, the deprecated
-- EmitterSize and MaxDistance, limits, SoundService's settings and the
-- listener.
local s = Instance.new("Sound")
log("defaults", s.RollOffMinDistance, s.RollOffMaxDistance, s.RollOffMode, s.EmitterSize, s.MaxDistance)
s.EmitterSize = 7
log("EmitterSize sets min", s.RollOffMinDistance)
s.MaxDistance = 50
log("MaxDistance sets max", s.RollOffMaxDistance)
s.RollOffMinDistance = -5
log("negative min", s.RollOffMinDistance)
s.RollOffMaxDistance = 3
s.RollOffMinDistance = 20
log("min past max kept", s.RollOffMinDistance, s.RollOffMaxDistance)
s:Destroy()

local SS = game:GetService("SoundService")
log("SoundService", SS.RolloffScale, string.format("%.2f", SS.DistanceFactor), SS.DopplerScale)
log("listener", SS:GetListener())
SS:SetListener(Enum.ListenerType.CFrame, CFrame.new(1, 2, 3))
log("CFrame listener", SS:GetListener())
local p = Instance.new("Part")
SS:SetListener(Enum.ListenerType.ObjectCFrame, p)
local kind, object = SS:GetListener()
log("object listener", kind, object == p)
SS:SetListener(Enum.ListenerType.Camera)
log("back to camera", SS:GetListener())
p:Destroy()
