-- The player's settings as the menu shows and changes them, through
-- UserSettings().GameSettings and SoundService (docs/behavior/user-settings.md).
-- No UI here. Each read and write goes through pcall: a setting the
-- engine lacks reads as its default instead of stopping the menu.

local GuiService = game:GetService("GuiService")
local SoundService = game:GetService("SoundService")

local GameSettings = UserSettings().GameSettings

local PlayerSettings = {}

local function read(get, default)
	local ok, value = pcall(get)
	if ok and value ~= nil then
		return value
	end
	return default
end

local function write(set)
	local ok, err = pcall(set)
	if not ok then
		warn("Rook's CoreScripts: " .. tostring(err))
	end
end

PlayerSettings.read = read
PlayerSettings.write = write

-- Master volume, 0 to 1.
function PlayerSettings.volume(): number
	return read(function()
		return GameSettings.MasterVolume
	end, 1)
end

function PlayerSettings.setVolume(v: number)
	write(function()
		GameSettings.MasterVolume = math.clamp(v, 0, 1)
	end)
end

-- The output devices as SoundService lists them: `names` to show (the
-- system's default first), `raw` and `ids` to choose with, and the index
-- of the chosen one.
function PlayerSettings.outputDevices()
	local ok, names, ids, chosen = pcall(function()
		return SoundService:GetOutputDevices()
	end)
	if not ok or type(names) ~= "table" or #names == 0 then
		return { names = { "Default" }, raw = { "" }, ids = { "" }, chosen = 1 }
	end
	local shown = table.clone(names)
	if ids[1] == "" then
		shown[1] = "Default (" .. names[1] .. ")"
	end
	return { names = shown, raw = names, ids = ids, chosen = chosen or 1 }
end

function PlayerSettings.setOutputDevice(devices, at: number)
	write(function()
		SoundService:SetOutputDevice(devices.raw[at] or "", devices.ids[at] or "")
	end)
end

function PlayerSettings.fullscreen(): boolean
	return read(function()
		return GameSettings.Fullscreen
	end, false)
end

function PlayerSettings.setFullscreen(on: boolean)
	if PlayerSettings.fullscreen() ~= on then
		write(function()
			GuiService:ToggleFullscreen()
		end)
	end
end

-- -1 is the display's rate and 0 none (docs/behavior/user-settings.md).
function PlayerSettings.framerateCap(): number
	return read(function()
		return GameSettings.FramerateCap
	end, -1)
end

function PlayerSettings.setFramerateCap(cap: number)
	write(function()
		GameSettings.FramerateCap = cap
	end)
end

function PlayerSettings.cameraSensitivity(): number
	return read(function()
		return GameSettings.MouseSensitivityThirdPerson.X
	end, 1)
end

function PlayerSettings.setCameraSensitivity(v: number)
	write(function()
		GameSettings.MouseSensitivity = v
		GameSettings.MouseSensitivityFirstPerson = Vector2.new(v, v)
		GameSettings.MouseSensitivityThirdPerson = Vector2.new(v, v)
	end)
end

function PlayerSettings.cameraInverted(): boolean
	return read(function()
		return GameSettings.CameraYInverted
	end, false)
end

function PlayerSettings.setCameraInverted(on: boolean)
	write(function()
		GameSettings.CameraYInverted = on
	end)
end

-- Calls `f` when anything in GameSettings changes; returns the
-- connection, or nil.
function PlayerSettings.onChanged(f: () -> ())
	local ok, connection = pcall(function()
		return GameSettings.Changed:Connect(f)
	end)
	return ok and connection or nil
end

return PlayerSettings
