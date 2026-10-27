-- Where LoadCharacter puts the player's character: on the one spawn it may
-- use (enabled, neutral or its team's, anywhere in Workspace), or 100 studs
-- above the origin when there's none; the way it faces, and the ForceField
-- from Duration. Heights are to the nearest 10 studs, since they depend on
-- the avatar. The place's own spawns are disabled while it runs.
--! player
local Players = game:GetService("Players")
local Teams = game:GetService("Teams")
local player = Players:GetPlayers()[1]

local restore = {}
for _, d in workspace:GetDescendants() do
	if d:IsA("SpawnLocation") and d.Enabled then
		d.Enabled = false
		table.insert(restore, d)
	end
end

local made = {}
local function spawn(name, x, parent)
	local s = Instance.new("SpawnLocation")
	s.Name = name
	s.Anchored = true
	s.Size = Vector3.new(6, 1, 6)
	s.CFrame = CFrame.new(x, 20, 300)
	s.Parent = parent or workspace
	table.insert(made, s)
	return s
end
local function team(name, color)
	local t = Instance.new("Team")
	t.Name = name
	t.TeamColor = BrickColor.new(color)
	t.AutoAssignable = false
	t.Parent = Teams
	table.insert(made, t)
	return t
end

local function where(label)
	player:LoadCharacter()
	local character = player.Character
	local root = character:WaitForChild("HumanoidRootPart")
	local on = "none"
	for _, s in made do
		if s:IsA("SpawnLocation") then
			local p = s.CFrame:PointToObjectSpace(root.Position)
			if math.abs(p.X) <= s.Size.X / 2 and math.abs(p.Z) <= s.Size.Z / 2 and p.Y > 0 then
				on = s.Name
			end
		end
	end
	local look = root.CFrame.LookVector
	-- Adding 0 turns -0 into 0.
	local function whole(v, step)
		return math.round(v / step) * step + 0
	end
	log(
		label,
		"on",
		on,
		"y",
		whole(root.Position.Y, 10),
		"x",
		whole(root.Position.X, 1),
		"z",
		whole(root.Position.Z, 1),
		"look",
		whole(look.X, 1),
		whole(look.Z, 1),
		"forcefield",
		character:FindFirstChildOfClass("ForceField") ~= nil
	)
end

local ok, err = pcall(function()
	where("no spawns")

	local a = spawn("A", 0)
	where("one neutral")

	a.Enabled = false
	where("only disabled")
	a.Enabled = true

	a.Duration = 0
	a.CFrame = CFrame.new(0, 20, 300) * CFrame.Angles(0, math.rad(90), 0)
	where("turned, no duration")
	a.Duration = 10

	local holder = Instance.new("Model")
	holder.Parent = workspace
	table.insert(made, holder)
	a.Parent = holder
	where("inside a model")

	a.Parent = game:GetService("ReplicatedStorage")
	where("in storage")
	a.Parent = workspace

	local red = team("Red", "Bright red")
	local blue = team("Blue", "Bright blue")
	a.Neutral = false
	a.TeamColor = red.TeamColor
	where("neutral player, red spawn")

	player.Team = red
	where("red player, red spawn")

	local b = spawn("B", 20)
	b.Neutral = false
	b.TeamColor = blue.TeamColor
	a.Enabled = false
	where("red player, blue spawn")

	b.Neutral = true
	where("red player, neutral spawn")
end)
if not ok then
	log("ERROR", err)
end

player.Neutral = true
for i = #made, 1, -1 do
	made[i]:Destroy()
end
for _, s in restore do
	s.Enabled = true
end
player:LoadCharacter()
