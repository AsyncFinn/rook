-- TextChatService's messages (creator-docs "In-experience text chat").
-- A client's SendAsync announces the message (SendingMessage) and sends
-- it to the server through TextChatService; the server checks the
-- sender's TextSource may send, fires Player.Chatted, and delivers it to
-- every client (or those ShouldDeliverCallback lets through), where
-- OnIncomingMessage (the channel's, then the service's) may restyle it
-- and MessageReceived fires on the channel and the service. Messages
-- aren't filtered: a local server has no one to filter for.
local std = ...
local game, Enum, DateTime = std.game, std.Enum, std.DateTime
local pcall, ipairs = std.pcall, std.ipairs
local new_instance, set, fire, get_callback, send =
	std.new_instance, std.set_internal, std.fire, std.get_callback, std.chat_send

local chat = game:GetService("TextChatService")
local players = game:GetService("Players")

local api = {}

local count = 0
local function newId()
	count += 1
	return string.format("%08x-%04x", math.random(0, 0x7fffffff), count)
end

local function sourceFor(channel, userId)
	for _, child in ipairs(channel:GetChildren()) do
		if child:IsA("TextSource") and child.UserId == userId then
			return child
		end
	end
	return nil
end

local function message(channel, source, text, metadata, id, status, millis)
	local m = new_instance("TextChatMessage")
	set(m, "Text", text)
	set(m, "TextChannel", channel)
	set(m, "TextSource", source)
	set(m, "MessageId", id)
	set(m, "Metadata", metadata or "")
	-- Inference: Roblox's chat window shows the speaker's name only
	-- through PrefixText, as "Name:".
	local player = source and players:GetPlayerByUserId(source.UserId)
	set(m, "PrefixText", player and player.DisplayName .. ":" or "")
	set(m, "Status", status)
	set(m, "Timestamp", DateTime.fromUnixTimestampMillis(millis or DateTime.now().UnixTimestampMillis))
	return m
end

-- The channel's OnIncomingMessage, then the service's, may give
-- TextChatMessageProperties whose PrefixText and Text replace the
-- message's (creator-docs TextChannel.OnIncomingMessage).
local function receive(channel, m)
	for _, owner in ipairs({ channel, chat }) do
		local callback = get_callback(owner, "OnIncomingMessage")
		if callback then
			local ok, props = pcall(callback, m)
			if ok and typeof(props) == "Instance" then
				if props.PrefixText ~= "" then
					set(m, "PrefixText", props.PrefixText)
				end
				if props.Text ~= "" then
					set(m, "Text", props.Text)
				end
			end
		end
	end
	fire(channel, "MessageReceived", m)
	fire(chat, "MessageReceived", m)
end

function api.sendAsync(channel, text, metadata)
	local player = players.LocalPlayer
	local source = player and sourceFor(channel, player.UserId)
	local id = newId()
	local m = message(channel, source, text, metadata, id, Enum.TextChatMessageStatus.Sending)
	fire(chat, "SendingMessage", m)
	if source and source.CanSend then
		send("server", channel, text, metadata or "", id)
	else
		set(m, "Status", Enum.TextChatMessageStatus.Unknown)
	end
	return m
end

function api.displaySystemMessage(channel, text, metadata)
	local m = message(channel, nil, text, metadata, newId(), Enum.TextChatMessageStatus.Success)
	receive(channel, m)
	return m
end

function api.addUserAsync(channel, userId)
	local found = sourceFor(channel, userId)
	if found then
		return found, true
	end
	local player = players:GetPlayerByUserId(userId)
	local source = new_instance("TextSource")
	source.Name = player and player.Name or tostring(userId)
	set(source, "UserId", userId)
	set(source, "CanSend", true)
	source.Parent = channel
	return source, true
end

-- Server: a player's message arriving.
function api.onServer(player, channel, text, metadata, id)
	if typeof(channel) ~= "Instance" or not channel:IsA("TextChannel") or typeof(text) ~= "string" then
		return
	end
	local source = sourceFor(channel, player.UserId)
	if not source or not source.CanSend then
		return
	end
	fire(player, "Chatted", text)
	local millis = DateTime.now().UnixTimestampMillis
	local deliver = get_callback(channel, "ShouldDeliverCallback")
	if not deliver then
		send("all", channel, player.UserId, text, metadata, id, millis)
		return
	end
	local m = message(channel, source, text, metadata, id, Enum.TextChatMessageStatus.Success, millis)
	for _, target in ipairs(channel:GetChildren()) do
		if target:IsA("TextSource") then
			local recipient = players:GetPlayerByUserId(target.UserId)
			local ok, yes = pcall(deliver, m, target)
			if recipient and ok and yes ~= false then
				send(recipient, channel, player.UserId, text, metadata, id, millis)
			end
		end
	end
end

-- Client: a message the server delivered.
function api.onClient(channel, userId, text, metadata, id, millis)
	if typeof(channel) ~= "Instance" then
		return
	end
	local source = sourceFor(channel, userId)
	receive(channel, message(channel, source, text, metadata, id, Enum.TextChatMessageStatus.Success, millis))
end

-- Server: everyone may chat in the default channels.
function api.playerAdded(player)
	local folder = chat:FindFirstChild("TextChannels")
	if not folder then
		return
	end
	for _, name in ipairs({ "RBXGeneral", "RBXSystem" }) do
		local channel = folder:FindFirstChild(name)
		if channel then
			api.addUserAsync(channel, player.UserId)
		end
	end
end

return api
