-- Tweens: TweenService:Create, Tween:Play/Pause/Cancel, TweenService:GetValue
-- and the legacy GuiObject:TweenPosition/TweenSize/TweenSizeAndPosition.
--
-- Behavior follows creator-docs (TweenService, TweenInfo, EasingStyle,
-- EasingDirection). Easing curves are the standard formulas; their exact
-- constants are Inference until checked against TweenService:GetValue in
-- Studio. Tweens advance on Heartbeat (Inference).

local std = ...
local set_internal, fire, new_instance = std.set_internal, std.fire, std.new_instance
local error, typeof, type, pairs, setmetatable, math = std.error, std.typeof, std.type, std.pairs, std.setmetatable, std.math
local heartbeat = std.heartbeat

local PI = math.pi
local sin, cos, sqrt = math.sin, math.cos, math.sqrt

-- Easing "In" curves by Enum.EasingStyle value.
local IN = {
	[0] = function(t) return t end, -- Linear
	[1] = function(t) return 1 - cos(t * PI / 2) end, -- Sine
	[2] = function(t) -- Back
		local s = 1.70158
		return t * t * ((s + 1) * t - s)
	end,
	[3] = function(t) return t * t end, -- Quad
	[4] = function(t) return t ^ 4 end, -- Quart
	[5] = function(t) return t ^ 5 end, -- Quint
	[6] = function(t) -- Bounce
		local u = 1 - t
		local v
		if u < 1 / 2.75 then
			v = 7.5625 * u * u
		elseif u < 2 / 2.75 then
			u -= 1.5 / 2.75
			v = 7.5625 * u * u + 0.75
		elseif u < 2.5 / 2.75 then
			u -= 2.25 / 2.75
			v = 7.5625 * u * u + 0.9375
		else
			u -= 2.625 / 2.75
			v = 7.5625 * u * u + 0.984375
		end
		return 1 - v
	end,
	[7] = function(t) -- Elastic
		if t == 0 or t == 1 then return t end
		return -(2 ^ (10 * (t - 1))) * sin((t - 1.075) * (2 * PI) / 0.3)
	end,
	[8] = function(t) -- Exponential
		if t == 0 then return 0 end
		return 2 ^ (10 * (t - 1))
	end,
	[9] = function(t) return 1 - sqrt(1 - t * t) end, -- Circular
	[10] = function(t) return t * t * t end, -- Cubic
}

local function ease(style, direction, t)
	local f = IN[style] or IN[0]
	if direction == 0 then
		return f(t)
	elseif direction == 1 then
		return 1 - f(1 - t)
	end
	if t < 0.5 then
		return f(t * 2) / 2
	end
	return 1 - f(2 - t * 2) / 2
end

local function lerp(a, b, t)
	local kind = typeof(a)
	if kind == "number" then
		return a + (b - a) * t
	elseif kind == "boolean" then
		return if t >= 1 then b else a
	elseif kind == "UDim" then
		return UDim.new(a.Scale + (b.Scale - a.Scale) * t, math.round(a.Offset + (b.Offset - a.Offset) * t))
	elseif kind == "Rect" then
		return Rect.new(a.Min:Lerp(b.Min, t), a.Max:Lerp(b.Max, t))
	elseif kind == "NumberRange" then
		return NumberRange.new(a.Min + (b.Min - a.Min) * t, a.Max + (b.Max - a.Max) * t)
	end
	return a:Lerp(b, t)
end

local TWEENABLE = {
	number = true, boolean = true, Vector3 = true, Vector2 = true, CFrame = true,
	Color3 = true, UDim = true, UDim2 = true, Rect = true, NumberRange = true,
}

local states = setmetatable({}, { __mode = "k" })
-- target -> property -> state, so a newer tween on a property cancels the older one.
local owners = setmetatable({}, { __mode = "k" })

local PlaybackState = Enum.PlaybackState

local function setState(s, state)
	s.state = state
	if s.tween then
		set_internal(s.tween, "PlaybackState", state)
	end
end

local function stop(s)
	if s.conn then
		s.conn:Disconnect()
		s.conn = nil
	end
	for name in s.goals do
		local byProp = owners[s.target]
		if byProp and byProp[name] == s then
			byProp[name] = nil
		end
	end
end

local function finish(s, state)
	stop(s)
	setState(s, state)
	if s.tween then
		fire(s.tween, "Completed", state)
	end
	if s.done then
		s.done(state)
	end
end

