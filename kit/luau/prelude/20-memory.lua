-- The page API, part 3: what a page remembers between frames.
--
-- A page is built afresh for every frame, so what the pointer is over, where the keyboard is and
-- how far a highlight has faded are not in the elements: they are kept by name (the element's id)
-- in these, and the page reads them when it builds. How far a box is scrolled is kept by the host
-- (`pencere.layout.scroll_offset`).

local layout = pencere.layout
local f32 = pencere.f32

-- --- Highlights that come and go ---------------------------------------------------------------

-- The cubic Bézier curve of a style sheet's `cubic-bezier(x1, y1, x2, y2)` at `x`.
local function bezier(x1, y1, x2, y2, x)
	local function curve(a, b, s)
		local u = 1 - s
		return 3 * u * u * s * a + 3 * u * s * s * b + s * s * s
	end
	local low, high = 0, 1
	for _ = 1, 24 do
		local middle = (low + high) / 2
		if curve(x1, x2, middle) < x then
			low = middle
		else
			high = middle
		end
	end
	return curve(y1, y2, (low + high) / 2)
end

-- How far a move is at `t` of its time (0 to 1), along `ease`: "linear", "out" (fast at first,
-- settling: what most changes are), "in-out" or "spring" (quick, long in coming to rest). The
-- same curves the host moves an element's `motion` along.
function pencere.ease(t, ease)
	t = math.clamp(t, 0, 1)
	if ease == "linear" then
		return t
	elseif ease == "out" or ease == nil then
		return bezier(0.2, 0, 0, 1, t)
	elseif ease == "in-out" then
		return bezier(0.4, 0, 0.2, 1, t)
	elseif ease == "spring" then
		if t >= 1 then
			return 1
		end
		return 1 - (1 + 10 * t) * math.exp(-10 * t)
	end
	error('an ease is "out", "linear", "in-out" or "spring", not ' .. tostring(ease), 2)
end

-- A highlight takes this long to come to full strength or to go, seconds. The levels are single
-- precision, as the engine's own are: a highlight that comes up and goes in equal steps comes
-- to exactly 0 and is forgotten, where doubles would leave a level of 1e-17 that is drawn.
local FADE_SECONDS = f32(0.12)
-- The longest step one frame may take: a frame after a long sleep moves the highlights a little,
-- not to their end, so the change is seen.
local STEP_MAX = 0.05

local Fades = {}
Fades.__index = Fades
pencere.Fades = Fades
Fades.SECONDS = FADE_SECONDS

function Fades.new()
	return setmetatable({ levels = {}, last = nil }, Fades)
end

-- Sets where `id`'s level goes (0 to 1), over `seconds` from one end to the other (a highlight's
-- 0.12 when nil). A level not known yet starts at 0 and comes up.
function Fades:to(id, target, seconds)
	target = math.clamp(target, 0, 1)
	seconds = seconds and f32(math.max(seconds, 0.001)) or FADE_SECONDS
	local fade = self.levels[id]
	if fade then
		fade.target, fade.seconds = target, seconds
	elseif target > 0 then
		self.levels[id] = { level = 0, target = target, seconds = seconds }
	end
end

-- Sets where `id`'s level goes and puts it there at once.
function Fades:snap(id, level)
	level = math.clamp(level, 0, 1)
	if level > 0 then
		self.levels[id] = { level = level, target = level, seconds = FADE_SECONDS }
	else
		self.levels[id] = nil
	end
end

-- `id`'s level now, 0 for one that is not known; along the curve `ease` where one is given
-- (`pencere.ease`): "out" comes up fast and settles.
function Fades:level(id, ease)
	local fade = self.levels[id]
	local level = fade and fade.level or 0
	if ease then
		return pencere.ease(level, ease)
	end
	return level
end

-- Whether anything is on the way to its target.
function Fades:moving()
	for _, fade in pairs(self.levels) do
		if fade.level ~= fade.target then
			return true
		end
	end
	return false
end

