--!strict
-- Small events on staggered timers, for `rook-player --bench --script`:
-- each should cost an update the size of what it changes, which the
-- bench's work counters show (docs/guide/getting-started.md#frame-times).
-- Runs as a server script; what it changes replicates to the client.
--
--   every frame  a part moves; a NumberValue nothing listens to changes
--   0.5 s        a PointLight turns on or off
--   0.75 s       a part is added, and the last one destroyed
--   1 s          a FillBall of Grass, then of Air, in the same place
--   always       a GUI Frame tweens back and forth; a looped Sound plays
--                at Volume 0

local Players = game:GetService("Players")
local ReplicatedStorage = game:GetService("ReplicatedStorage")
local RunService = game:GetService("RunService")
local TweenService = game:GetService("TweenService")

local spawn = workspace:FindFirstChildWhichIsA("SpawnLocation", true)
local origin = if spawn then spawn.Position + Vector3.new(0, 8, -16) else Vector3.new(0, 12, -16)

local folder = Instance.new("Folder")
folder.Name = "EventSoak"
folder.Parent = workspace

local mover = Instance.new("Part")
mover.Name = "Mover"
mover.Anchored = true
mover.Size = Vector3.new(2, 2, 2)
mover.Parent = folder

local lamp = Instance.new("Part")
lamp.Name = "Lamp"
lamp.Anchored = true
lamp.Size = Vector3.new(1, 1, 1)
lamp.Position = origin + Vector3.new(6, 0, 0)
lamp.Parent = folder
local light = Instance.new("PointLight")
light.Range = 16
light.Parent = lamp

local speaker = Instance.new("Part")
speaker.Name = "Speaker"
speaker.Anchored = true
speaker.Size = Vector3.new(1, 1, 1)
speaker.Position = origin + Vector3.new(-6, 0, 0)
speaker.Parent = folder
local sound = Instance.new("Sound")
sound.SoundId = "rbxasset://sounds/electronicpingshort.wav"
sound.Volume = 0
sound.Looped = true
sound.Parent = speaker
sound:Play()

local unheard = Instance.new("NumberValue")
unheard.Name = "Unheard"
unheard.Parent = ReplicatedStorage

-- A tween on a Frame in every player's GUI, and in StarterGui for players
-- who come later.
local function addGui(parent: Instance)
	local screen = Instance.new("ScreenGui")
	screen.Name = "EventSoak"
	screen.ResetOnSpawn = false
	local frame = Instance.new("Frame")
	frame.Size = UDim2.fromOffset(80, 40)
	frame.Position = UDim2.fromScale(0.1, 0.1)
	frame.Parent = screen
	screen.Parent = parent
	local tween = TweenService:Create(
		frame,
		TweenInfo.new(2, Enum.EasingStyle.Linear, Enum.EasingDirection.InOut, -1, true),
		{ Position = UDim2.fromScale(0.6, 0.1), BackgroundTransparency = 0.5 }
	)
	tween:Play()
end
addGui(game:GetService("StarterGui"))
local function addPlayerGui(player: Player)
	local gui = player:WaitForChild("PlayerGui")
	addGui(gui)
end
Players.PlayerAdded:Connect(addPlayerGui)
for _, player in Players:GetPlayers() do
	task.spawn(addPlayerGui, player)
end

local t = 0
RunService.Heartbeat:Connect(function(dt)
	t += dt
	mover.CFrame = CFrame.new(origin + Vector3.new(math.sin(t) * 4, 0, 0)) * CFrame.Angles(0, t, 0)
	unheard.Value = t
end)

task.spawn(function()
	while true do
		task.wait(0.5)
		light.Enabled = not light.Enabled
	end
end)

task.spawn(function()
	local last: Part? = nil
	while true do
		task.wait(0.75)
		if last then
			last:Destroy()
		end
		local part = Instance.new("Part")
		part.Anchored = true
		part.Size = Vector3.new(1, 1, 1)
		part.Position = origin + Vector3.new(0, 4, 0)
		part.Parent = folder
		last = part
	end
end)

task.spawn(function()
	local at = origin + Vector3.new(0, -4, 12)
	local grass = true
	while true do
		task.wait(1)
		workspace.Terrain:FillBall(at, 4, if grass then Enum.Material.Grass else Enum.Material.Air)
		grass = not grass
	end
end)
