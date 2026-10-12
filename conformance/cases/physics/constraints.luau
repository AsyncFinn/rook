-- Constraints and driven hinges, compared once they've settled: legacy
-- RotateV and RotateP from motor surfaces, HingeConstraint actuators, and
-- parts hanging from springs, ropes, rods and ball sockets.
-- Physics needs the parts in Workspace: they go far above the place, each
-- in its own spot, and are destroyed at the end.
local made = {}
local function part(x, y, anchored, size)
    local p = Instance.new("Part")
    p.Anchored = anchored
    p.Size = size
    p.CFrame = CFrame.new(x, 1000 + y, 0)
    for _, s in ipairs({ "Top", "Bottom", "Left", "Right", "Front", "Back" }) do
        p[s .. "Surface"] = Enum.SurfaceType.Smooth
    end
    p.Parent = workspace
    table.insert(made, p)
    return p
end
local function n(x, places)
    local s = string.format("%." .. (places or 2) .. "f", x)
    if tonumber(s) == 0 then
        s = string.format("%." .. (places or 2) .. "f", 0)
    end
    return s
end
local function yaw(base, wheel)
    local _, y = base.CFrame:ToObjectSpace(wheel.CFrame):ToEulerAnglesYXZ()
    return y
end
local function attach(p, cf)
    local a = Instance.new("Attachment")
    a.CFrame = cf
    a.Parent = p
    return a
end

-- Motor surfaces: the base's top surface drives the cube on it.
local function surface(x, kind, input, a, b)
    local base = part(x, 0, true, Vector3.new(4, 1, 4))
    local wheel = part(x, 1.5, false, Vector3.new(2, 2, 2))
    base.TopSurface = kind
    base.TopSurfaceInput = input
    base.TopParamA = a
    base.TopParamB = b
    wheel:MakeJoints()
    wheel:SetNetworkOwner(nil)
    return base, wheel, base:GetJoints()[1]
end
local vBase, vWheel, vJoint = surface(0, Enum.SurfaceType.Motor, Enum.InputType.Constant, 0, 0.5)
local pBase, pWheel, pJoint = surface(20, Enum.SurfaceType.SteppingMotor, Enum.InputType.Constant, 0, 1)
local hBase, hWheel = surface(40, Enum.SurfaceType.Motor, Enum.InputType.NoInput, 3, 2)
log("joints", vJoint.ClassName, pJoint.ClassName)

-- HingeConstraints turning about world Y.
local up = CFrame.Angles(0, 0, math.pi / 2)
local function hinge(x, props)
    local base = part(x, 0, true, Vector3.new(4, 1, 4))
    local wheel = part(x, 3, false, Vector3.new(2, 2, 2))
    wheel:SetNetworkOwner(nil)
    local h = Instance.new("HingeConstraint")
    h.Attachment0 = attach(base, CFrame.new(0, 1.5, 0) * up)
    h.Attachment1 = attach(wheel, CFrame.new(0, -1.5, 0) * up)
    for k, v in pairs(props) do
        h[k] = v
    end
    h.Parent = base
    return base, wheel, h
end
local _, motorWheel = hinge(60, { ActuatorType = Enum.ActuatorType.Motor, AngularVelocity = 5, MotorMaxTorque = 1e9 })
local servoBase, servoWheel, servo = hinge(80, {
    ActuatorType = Enum.ActuatorType.Servo,
    TargetAngle = 90,
    AngularSpeed = 2,
    ServoMaxTorque = 1e9,
})
local _, limitWheel, limited = hinge(100, {
    ActuatorType = Enum.ActuatorType.Servo,
    TargetAngle = 90,
    AngularSpeed = 2,
    ServoMaxTorque = 1e9,
    LimitsEnabled = true,
    UpperAngle = 30,
    LowerAngle = -30,
})

-- A cube hanging below an anchored base, 3.5 studs between attachments.
local function hang(x, class, props)
    local base = part(x, 0, true, Vector3.new(4, 1, 4))
    local w = part(x, -5, false, Vector3.new(2, 2, 2))
    w:SetNetworkOwner(nil)
    local c = Instance.new(class)
    c.Attachment0 = attach(base, CFrame.new(0, -0.5, 0))
    c.Attachment1 = attach(w, CFrame.new(0, 1, 0))
    for k, v in pairs(props) do
        c[k] = v
    end
    c.Parent = base
    return w, c
end
local spring = hang(120, "SpringConstraint", { Stiffness = 1098.72, Damping = 50, FreeLength = 2 })
local rope, ropeC = hang(140, "RopeConstraint", { Length = 6 })
local rod = hang(160, "RodConstraint", { Length = 2 })
local ball = hang(180, "BallSocketConstraint", {})

task.wait(1.5)
log("RotateV", n(vWheel.AssemblyAngularVelocity.Y, 1))
log("RotateP", n(yaw(pBase, pWheel)))
log("NoInput", n(yaw(hBase, hWheel)))
log("motor", n(motorWheel.AssemblyAngularVelocity.Y, 1))
log("servo", n(servo.CurrentAngle, 0), n(math.deg(yaw(servoBase, servoWheel)), 0))
log("limited", math.abs(limited.CurrentAngle - 30) < 2)

-- Live surface changes apply.
pBase.TopParamB = -0.5
task.wait(3)
log("RotateP changed", n(yaw(pBase, pWheel)))
local function below(w)
    return n(w.Position.Y - 1000, 1)
end
log("spring", below(spring))
log("rope", (ropeC.Attachment0.WorldPosition - ropeC.Attachment1.WorldPosition).Magnitude <= 6.1, rope.Position.Y < 1000 - 7)
log("rod", below(rod))
log("ball socket", below(ball))

for _, p in ipairs(made) do
    p:Destroy()
end
