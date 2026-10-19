-- Whether mouse movement reuses one InputObject across InputChanged events,
-- and whether a kept object's Position and Delta follow later moves.
--
-- Mouse input only reaches clients, so this runs on the playtest client.
--! client mouse 300,300 340,320 400,350 420,380
local UserInputService = game:GetService("UserInputService")
local RunService = game:GetService("RunService")

local kept = {}
local snapshots = {}
local connection = UserInputService.InputChanged:Connect(function(input)
	if input.UserInputType ~= Enum.UserInputType.MouseMovement or #kept >= 3 then
		return
	end
	table.insert(kept, input)
	table.insert(snapshots, { input.Position, input.Delta, input.UserInputState })
end)

local start = os.clock()
while #kept < 3 and os.clock() - start < 10 do
	RunService.Heartbeat:Wait()
end
connection:Disconnect()

log("client", RunService:IsClient(), "moves", #kept)
if #kept >= 2 then
	log("same object 1-2", rawequal(kept[1], kept[2]))
	log("same object 2-3", kept[3] ~= nil and rawequal(kept[2], kept[3]))
	-- If the object is shared, the first one now shows the last move.
	local last = snapshots[#snapshots]
	log("first follows last position", kept[1].Position == last[1])
	log("first follows last delta", kept[1].Delta == last[2])
	log("first changed since its event", kept[1].Position ~= snapshots[1][1])
	log("state", snapshots[1][3], kept[1].UserInputState)
	log("parent", kept[1].Parent)
end
