local std = ...
local ds = std.datastore
local sort = std.table.sort
return function(self, ascending, pageSize, minValue, maxValue)
    local entries = {}
    for key, value in ds.data(self) do
        if type(value) == "number"
            and (minValue == nil or value >= minValue)
            and (maxValue == nil or value <= maxValue) then
            entries[#entries + 1] = { key = key, value = value }
        end
    end
    sort(entries, function(a, b)
        if ascending then return a.value < b.value end
        return a.value > b.value
    end)
    local size = pageSize or 50
    local pages = { IsFinished = false }
    local start = 1
    local function page()
        local out = {}
        for i = start, math.min(start + size - 1, #entries) do
            out[#out + 1] = entries[i]
        end
        return out
    end
    pages.IsFinished = #entries < start + size
    function pages:GetCurrentPage()
        return page()
    end
    function pages:AdvanceToNextPageAsync()
        start += size
        pages.IsFinished = #entries < start + size
    end
    return pages
end
