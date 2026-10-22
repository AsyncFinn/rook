-- UIPageLayout's JumpTo, JumpToIndex, Next and Previous, and the tween
-- between pages (docs/behavior/gui/modern.md#uipagelayout). The layout
-- (rook-gui) shows the pages from the view this sets while it turns, and
-- from CurrentPage otherwise.
--
-- Measured in Studio 0.744: a jump sets CurrentPage at once; when
-- Animated, the view eases there over TweenTime by EasingStyle and
-- EasingDirection (Back Out by default overshoots by a tenth of a page),
-- then Stopped fires with the page; without Animated it moves at once and
-- Stopped doesn't fire. Next on the last page (or Previous on the first)
-- does nothing unless Circular, which goes round: the view ends on the
-- page when Animated, but otherwise stays past the end (one page before
-- the first, say), where the layout shows no page, until the next jump.
-- An index out of range does nothing.

local std = ...
local fire, set_internal, pages, turn_pages = std.fire, std.set_internal, std.pages, std.turn_pages
local ease, heartbeat = std.tween.ease, std.heartbeat
local setmetatable, math = std.setmetatable, std.math

-- layout -> { from, to, elapsed, conn }
local turns = setmetatable({}, { __mode = "k" })

local function indexOf(list, page)
	for i, p in list do
		if p == page then
			return i - 1
		end
	end
	return nil
end

-- Where the view is now, as a page index.
local function viewOf(layout, list)
	local t = turns[layout]
	if t then
		local raw = if layout.TweenTime > 0 then math.min(t.elapsed / layout.TweenTime, 1) else 1
		local eased = ease(layout.EasingStyle.Value, layout.EasingDirection.Value, raw)
		return t.from + (t.to - t.from) * eased
	end
	return indexOf(list, layout.CurrentPage) or 0
end

local function stopTurning(layout)
	local t = turns[layout]
	if t then
		t.conn:Disconnect()
		turns[layout] = nil
	end
end

-- Turn to page index `to` (unwrapped: past the ends when going round).
local function turnTo(layout, list, to)
	local count = #list
	local page = list[to % count + 1]
	local from = viewOf(layout, list)
	stopTurning(layout)
	set_internal(layout, "CurrentPage", page)
	if not layout.Animated then
		turn_pages(layout, if to == to % count then nil else to)
		return
	end
	local t = { from = from, to = to, elapsed = 0 }
	turns[layout] = t
	turn_pages(layout, from)
	t.conn = heartbeat:Connect(function(dt)
		t.elapsed += dt
		if t.elapsed < layout.TweenTime then
			turn_pages(layout, viewOf(layout, list))
			return
		end
		stopTurning(layout)
		turn_pages(layout, nil)
		fire(layout, "Stopped", page)
	end)
end

local pageLayout = {}

function pageLayout.jumpToIndex(layout, index)
	local list = pages(layout)
	if index < 0 or index >= #list then
		return
	end
	turnTo(layout, list, index)
end

function pageLayout.jumpTo(layout, page)
	local list = pages(layout)
	local index = indexOf(list, page)
	if index then
		turnTo(layout, list, index)
	end
end

-- Step `by` pages from the current one.
function pageLayout.step(layout, by)
	local list = pages(layout)
	local count = #list
	local index = indexOf(list, layout.CurrentPage)
	if count == 0 or not index then
		return
	end
	local to = index + by
	if to < 0 or to >= count then
		if not layout.Circular then
			return
		end
	end
	turnTo(layout, list, to)
end

return pageLayout
