-- A player's Team, TeamColor and Neutral keep each other in step: setting
-- one sets the others, TeamColor picking the first team of that color; a
-- color no team has; a team recolored, taken out of Teams or destroyed
-- while players are on it; Team:GetPlayers. The player starts neutral, on
-- White. The order of the Changed signals isn't checked: Studio fires
-- TeamColor twice on joining a team, an engine detail scripts don't use.
--! player
local Teams = game:GetService("Teams")
local player = game:GetService("Players"):GetPlayers()[1]

local made = {}
local function team(name, color)
	local t = Instance.new("Team")
	t.Name = name
	t.TeamColor = BrickColor.new(color)
	t.AutoAssignable = false
	t.Parent = Teams
	table.insert(made, t)
	return t
end

local function state(label)
	local t = player.Team
	log(label, "team", t and t.Name or "nil", "color", player.TeamColor, "neutral", player.Neutral)
end

player.Neutral = true
player.TeamColor = BrickColor.new("White")
local ok, err = pcall(function()
	local red = team("Red", "Bright red")
	local red2 = team("Red2", "Bright red")
	local blue = team("Blue", "Bright blue")
	state("start")

	player.Team = blue
	state("set Team")
	log("blue players", #blue:GetPlayers(), "red players", #red:GetPlayers())

	player.TeamColor = BrickColor.new("Bright red")
	state("set TeamColor to a shared color")

	player.Team = red2
	state("set Team to the second of a color")
	task.wait()
	state("a frame later")

	player.TeamColor = BrickColor.new("Bright red")
	state("same color again")

	player.TeamColor = BrickColor.new("Lime green")
	state("color no team has")

	player.Team = blue
	player.Neutral = true
	state("set Neutral")

	player.Neutral = false
	state("not Neutral again")

	blue.TeamColor = BrickColor.new("Bright yellow")
	task.wait()
	state("team recolored")

	player.Team = blue
	state("rejoined")
	blue:Destroy()
	task.wait()
	state("team destroyed")

	player.Team = red
	red.Parent = nil
	task.wait()
	state("team unparented")
	log("red2 players", #red2:GetPlayers())
	log("unparented players", pcall(red.GetPlayers, red))
end)
if not ok then
	log("ERROR", err)
end

player.Neutral = true
for _, t in made do
	t:Destroy()
end
