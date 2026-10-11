-- TweenService:GetValue for every easing style and direction, sampled at
-- fixed points. Pins down the exact curve constants.
local TweenService = game:GetService("TweenService")
for _, style in Enum.EasingStyle:GetEnumItems() do
    for _, direction in Enum.EasingDirection:GetEnumItems() do
        local samples = {}
        for i = 0, 10 do
            local v = TweenService:GetValue(i / 10, style, direction)
            samples[#samples + 1] = string.format("%.4f", v)
        end
        log(style.Name, direction.Name, table.concat(samples, " "))
    end
end