-- Takes the step to `now` (seconds on the application's clock). Reduce Motion puts every level
-- at its target. True while anything is still moving afterwards: the page asks for another frame.
function Fades:advance(now, reduce_motion)
	local levels = self.levels
	if not self:moving() then
		self.last = nil
		for id, fade in pairs(levels) do
			if not (fade.level > 0) then
				levels[id] = nil
			end
		end
		return false
	end
	local dt = 0
	if self.last then
		dt = math.clamp(now - self.last, 0, STEP_MAX)
	end
	for _, fade in pairs(levels) do
		local step = reduce_motion and 1 or f32(f32(dt) / (fade.seconds or FADE_SECONDS))
		if fade.level < fade.target then
			fade.level = math.min(f32(fade.level + step), fade.target)
		elseif fade.level > fade.target then
			fade.level = math.max(f32(fade.level - step), fade.target)
		end
	end
	local moving = self:moving()
	-- A level that came to rest at 0 is not kept.
	for id, fade in pairs(levels) do
		if not (fade.level > 0 or fade.target > 0) then
			levels[id] = nil
		end
	end
	self.last = moving and now or nil
	if reduce_motion then
		return false
	end
	return moving
end

-- --- A long list's arithmetic ------------------------------------------------------------------

-- Where each row of a list stands, which rows a window of it shows and how far to scroll to
-- see one. Rows are counted from 1. A page that shows a long list builds only the rows in view
-- and keeps the rest of the list's length with a spacer above and one below; which rows to build
-- as the list scrolls, and when to build them again, is `pencere.Built`'s.
local Rows = {}
Rows.__index = Rows
pencere.Rows = Rows

-- A list whose rows are `heights` high (a list of numbers).
function Rows.new(heights)
	local tops = { 0 }
	local sum = 0
	for index, height in ipairs(heights) do
		sum += math.max(height, 0)
		tops[index + 1] = sum
	end
	return Rows.from_tops(tops)
end

-- A list whose rows start where `tops` says: `tops[i]` is where row i starts, from 0, and the last
-- is where the last row ends. The table is the list's, not a copy: a page that keeps the sums of a
-- list that grows gives them as they are, and nothing is summed again.
function Rows.from_tops(tops)
	return setmetatable({ tops = tops, count = math.max(#tops - 1, 0), lead = 0, trail = 0 }, Rows)
end

-- A list of `count` rows each `height` high: as long as any list, as small as none.
function Rows.even(count, height)
	return setmetatable({ step = math.max(height, 0), count = math.max(count, 0), lead = 0, trail = 0 }, Rows)
end

-- The same list with `lead` points of room before the first row and `trail` after the last.
function Rows:padded(lead, trail)
	self.lead, self.trail = lead, trail
	return self
end

function Rows:len()
	return self.count
end

function Rows:is_empty()
	return self.count == 0
end

-- The length of the whole list, with the room around the rows.
function Rows:total()
	return self:top(self.count + 1) + self.trail
end

-- Where row `index` starts (the list's length for a row past the end).
function Rows:top(index)
	index = math.min(index, self.count + 1)
	if self.step then
		return self.lead + (index - 1) * self.step
	end
	return self.lead + (self.tops[index] or 0)
end

function Rows:height(index)
	return self:top(index + 1) - self:top(index)
end

-- How many of the rows' tops (the end of the last one among them) are before `y`, or not past it
-- where `inclusive`.
local function count_before(rows, y, inclusive)
	local low, high = 1, rows.count + 2
	while low < high do
		local middle = (low + high) // 2
		local top = rows:top(middle)
		if top < y or (inclusive and top == y) then
			low = middle + 1
		else
			high = middle
		end
	end
	return low - 1
end

-- The row at `y` from the list's start (the last row for a point past the end).
function Rows:at(y)
	if self:is_empty() then
		return nil
	end
	local after = count_before(self, y, true)
	return math.min(math.max(after, 1), self:len())
end

-- The rows that touch the window `offset .. offset + view`, `overscan` points more at each end:
-- the first and the last (the last is before the first when there are none).
function Rows:window(offset, view, overscan)
	if self:is_empty() or view <= 0 then
		return 1, 0
	end
	local from = math.max(offset - overscan, 0)
	local to = offset + view + overscan
	if from >= self:total() then
		return self:len() + 1, self:len()
	end
	local first = self:at(from)
	local last = math.min(count_before(self, to, false), self:len())
	return first, math.max(last, first - 1)
end

-- The offset that shows all of row `index` in a window of `view` points that is at `offset`
-- now: `offset` itself when the row is in view, the nearest edge otherwise. The first row is
-- shown from the start of the list and the last to its end, room around them and all.
function Rows:reveal(index, offset, view)
	local top = index == 1 and 0 or self:top(index)
	local bottom = index >= self:len() and self:total() or self:top(index + 1)
	if top < offset then
		return top
	elseif bottom > offset + view then
		return math.min(math.max(bottom - view, 0), top)
	end
	return offset
end

-- How much of a list is built beyond what is in view, in views: more the way it goes than behind
-- it, for the host scrolls what is built at once and the page builds what comes into view only a
-- frame or two later. How near the end of what is built, the way the list goes, what is in view
-- comes before the page is built again, in views: as much as comes into view while the page
-- builds, at the most a person scrolls.
local AHEAD, BEHIND, NEAR = 2, 0.5, 1

-- Which rows of a long list a page built, and which way the list goes: kept between frames (one
-- for each list), so that the page builds two views of rows ahead of what is in view and half a
-- view behind it, and is built again only when what is in view comes within a view of the end of
-- what was built, the way the list goes: not at every turn of the wheel, and never short of what
-- scrolling brings into view while it keeps up. `built:span` is the rows to build; `built:stale`,
-- in a `scrolled` event, whether the list's new place means building again. A page that builds its
-- list with `:list` has all of this done for it.
local Built = {}
Built.__index = Built
pencere.Built = Built

function Built.new()
	-- `first`, `last` and `count`: the rows built last, and how many the list had then. `up`
	-- and `seen`: whether the list last went up, and where it was then: the way is changed only
	-- by a move, so that the build a move asked for, at the same place, finds the way it went.
	return setmetatable({ first = nil, last = nil, count = nil, up = false, seen = 0 }, Built)
end

local function went(built, offset)
	if offset ~= built.seen then
		built.up = offset < built.seen
		built.seen = offset
	end
end

-- Whether, with the list `rows` at `offset` in a window of `view` points, the page is to be built
-- again: what is in view is past what was built, or near its end the way the list goes, or the
-- list is not as long as it was.
function Built:stale(rows, offset, view)
	went(self, offset)
	if self.count == nil or self.count ~= rows:len() then
		return true
	end
	local top, bottom = rows:top(self.first), rows:top(self.last + 1)
	local more_above, more_below = self.first > 1, self.last < rows:len()
	if (more_above and offset < top) or (more_below and offset + view > bottom) then
		return true
	end
	local near = view * NEAR
	if self.up then
		return more_above and offset - near < top
	end
	return more_below and offset + view + near > bottom
end

-- The rows to build (the first and the last) with the list `rows` at `offset` in a window of
-- `view` points: those built last while they still hold what is in view with room, else a new
-- span around it, longer the way the list goes (and all of it the other way at an end of the list).
function Built:span(rows, offset, view)
	if not self:stale(rows, offset, view) then
		return self.first, self.last
	end
	-- Before anything is built there is no way the list goes yet: as much is built both ways.
	local before, after = AHEAD, AHEAD
	if self.count ~= nil then
		before, after = self.up and AHEAD or BEHIND, self.up and BEHIND or AHEAD
	end
	-- What would be built past an end of the list is built at the other instead: a list at its end
	-- can only go back.
	local start, finish = rows:top(1), rows:total()
	local from, to = offset - view * before, offset + view * (1 + after)
	if from < start then
		to += start - from
		from = start
	end
	if to > finish then
		from = math.max(from - (to - finish), start)
		to = finish
	end
	local first, last = rows:window(from, to - from, 0)
	self.first, self.last, self.count = first, last, rows:len()
	return first, last
end

-- --- A long list in a box the host scrolls -----------------------------------------------------

-- What is kept of each long list a page builds with `:list`, by the box's name: its `Built`, the
-- rows as they stood (`rows`), the window it was built for (`view`), the rows made (`first`,
-- `last`), its box, and whether this render built it (`seen`). A list the page stops building is
-- forgotten.
local lists = {}
-- How tall the page is: the window of a list that has not been laid out yet.
local page_h = 0

local function list_spacer(height)
	return pencere.div():h(math.max(height, 0)):no_shrink()
end

-- Fills the box with a long list: `rows` (a `pencere.Rows`) is where its rows stand, and `make(i)`
-- makes row i. Only the rows near what is in view are made, more of them the way the list goes
-- (`pencere.Built`), and the rest of the list's length is a spacer above them and one below. The box
-- has a name (`:id`, before `:list`), scrolls (`:scroll()`), is a column, and has no padding above
-- and no gap between its children: room above the rows is `rows:padded(lead, trail)`.
--
-- The host scrolls the rows made at once; the page is drawn again only when what is in view comes
-- near the end of what was made (a `scrolled` event asks for the frame whatever `handle` answers),
-- and when the box was laid out taller than the rows made can fill.
--
--   pencere.div():id("log"):grow(1):min_h(0):scroll()
--       :list(pencere.Rows.even(#lines, 18), function(i) return line(lines[i]) end)
function pencere.Element:list(rows, make)
	if getmetatable(rows) ~= Rows then
		error("list takes the rows of the list (a pencere.Rows), then a function that makes row i", 2)
	end
	if type(make) ~= "function" then
		error("list takes the rows of the list, then a function that makes row i, not a " .. type(make), 2)
	end
	local id = self.meta.id
	if id == nil then
		error("a long list is a box with a name: give it one (:id) before :list", 2)
	end
	local list = lists[id]
	if list == nil then
		list = { built = Built.new() }
		lists[id] = list
	end
	local extent = layout.scroll_extent(id)
	local view = extent and extent.view or page_h
	local first, last = list.built:span(rows, (layout.scroll_offset(id)), view)
	list.rows, list.view, list.first, list.last, list.box, list.seen = rows, view, first, last, self, true
	self:child(list_spacer(rows:top(first)))
	for index = first, last do
		self:child(make(index))
	end
	self:child(list_spacer(rows:total() - rows:top(last + 1)))
	return self
end

-- The first and the last row the list named `id` made in the last render (the last is before the
-- first when it made none), and how tall its window is (as it was laid out, or the page before it
-- was): what a page asks its program for a window at a time, and how far a page of rows goes. Nil
-- for a box that is not a long list.
function pencere.list_window(id)
	local list = lists[id]
	if list == nil then
		return nil
	end
	return list.first, list.last, list.view
end

-- What the page's part of the prelude asks of the long lists (`40-page.lua`).
pencere._lists = {}

-- A render of a page `height` tall begins.
function pencere._lists.begin(height)
	page_h = height
	for _, list in pairs(lists) do
		list.seen = false
	end
end

-- The render made its tree: a list it did not build is forgotten, and a box that cannot hold a
-- long list is said to be one.
function pencere._lists.finish()
	for id, list in pairs(lists) do
		if not list.seen then
			lists[id] = nil
		else
			local style = list.box.style
			local name = tostring(id)
			if style.overflow ~= "scroll" then
				error("the long list " .. name .. " is in a box that does not scroll: :scroll() it", 0)
			end
			if style.display == "grid" or style.direction == "row" then
				error("the long list " .. name .. " is in a box that is not a column: its rows stand one under another", 0)
			end
			if style.padding and (style.padding[1] or 0) ~= 0 then
				error(
					"the long list " .. name .. " has padding above its rows: the room above them is rows:padded(lead, trail)",
					0
				)
			end
			if (style.gap_y or 0) ~= 0 then
				error("the long list " .. name .. " has a gap between its rows: a row's height is all of its room", 0)
			end
		end
	end
end

-- The host scrolled `boxes` ({ { id =, x =, y = }, ... }): whether a long list among them is to be
-- built again at its new place.
function pencere._lists.scrolled(boxes)
	local again = false
	for _, box in ipairs(boxes) do
		local list = lists[box.id]
		if list then
			local extent = layout.scroll_extent(box.id)
			if extent then
				list.view = extent.view
			end
			again = list.built:stale(list.rows, box.y, list.view) or again
		end
	end
	return again
end

-- The page was laid out: whether a long list's window is now one the rows made do not fill.
function pencere._lists.settle()
	local again = false
	for id, list in pairs(lists) do
		local extent = layout.scroll_extent(id)
		if extent then
			list.view = extent.view
			again = list.built:stale(list.rows, (layout.scroll_offset(id)), extent.view) or again
		end
	end
	return again
end

-- --- What the pointer and the keyboard are on ----------------------------------------------------

-- How far the pointer moves with a button held on something that can be dragged before it is
-- dragged, points: less is a click with an unsteady hand.
pencere.DRAG_START = 4

-- The remembered state of a page: what the pointer is over and has pressed, what is being
-- dragged, where the keyboard is. (How far a box is scrolled is the host's: `pencere.layout`.)
-- Every method that takes the pointer answers whether anything it keeps *changed* (a pointer moving inside the row it was already over
-- changes nothing, and no frame is drawn for it). `laid` is `pencere.layout`.
local Ui = {}
Ui.__index = Ui
pencere.Ui = Ui

function Ui.new()
	return setmetatable({
		-- The pressable element the pointer is over, the one a button is held on, and the
		-- element the keyboard is on.
		hover = nil,
		pressed = nil,
		focus = nil,
		-- What a button went down on that can be dragged: { source =, from =, at =, target =,
		-- moving = }; and what was dragged and let go and is not yet taken.
		drag = nil,
		dropped = nil,
		-- The highlights coming and going.
		fades = Fades.new(),
	}, Ui)
end

-- The pointer is at `at` (nil: it left the page). Whether the element it is over changed, or —
-- while something is dragged — whether the drag moved.
function Ui:point(laid, at)
	local changed = false
	local drag = self.drag
	if drag and at then
		local dx, dy = at.x - drag.from.x, at.y - drag.from.y
		if not drag.moving and dx * dx + dy * dy >= pencere.DRAG_START * pencere.DRAG_START then
			drag.moving = true
			changed = true
		end
		if drag.moving then
			local target = laid.drop_target(at)
			if target == drag.source then
				target = nil
			end
			changed = changed or drag.at.x ~= at.x or drag.at.y ~= at.y or drag.target ~= target
			drag.at = { x = at.x, y = at.y }
			drag.target = target
		end
	end
	local over = nil
	if at then
		over = laid.hit(at)
	end
	changed = changed or self.hover ~= over
	self.hover = over
	return changed
end

-- A button went down at `at`: the element under it is the pressed one, and the keyboard moves
-- to the nearest element around it that takes it. The element pressed.
function Ui:press(laid, at)
	local hit = laid.hit(at)
	self.pressed = hit
	self.dropped = nil
	local source = laid.drag_source(at)
	if source ~= nil then
		self.drag = {
			source = source,
			from = { x = at.x, y = at.y },
			at = { x = at.x, y = at.y },
			moving = false,
		}
	else
		self.drag = nil
	end
	local takes = {}
	for _, id in ipairs(laid.focus_order()) do
		takes[id] = true
	end
	for _, id in ipairs(laid.ids_at(at)) do
		if takes[id] then
			self.focus = id
			break
		end
	end
	return hit
end

-- The button came up at `at`. A click is a release over the element the press was on, when it was
-- not dragged: that element. What was dragged is put down (`take_drop`).
function Ui:release(laid, at)
	local drag = self.drag
	self.drag = nil
	if drag and drag.moving then
		self.pressed = nil
		local target = laid.drop_target(at)
		if target == drag.source then
			target = nil
		end
		self.dropped = { source = drag.source, target = target, at = { x = at.x, y = at.y } }
		return nil
	end
	local pressed = self.pressed
	self.pressed = nil
	if pressed ~= nil and laid.hit(at) == pressed then
		return pressed
	end
	return nil
end

-- What is being dragged, once the pointer has moved far enough for it to be a drag: { source =,
-- at = { x =, y = }, target = } — the page draws it where the pointer is and marks the target.
function Ui:dragging()
	local drag = self.drag
	if drag and drag.moving then
		return drag
	end
	return nil
end

-- What was dragged and let go since the last ask: { source =, target =, at = }. Asked once, it is
-- no longer kept.
function Ui:take_drop()
	local dropped = self.dropped
	self.dropped = nil
	return dropped
end

-- Gives up the drag under way, if there is one (the escape key). Whether there was one.
function Ui:cancel_drag()
	if self:dragging() then
		self.drag = nil
		self.pressed = nil
		return true
	end
	return false
end

-- Puts the keyboard on `id`. Whether it moved.
function Ui:focus_on(id)
	local changed = self.focus ~= id
	self.focus = id
	return changed
end

-- Moves the keyboard to the next element that takes it (the previous one when `back`), around
-- from the last to the first. The element the keyboard is on now.
function Ui:focus_step(laid, back)
	local order = laid.focus_order()
	local count = #order
	if count == 0 then
		self.focus = nil
		return nil
	end
	local at = nil
	for index, id in ipairs(order) do
		if id == self.focus then
			at = index
			break
		end
	end
	local nextAt
	if at == nil then
		nextAt = back and count or 1
	elseif back then
		nextAt = (at - 2) % count + 1
	else
		nextAt = at % count + 1
	end
	self.focus = order[nextAt]
	return self.focus
end

-- Where a key goes: the element the keyboard is on, then the named elements around it, the
-- innermost first.
function Ui:key_path(laid)
	if self.focus == nil then
		return {}
	end
	return laid.path_of(self.focus)
end