local function step(s, dt)
	s.elapsed += dt
	local info = s.info
	if s.state == PlaybackState.Delayed then
		if s.elapsed < info.DelayTime then
			return
		end
		s.elapsed -= info.DelayTime
		setState(s, PlaybackState.Playing)
	end
	local time = info.Time
	local raw = if time > 0 then math.min(s.elapsed / time, 1) else 1
	local alpha = if s.reversing then 1 - raw else raw
	local eased = ease(info.EasingStyle.Value, info.EasingDirection.Value, alpha)
	for name, goal in s.goals do
		s.target[name] = lerp(s.starts[name], goal, eased)
	end
	if raw < 1 then
		return
	end
	-- One pass finished: reverse, repeat, or complete.
	s.elapsed = 0
	if info.Reverses and not s.reversing then
		s.reversing = true
		return
	end
	s.reversing = false
	if info.RepeatCount < 0 or s.repeats < info.RepeatCount then
		s.repeats += 1
		if info.DelayTime > 0 then
			setState(s, PlaybackState.Delayed)
		end
		return
	end
	finish(s, PlaybackState.Completed)
end

local function play(s)
	if s.state == PlaybackState.Playing or s.state == PlaybackState.Delayed then
		return
	end
	if s.state ~= PlaybackState.Paused then
		-- Start values are read when the tween starts playing.
		s.starts = {}
		for name in s.goals do
			s.starts[name] = s.target[name]
		end
		s.elapsed, s.repeats, s.reversing = 0, 0, false
	end
	local byProp = owners[s.target] or {}
	owners[s.target] = byProp
	for name in s.goals do
		local other = byProp[name]
		if other and other ~= s then
			finish(other, PlaybackState.Canceled)
		end
		byProp[name] = s
	end
	setState(s, if s.info.DelayTime > 0 and s.elapsed == 0 and s.state ~= PlaybackState.Paused
		then PlaybackState.Delayed
		else PlaybackState.Playing)
	s.conn = heartbeat:Connect(function(dt)
		step(s, dt)
	end)
end

local tween = {}

function tween.create(instance, info, goals)
	if typeof(instance) ~= "Instance" then
		error("Argument 1 missing or nil", 0)
	end
	if typeof(info) ~= "TweenInfo" then
		error("Argument 2 missing or nil", 0)
	end
	if type(goals) ~= "table" then
		error("Argument 3 missing or nil", 0)
	end
	local copy = {}
	for name, goal in pairs(goals) do
		local current = instance[name]
		if not TWEENABLE[typeof(goal)] or typeof(current) ~= typeof(goal) then
			error(("TweenService:Create property named '%s' cannot be tweened due to type mismatch (property is a '%s', but given type is '%s')"):format(
				tostring(name), typeof(current), typeof(goal)), 0)
		end
		copy[name] = goal
	end
	local t = new_instance("Tween")
	set_internal(t, "Instance", instance)
	set_internal(t, "TweenInfo", info)
	states[t] = { tween = t, target = instance, info = info, goals = copy, state = PlaybackState.Begin }
	return t
end

function tween.play(t)
	local s = states[t]
	if s then play(s) end
end

-- Only a Playing tween pauses; a Delayed one carries on (Confirmed, Studio
-- recording 2026-10-17, and creator-docs TweenBase:Pause).
function tween.pause(t)
	local s = states[t]
	if s and s.state == PlaybackState.Playing then
		stop(s)
		setState(s, PlaybackState.Paused)
	end
end

function tween.cancel(t)
	local s = states[t]
	if s and s.state ~= PlaybackState.Completed and s.state ~= PlaybackState.Canceled then
		finish(s, PlaybackState.Canceled)
		s.elapsed = 0
	end
end

function tween.getValue(alpha, style, direction)
	return ease(style.Value, direction.Value, math.clamp(alpha, 0, 1))
end

-- Legacy GUI tweens. Returns whether the tween started; callback gets
-- Enum.TweenStatus (creator-docs GuiObject:TweenPosition).
local guiTweens = setmetatable({}, { __mode = "k" })

function tween.gui(object, goals, direction, style, time, override, callback)
	local running = guiTweens[object]
	if running and running.state == PlaybackState.Playing then
		if not override then
			return false
		end
		finish(running, PlaybackState.Canceled)
	end
	local info = TweenInfo.new(time or 1, style or Enum.EasingStyle.Quad, direction or Enum.EasingDirection.Out)
	local s = { target = object, info = info, goals = goals, state = PlaybackState.Begin }
	s.done = function(state)
		if guiTweens[object] == s then
			guiTweens[object] = nil
		end
		if callback then
			task.spawn(callback, if state == PlaybackState.Completed
				then Enum.TweenStatus.Completed
				else Enum.TweenStatus.Canceled)
		end
	end
	guiTweens[object] = s
	play(s)
	return true
end

return tween
