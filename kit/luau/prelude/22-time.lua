-- The page API, part 4: replies put together, and the time that wakes a page.
--
-- A page has no clock of its own. The host gives it the time as it draws it (`render`'s `now`)
-- and when it wakes it: a reply's `wake = seconds` asks the host to wake the page that many
-- seconds from now (0: at the next frame), and once the time has come the page's `handle` is told
-- { type = "wake", now = } and answers as it answers any event. Of the times a page asks the
-- earliest stands (what one part of it asks is not put off by another); a wake that came is gone,
-- and a page that wants another asks again. The host wakes a page at most once a frame.
--
-- The kit's timers run on it, named as the program runner's:
--
--   local saving = pencere.time.after(0.5, function() pencere.call("save", text) end)
--   local clock = pencere.time.every(1, function() return true end)   -- a frame each second
--   saving:cancel()             -- whether it was still waiting
--   clock:active()              -- whether it is
--   pencere.time.now()          -- the time the host last gave the page
--   pencere.time.wall()         -- the time of day: Unix seconds, and the local offset
--
-- `after(seconds, fn)` runs `fn` once, `seconds` (0 or more) from now; `every(seconds, fn)` every
-- `seconds` (more than 0) from now on, and once, not once for each, for runs it fell behind on. A
-- timer runs while the page is woken: the timers that are due, in the order of their times (and
-- of their making), before the page's own `handle` hears of the wake, which it does only where it
-- asked a `wake` itself. What `fn` returns is a reply, put together with what the others and the
-- page return (`pencere.merge`), and what is put together is held to what one reply may hand the
-- editors: the decorations past that wait for the next reply. A timer set while the page is woken
-- counts from the time of the wake. One set anywhere else (an event, a message, `render`,
-- `settle`, the script's start) counts from the next wake, which the kit asks for at once (for one
-- set in `settle`, which has no reply, the kit has the page rendered again): the host's next
-- frame, at most a frame late and never early, for the time the host last gave may be that of a
-- frame long gone. The kit asks the host for the wakes its timers want with the page's replies and
-- frames; a page that sets none and asks none is asked nothing new, and its `handle` is never told
-- `wake`.
--
-- `pencere.time.now()` is the time the host last gave the page, `render`'s `now` or a wake's:
-- seconds on the host's clock, 0 before it gave any. It does not go on between them.

local time = {}
pencere.time = time

-- The latest time the host gave, and the time of the wake being answered (nil while none is).
local known = 0
local waking = nil
-- The timers that wait, as a set. A timer set where the time was not known has no `due`, only
-- its `delay`, until the next wake places it.
local timers = {}
local made = 0
-- What the page asked of its own wakes: the time it asked for, placed, and the fewest seconds it
-- asked where the time was not known (placed at the next wake).
local own_due, own_delay = nil, nil
-- Something was set where the time was not known (a timer, or the page's own wake) since the kit
-- last asked for its wakes (`hooks.asked`).
local owed = false

-- What one reply may hand the page's editors, as the host holds it (the kit's SPEC, Section 4, the
-- protocol's floors): decorations, and bytes of text (what loads, changes and commands put in, and
-- the names of the layers decorated).
local REPLY_DECORATIONS = 20000
local REPLY_TEXT = 1024 * 1024
-- The decorations of the editors held back from a reply they would have taken past that, for the
-- next: { [editor] = { ask, ... } } in order, nil for none.
local held = nil

-- --- Replies put together ------------------------------------------------------------------------

-- An ask of an editor, or a list of them, as a list (nil for what is neither).
local function asks_of(value)
	if type(value) ~= "table" then
		return nil
	end
	if value[1] == nil and next(value) ~= nil then
		return { value }
	end
	return value
end

-- Two replies (nil, true, false or a table) as one: everything both ask, the first's asks of an
-- editor before the second's; a frame where either wants one, the page closed where either asks;
-- where both say the one thing (the keyboard, the pasteboard, a menu, a link, the pointer's shape)
-- the second's; and the earlier of two wakes. Neither is changed.
local function merge_two(first, second)
	if first == nil then
		return second
	elseif second == nil then
		return first
	end
	local first_table, second_table = type(first) == "table", type(second) == "table"
	if not first_table and not second_table then
		return first == true or second == true
	end
	local out = first_table and table.clone(first) or { redraw = first == true }
	local other = second_table and second or { redraw = second == true }
	for key, value in next, other do
		local mine = out[key]
		if mine == nil then
			out[key] = value
		elseif key == "redraw" or key == "close" then
			out[key] = (mine and true or false) or (value and true or false)
		elseif key == "wake" then
			-- What is not a number is kept, for the reply to be refused for it.
			if type(mine) == "number" and type(value) == "number" then
				out.wake = math.min(mine, value)
			elseif type(mine) == "number" then
				out.wake = value
			end
		elseif key == "editor" and type(mine) == "table" and type(value) == "table" then
			local editors = table.clone(mine)
			for name, asks in next, value do
				local before, after = asks_of(editors[name]), asks_of(asks)
				if before == nil or after == nil then
					editors[name] = asks
				else
					local all = table.clone(before)
					table.move(after, 1, #after, #all + 1, all)
					editors[name] = all
				end
			end
			out.editor = editors
		elseif key == "fill" and type(mine) == "table" and type(value) == "table" then
			local fill = table.clone(mine)
			for name, text in next, value do
				fill[name] = text
			end
			out.fill = fill
		else
			out[key] = value
		end
	end
	return out
end

-- Any number of replies as one, put together in order as two are (`merge_two`); nil where every one
-- is. None is changed. What a page's parts answer is put together with it: the timers' and the
-- page's own, and a part's of the page that answers for itself (a module of the library, such as a
-- syntax's `syntax:reply`).
local function merge(...)
	local out = nil
	for index = 1, select("#", ...) do
		out = merge_two(out, (select(index, ...)))
	end
	return out
end
pencere.merge = merge

-- Whether an ask of an editor is a decorate, as the interpreter reads it (a load, a change, a
-- selection or a reveal first).
local function is_decorate(ask)
	return type(ask) == "table"
		and ask.decorate ~= nil
		and ask.load == nil
		and ask.change == nil
		and ask.select == nil
		and ask.reveal == nil
end

local function bytes(value)
	return type(value) == "string" and #value or 0
end

-- The text an ask hands an editor, bytes, as the host counts it: a load's, the inserts of a
-- change and its tag, the words of the commands that carry some (insert, find, replace,
-- replace_all), the name of a layer decorated.
local function text_of(ask)
	if type(ask) ~= "table" then
		return 0
	elseif ask.load ~= nil then
		return bytes(ask.load)
	elseif ask.change ~= nil then
		local sum = bytes(ask.tag)
		if type(ask.change) == "table" then
			for _, change in ipairs(ask.change) do
				sum += type(change) == "table" and bytes(change.insert) or 0
			end
		end
		return sum
	elseif ask.select ~= nil or ask.reveal ~= nil then
		return 0
	elseif ask.decorate ~= nil then
		return bytes(ask.decorate)
	elseif ask.command == "find" then
		return bytes(ask.pattern)
	elseif ask.command == "insert" or ask.command == "replace" or ask.command == "replace_all" then
		return bytes(ask.text)
	end
	return 0
end

-- `reply` held to what one reply may hand the editors: the decorations that would take it past
-- that wait for the next reply, and with each of them every decoration after it of the same
-- editor, so that what a layer is given last is still what it has. Only decorations wait: they are
-- made against a version and carried over what is done since, so they mean a frame later what
-- they meant; a load, a change, a selection or a command goes when the page asks it. What would not
-- fit in a reply by itself goes as it is, for the host to refuse: the page's mistake, not the kit's.
local function fit(reply)
	if type(reply) ~= "table" or type(reply.editor) ~= "table" then
		return reply
	end
	local editors = reply.editor
	-- What goes whatever else does: everything but the decorations.
	local text, decorations = 0, 0
	for _, asks in next, editors do
		for _, ask in ipairs(asks_of(asks) or {}) do
			if not is_decorate(ask) then
				text += text_of(ask)
			end
		end
	end
	local out = nil
	for name, asks in next, editors do
		local list = asks_of(asks) or {}
		local kept, waiting = nil, nil
		for k, ask in ipairs(list) do
			local wait = false
			if is_decorate(ask) then
				local count = type(ask.decorations) == "table" and #ask.decorations or 0
				local words = bytes(ask.decorate)
				local fits = count <= REPLY_DECORATIONS and words <= REPLY_TEXT
				if
					fits
					and (waiting ~= nil or decorations + count > REPLY_DECORATIONS or text + words > REPLY_TEXT)
				then
					wait = true
				else
					decorations += count
					text += words
				end
			end
			if wait then
				if waiting == nil then
					waiting = {}
					kept = table.move(list, 1, k - 1, 1, {})
				end
				waiting[#waiting + 1] = ask
			elseif kept ~= nil then
				kept[#kept + 1] = ask
			end
		end
		if waiting ~= nil then
			held = held or {}
			local before = held[name]
			if before == nil then
				held[name] = waiting
			else
				table.move(waiting, 1, #waiting, #before + 1, before)
			end
			out = out or table.clone(editors)
			-- An editor given nothing is not named: `{}` would be an ask, and not one.
			out[name] = #kept > 0 and kept or nil
		end
	end
	if out == nil then
		return reply
	end
	local fitted = table.clone(reply)
	fitted.editor = next(out) ~= nil and out or nil
	return fitted
end

-- --- Timers --------------------------------------------------------------------------------------

local Timer = {}
Timer.__index = Timer

local function add(seconds, every, fn, name)
	if type(seconds) ~= "number" or not (seconds >= 0 and seconds < math.huge) then
		error("pencere.time." .. name .. " takes a number of seconds, 0 or more, then a function", 3)
	end
	if every and seconds == 0 then
		error("pencere.time.every: the time between two runs is more than 0 seconds", 3)
	end
	if type(fn) ~= "function" then
		error("pencere.time." .. name .. " takes a number of seconds, then the function to run", 3)
	end
	made += 1
	if waking == nil then
		owed = true
	end
	local timer = setmetatable({
		due = waking and waking + seconds or nil,
		delay = seconds,
		every = every and seconds or nil,
		fn = fn,
		order = made,
		waiting = true,
	}, Timer)
	timers[timer] = true
	return timer
end

-- Runs `fn` once, `seconds` (0 or more) from now. The timer.
function time.after(seconds, fn)
	return add(seconds, false, fn, "after")
end

-- Runs `fn` every `seconds` (more than 0), the first time `seconds` from now. The timer.
function time.every(seconds, fn)
	return add(seconds, true, fn, "every")
end

-- The time the host last gave the page, seconds on its clock (0 before it gave any).
function time.now()
	return known
end

-- The time of day now, as the host's system says it: seconds of Unix time (since 1970-01-01
-- 00:00:00 UTC), and how far the person's local time is ahead of UTC, seconds. Read as it is
-- asked, it goes on within a call, unlike `time.now()`; it is no clock to time a page's work by
-- (the person may set the system's clock), but what "4 minutes ago" and "14:32" are told from.
function time.wall()
	return pencere.host.wall_clock()
end

-- Stops the timer: whether it was still waiting (an `after` that ran is not).
function Timer:cancel()
	local was = self.waiting
	self.waiting = false
	timers[self] = nil
	return was
end

-- Whether the timer is still waiting to run.
function Timer:active()
	return self.waiting
end

-- --- What the application's calls do with it (`40-page.lua`) -------------------------------------

local hooks = {}
pencere._time = hooks

-- The host gave the time `now` (a frame).
function hooks.given(now)
	if type(now) == "number" and now > known then
		known = now
	end
end

-- A reply the page's own code returned, the wake it asks taken out of it and kept: the page is
-- woken (its `handle` told) at the earliest of the times it asked.
function hooks.taken(reply)
	if type(reply) ~= "table" or reply.wake == nil then
		return reply
	end
	local seconds = reply.wake
	if type(seconds) ~= "number" or not (seconds >= 0 and seconds < math.huge) then
		error("wake is a number of seconds, 0 or more", 0)
	end
	if waking ~= nil then
		local due = waking + seconds
		if own_due == nil or due < own_due then
			own_due = due
		end
	elseif own_delay == nil or seconds < own_delay then
		own_delay = seconds
		owed = true
	end
	local out = table.clone(reply)
	out.wake = nil
	return out
end

-- In how many seconds the page is to be woken, for what the kit has learnt since it last asked:
-- at once where a timer (or the page) asked where the time was not known, to place it, or where
-- decorations wait for a reply (`fit`); in a wake, the earliest time waiting. Outside a wake what
-- was placed was asked when it was, and asking it again from a time that may be old would only ask
-- it later. Nil for nothing.
function hooks.asked()
	owed = false
	local unplaced = own_delay ~= nil or held ~= nil
	local earliest = own_due
	for timer in next, timers do
		local due = timer.due
		if due == nil then
			unplaced = true
		elseif earliest == nil or due < earliest then
			earliest = due
		end
	end
	if unplaced then
		return 0
	elseif waking == nil or earliest == nil then
		return nil
	end
	return math.max(0, earliest - waking)
end

-- Whether a timer was set (or a wake asked) where the time was not known since the kit last asked
-- for its wakes, and the kit is to ask for one: what a call that has no reply (`settle`) says by
-- asking for the page again, whose frame asks.
function hooks.owed()
	return owed and hooks.asked() ~= nil
end

-- The page's reply as it goes to the host: the decorations that waited for it first, held, with
-- the page's, to what a reply may hand the editors (`fit`), and with the wake the kit asks for. The
-- end of a call of the application's.
function hooks.finish(reply)
	if held ~= nil then
		local waiting = held
		held = nil
		reply = merge({ editor = waiting }, reply)
	end
	reply = fit(reply)
	local seconds = hooks.asked()
	waking = nil
	if seconds == nil then
		return reply
	end
	return merge(reply, { wake = seconds })
end

-- The page is woken at `now`: what was set where the time was not known is placed from it, the
-- timers that are due run in the order of their times, then the page's `handle` is told where it
-- asked a wake itself. What they return, put together. A timer set by these counts from `now`
-- and runs at a later wake, not in this one.
function hooks.woken(now, handle)
	hooks.given(now)
	waking = now
	for timer in next, timers do
		if timer.due == nil then
			timer.due = now + timer.delay
		end
	end
	if own_delay ~= nil then
		local due = now + own_delay
		if own_due == nil or due < own_due then
			own_due = due
		end
		own_delay = nil
	end
	local due = {}
	for timer in next, timers do
		if timer.due <= now then
			due[#due + 1] = timer
		end
	end
	table.sort(due, function(a, b)
		if a.due ~= b.due then
			return a.due < b.due
		end
		return a.order < b.order
	end)
	-- The page's own wake is gone once it comes, before the page is told, so that what it asks
	-- then stands.
	local own = own_due ~= nil and own_due <= now
	if own then
		own_due = nil
	end
	local reply = nil
	for _, timer in ipairs(due) do
		-- One that ran before it may have stopped it.
		if timer.waiting then
			if timer.every then
				local next_due = timer.due + timer.every
				if next_due <= now then
					next_due = now + timer.every
				end
				timer.due = next_due
			else
				timer.waiting = false
				timers[timer] = nil
			end
			reply = merge(reply, hooks.taken(timer.fn()))
		end
	end
	if own and handle then
		reply = merge(reply, hooks.taken(handle({ type = "wake", now = now })))
	end
	return reply
end
