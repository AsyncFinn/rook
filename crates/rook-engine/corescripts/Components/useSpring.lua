-- A number that springs to its target instead of jumping, as Roblox's
-- top bar icons grow and shrink. The spring is critically damped: it
-- reaches the target without overshooting. `frequency` is in cycles a
-- second. A target that changes mid-way keeps the motion's speed.
--
-- Returns a binding to set a property from, such as
-- `Size = size:map(function(v) return UDim2.fromOffset(v, v) end)`.

local CorePackages = game:GetService("CorePackages")
local RunService = game:GetService("RunService")
local React = require(CorePackages.Packages.React)

-- Closer than this, in value and in speed, the spring stops on its target.
local REST = 0.01

return function(target: number, frequency: number)
	local value, setValue = React.useBinding(target)
	local motion = React.useRef({ value = target, velocity = 0 })

	React.useEffect(function()
		local m = motion.current
		if m.value == target and m.velocity == 0 then
			return nil
		end
		local w = 2 * math.pi * frequency
		local connection
		connection = RunService.Heartbeat:Connect(function(dt)
			-- The critically damped spring's motion over dt, exactly.
			local offset = m.value - target
			local c = m.velocity + w * offset
			local decay = math.exp(-w * dt)
			offset = (offset + c * dt) * decay
			m.velocity = (m.velocity - w * c * dt) * decay
			if math.abs(offset) < REST and math.abs(m.velocity) < REST then
				offset, m.velocity = 0, 0
				connection:Disconnect()
			end
			m.value = target + offset
			setValue(m.value)
		end)
		return function()
			connection:Disconnect()
		end
	end, { target, frequency })

	return value
end
