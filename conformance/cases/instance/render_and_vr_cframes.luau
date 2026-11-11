-- Part:GetRenderCFrame, and VRService's tracked points without a headset.
local p = Instance.new("Part")
p.CFrame = CFrame.new(1, 2, 3) * CFrame.Angles(0.1, 0.2, 0.3)
log("render unparented", p:GetRenderCFrame() == p.CFrame)
log("render", pcall(function()
    return p.GetRenderCFrame()
end))
p:Destroy()
local vr = game:GetService("VRService")
for _, t in { Enum.UserCFrame.Head, Enum.UserCFrame.LeftHand, Enum.UserCFrame.Floor } do
    log(t.Name, vr:GetUserCFrame(t), vr:GetUserCFrameEnabled(t))
end
log("by name", vr:GetUserCFrame("RightHand"))
log("missing", pcall(function()
    return vr:GetUserCFrame()
end))
log("missing enabled", pcall(function()
    return vr:GetUserCFrameEnabled()
end))
