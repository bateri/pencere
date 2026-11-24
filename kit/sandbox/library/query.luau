-- @pencere/query: what a page reads of its program, kept by what it asked.
--
-- A page asks its program with `pencere.call` and the answer comes later, out of order with the
-- others. What every page then writes by itself is the same ledger: whether the data is on its way,
-- ready or lost; the old data that stays while a new answer is awaited; the answer of a choice the
-- person has since left, which is let go; the one call that is out, which asks again when it is
-- over; the data that is asked again while it is on the screen; the data asked again when what it
-- was asked from changed. This module is that ledger, kept once. It is pure Luau on the page API
-- (`pencere.call`, `pencere.cancel`, `pencere.time`, `pencere.json`), a layer above the prelude.
--
--   local query = require("@pencere/query")
--   local q = query.new()
--   render = function()
--       local list = q:get("issues", { team = team }, { every = 5, stamp = at })
--       if list.status == "loading" then return pencere.text("Loading") end
--       ...list.data, list.fetching, list.error...
--   end,
--
-- `q:get(method, params, options)` is what a render asks. It returns what is known now and asks the
-- program where the data is missing or stale: `{ status =, data =, error =, fetching = }`, with
-- `status` "loading" (no data, and it is asked), "ready" (there is data: `data` may be nil, a JSON
-- null is a result) or "failed" (no data, and the last ask failed). A failed ask where there is data
-- leaves the status "ready" and says `error` (`{ code =, message = }`) beside the old `data`;
-- `fetching` is whether a call is out. A key (the method and its params) has one call out at most.
-- The module's own callbacks ask for a frame, so that what changes is seen at the next render.
--
-- What makes data stale: `q:invalidate(method, params)`, a `stamp` option that is not the one the
-- data was asked under, and `every` seconds (polling) for as long as the page goes on asking. A key
-- whose ask failed is not asked again by the render that shows it, only by one of these.
--
-- The rest of a context: `q:peek` (looks, never asks), `q:set` (what the program pushed),
-- `q:forget` (lets go of a key and its call). Data nobody asked for in `keep` seconds of renders is
-- let go. Where `params` is not said to `invalidate` and `forget`, every key of the method is meant.

local query = {}

local Query = {}
Query.__index = Query

-- How long data nobody asks for is kept, seconds (of the renders: `keep` in `query.new`).
local KEEP = 300
-- How deep two data are compared before they are taken for different.
local DEEP = 100

local NEW_OPTIONS = { keep = true }
local GET_OPTIONS = { every = true, stamp = true, slot = true, version = true }

-- --- What the caller gave -------------------------------------------------------------------------

-- Raises `message` at the line of the page that called the function that calls `fail` (`level` 3:
-- 1 is the function that calls `fail`, 2 its caller, 3 the caller of that). A mistake of the page is
-- the page's line, as `ui.new`'s is.
local function fail(level, message)
	error(message, level + 1)
end

local function check_options(options, allowed, name)
	if options == nil then
		return
	end
	if type(options) ~= "table" then
		fail(3, name .. " takes a table of options, not " .. type(options))
	end
	for option in options do
		if not allowed[option] then
			fail(3, name .. " has no option called " .. tostring(option))
		end
	end
end

local function check_self(self, name)
	if getmetatable(self) ~= Query then
		fail(3, "query:" .. name .. " is called on a context: q:" .. name .. "(...)")
	end
end

local function check_call(method, params, name)
	if type(method) ~= "string" then
		fail(3, "query:" .. name .. " takes the method to ask for, a string, not " .. type(method))
	end
	if params ~= nil and type(params) ~= "table" then
		fail(3, "query:" .. name .. " takes params that are nil or a table, not " .. type(params))
	end
end

-- The key of a method and its params, and the params as text. No params and an empty table are one
-- key (`encode` writes the members in order, so that the same params are always the same text).
-- Params that cannot be written are the page's mistake, said at the page's line (`level`, as `fail`
-- has it: where the function that calls `key_of` was called from).
local function key_of(method, params, level)
	local text = "{}"
	if params ~= nil then
		local ok, written = pcall(pencere.json.encode, params)
		if not ok then
			fail(level, (string.gsub(tostring(written), "^%S+:%d+: ", "")))
		end
		text = written
	end
	return method .. "\0" .. text, text
end

-- --- What is the same ----------------------------------------------------------------------------

-- Whether two data are alike all the way down. It stops at the first difference.
local function same(a, b, depth)
	if a == b then
		return true
	end
	if type(a) ~= "table" or type(b) ~= "table" or depth > DEEP then
		return false
	end
	-- `json.array` and `json.null` are metatables: an empty list is not an empty object.
	if getmetatable(a) ~= getmetatable(b) then
		return false
	end
	for member, value in a do
		if not same(value, b[member], depth + 1) then
			return false
		end
	end
	for member in b do
		if a[member] == nil then
			return false
		end
	end
	return true
end

-- --- An entry: what a context knows of one key ------------------------------------------------------

local function view(entry)
	local status = "loading"
	if entry.ready then
		status = "ready"
	elseif entry.call == nil and entry.error ~= nil then
		status = "failed"
	end
	return { status = status, data = entry.data, error = entry.error, fetching = entry.call ~= nil }
end

local function new_entry(key, method, params, text, now)
	-- The call is made from what the key says, not from the table the page may change later.
	local asked = nil
	if text ~= "{}" then
		asked = pencere.json.decode(text)
		if asked == nil then
			asked = params
		end
	elseif params ~= nil then
		asked = {}
	end
	return {
		key = key,
		method = method,
		params = asked,
		-- There is data (it may be nil).
		ready = false,
		data = nil,
		-- The version of `data`, where the page has a way to tell (`options.version`).
		version = nil,
		error = nil,
		-- The number of the call that is out.
		call = nil,
		-- The next `get` asks.
		stale = true,
		-- What the call that is out, or the last, was asked under, and what the page says now.
		stamp = nil,
		want_stamp = nil,
		-- A key that `set` made takes the stamp of the first `get` that follows: it was asked under
		-- none, and what is told is as new as the page says.
		adopt = false,
		-- When the last call was over, and whether a `get` came since: the polling goes on for
		-- data that is asked for.
		answered = nil,
		touched = false,
		every = nil,
		timer = nil,
		version_of = nil,
		-- When it was last asked by a `get` (what `keep` counts from), in which slot (a name, or nil),
		-- and which `get` of the context it was (they are counted).
		used = now,
		asked_at = nil,
		slot = nil,
		asked_tick = 0,
	}
end

-- --- The calls --------------------------------------------------------------------------------------

local answered, poll

local function stop_polling(entry)
	if entry.timer ~= nil then
		entry.timer:cancel()
		entry.timer = nil
	end
end

-- Lets go of an entry: its timer, its call (the program is told it is given up, and what it says of
-- it is not heard) and its place in a slot.
local function drop(self, entry)
	self.entries[entry.key] = nil
	stop_polling(entry)
	if entry.call ~= nil then
		pencere.cancel(entry.call)
		entry.call = nil
	end
	for name, held in self.slots do
		if held == entry then
			self.slots[name] = nil
		end
	end
end

local function ask(self, entry)
	stop_polling(entry)
	entry.stale = false
	entry.stamp = entry.want_stamp
	local id
	id = pencere.call(entry.method, entry.params, function(result, err)
		return answered(self, entry, id, result, err)
	end)
	entry.call = id
end

-- The next ask of a key that is asked for `every` seconds: one timer, from an answer.
local function arm(self, entry, delay)
	stop_polling(entry)
	if entry.every == nil then
		return
	end
	entry.timer = pencere.time.after(delay or entry.every, function()
		return poll(self, entry)
	end)
end

-- The timer of a key that is asked for `every` seconds: asks again if a `get` came since the last
-- answer; if none did, the data is not on the screen and the polling stops, until a `get` sets it
-- going again.
function poll(self, entry)
	entry.timer = nil
	if self.entries[entry.key] ~= entry or entry.call ~= nil or entry.every == nil or not entry.touched then
		return nil
	end
	ask(self, entry)
	return true
end

-- Puts `data` in the entry unless it is older than what is there. The table that is there stays
-- when the new one is alike (what a page keeps by its identity is not lost to an answer that changed
-- nothing).
local function store(entry, data)
	local version = nil
	if entry.version_of ~= nil and data ~= nil then
		version = entry.version_of(data)
		if version ~= nil and type(version) ~= "number" then
			error("query: the version of data is a number or nil, not " .. type(version), 0)
		end
		if entry.ready and version ~= nil and entry.version ~= nil and version < entry.version then
			return false
		end
	end
	if not (entry.ready and same(entry.data, data, 1)) then
		entry.data = data
	end
	entry.ready = true
	entry.version = version
	return true
end

-- What the program answered a call: for the key it was asked for, unless the key was let go of or
-- the call given up (its number is no longer the entry's).
function answered(self, entry, id, result, err)
	if entry.call ~= id then
		return nil
	end
	entry.call = nil
	if err ~= nil then
		entry.error = err
	else
		-- A result that is older than what the entry holds is let go of (`store`), and the ask is
		-- over all the same.
		store(entry, result)
		entry.error = nil
	end
	entry.answered = pencere.time.now()
	entry.touched = false
	arm(self, entry)
	-- `fetching` changed whether or not the data did, and what is on the screen asks `get` again.
	return true
end

-- --- Slots, and what is let go ---------------------------------------------------------------------------

-- A key takes the place of the one that had a slot before it, and the call of the one that is left
-- is given up at the next wake: a `get` that comes after the claim, in the same render, asks the one
-- that is left (a part of the page that has it with no slot, or in another) and keeps it. So does a
-- `get` before the claim at the time of the claim (a render is told by its time: the page is made for
-- an event at the time of the frame before it, so that two renders may have one).
local function leave(self)
	self.leaver = nil
	local leaving = self.leaving
	self.leaving = {}
	for _, item in leaving do
		local entry = item.entry
		local asked_since = entry.asked_tick > item.tick
		local asked_here = entry.asked_at == item.at and entry.slot ~= item.slot
		if
			self.entries[entry.key] == entry
			and entry.call ~= nil
			and self.slots[item.slot] ~= entry
			and not asked_since
			and not asked_here
		then
			pencere.cancel(entry.call)
			entry.call = nil
			if entry.ready then
				-- Its data stays, and the ask that was lost is made again when it is asked for.
				entry.stale = true
			else
				drop(self, entry)
			end
		end
	end
	return nil
end

local function claim(self, name, entry, now)
	local holder = self.slots[name]
	self.slots[name] = entry
	if holder == nil or holder == entry or holder.call == nil then
		return
	end
	table.insert(self.leaving, { entry = holder, slot = name, at = now, tick = self.tick })
	if self.leaver == nil then
		self.leaver = pencere.time.after(0, function()
			return leave(self)
		end)
	end
end

-- A `get` that comes at a time the last did not is a new render: what no render has asked for in
-- `keep` seconds before the last one is let go. The time that passes with no render (a pane that is
-- not seen) is not counted: the pane that comes back has its data.
local function sweep(self, now)
	local before = self.seen
	self.seen = now
	if before == nil or before == now then
		return
	end
	local oldest = before - self.keep
	for _, entry in self.entries do
		if entry.used < oldest then
			drop(self, entry)
		end
	end
end

-- --- The context ----------------------------------------------------------------------------------

-- A context for a page: `options.keep` is how long, in seconds of the renders, data that is not asked
-- for is kept (300).
function query.new(options)
	check_options(options, NEW_OPTIONS, "query.new")
	local keep = KEEP
	if options ~= nil and options.keep ~= nil then
		keep = options.keep
		if type(keep) ~= "number" or keep ~= keep or keep < 0 then
			fail(2, "query.new takes keep as a number of seconds, 0 or more")
		end
	end
	return setmetatable({
		keep = keep,
		-- By key.
		entries = {},
		-- The key that has each slot, by name.
		slots = {},
		-- The time of the last `get`.
		seen = nil,
		-- The calls to give up at the next wake: { entry =, slot =, at =, tick = }.
		leaving = {},
		leaver = nil,
		-- The number of `get`s made.
		tick = 0,
	}, Query)
end

-- What `method` is known to say for `params`, asking the program where it is missing or stale:
-- `{ status =, data =, error =, fetching = }`. `options`:
--   every   seconds: asks again that long after each answer, for as long as the page asks for it
--   stamp   a value the page makes of what the data is asked from: it is asked again when it is not
--           the one the data was asked under (compared with ==); the old data stays meanwhile
--   slot    a name: one key at a time has the slot, and the call of the one before it is given up
--           (its data stays)
--   version a function of data that says its version, a number: an answer or a `set` that is older
--           than the data is let go
-- `every` and `version` are the last `get`'s of a key.
function Query:get(method, params, options)
	check_self(self, "get")
	check_call(method, params, "get")
	check_options(options, GET_OPTIONS, "query:get")
	local key, text = key_of(method, params, 3)
	local every, stamp, slot, version = nil, nil, nil, nil
	if options ~= nil then
		every, stamp, slot, version = options.every, options.stamp, options.slot, options.version
		if every ~= nil and not (type(every) == "number" and every > 0 and every < math.huge) then
			fail(2, "query:get takes every as a number of seconds, more than 0")
		end
		if slot ~= nil and type(slot) ~= "string" then
			fail(2, "query:get takes slot as a name, a string, not " .. type(slot))
		end
		if version ~= nil and type(version) ~= "function" then
			fail(2, "query:get takes version as a function, not " .. type(version))
		end
	end
	local now = pencere.time.now()
	sweep(self, now)
	local entry = self.entries[key]
	if entry == nil then
		entry = new_entry(key, method, params, text, now)
		self.entries[key] = entry
	end
	self.tick += 1
	entry.used = now
	entry.asked_at = now
	entry.asked_tick = self.tick
	entry.slot = slot
	entry.touched = true
	entry.every = every
	entry.version_of = version
	entry.want_stamp = stamp
	if entry.adopt then
		entry.stamp = stamp
		entry.adopt = false
	end
	if slot ~= nil then
		claim(self, slot, entry, now)
	end
	if entry.call == nil then
		local due = entry.stale or (stamp ~= nil and stamp ~= entry.stamp)
		if not due and every ~= nil and entry.answered ~= nil and entry.timer == nil then
			-- The polling stopped, for no one asked while it ran: it goes on from the last answer.
			local left = entry.answered + every - now
			if left <= 0 then
				due = true
			else
				arm(self, entry, left)
			end
		end
		if due then
			ask(self, entry)
		end
	end
	return view(entry)
end

-- What is known of the key, as `get` says it, or nil where nothing is: it asks nothing and counts as
-- no use of the data. For handlers, which do not render.
function Query:peek(method, params)
	check_self(self, "peek")
	check_call(method, params, "peek")
	local key = key_of(method, params, 3)
	local entry = self.entries[key]
	if entry == nil then
		return nil
	end
	return view(entry)
end

-- The entries `params` says: the key of the method and params, or every key of the method where
-- there are no params.
local function entries_of(self, method, params)
	if params ~= nil then
		local entry = self.entries[(key_of(method, params, 4))]
		return entry and { entry } or {}
	end
	local found = {}
	for _, entry in self.entries do
		if entry.method == method then
			table.insert(found, entry)
		end
	end
	return found
end

-- The key is stale: the next `get` asks (a call that is out is not given up: its answer is written
-- and the key stays stale). Where there are no params, every key of the method.
function Query:invalidate(method, params)
	check_self(self, "invalidate")
	check_call(method, params, "invalidate")
	for _, entry in entries_of(self, method, params) do
		entry.stale = true
	end
end

-- Puts data in a key, as the program pushed it (the last of this and an answer is what stays,
-- unless `version` says one is older). The next `get` does not ask on its account: a key that `set`
-- makes takes the stamp of the first `get` that follows, as what it holds is as new as the page says.
function Query:set(method, params, data)
	check_self(self, "set")
	check_call(method, params, "set")
	local key, text = key_of(method, params, 3)
	local entry = self.entries[key]
	if entry == nil then
		local now = pencere.time.now()
		entry = new_entry(key, method, params, text, now)
		entry.stale = false
		entry.adopt = true
		-- What the program pushed is as new as now: a `get` that asks `every` seconds asks from here.
		entry.answered = now
		self.entries[key] = entry
	end
	if store(entry, data) then
		entry.error = nil
	end
end

-- Lets go of the key: its data, and its call (the program is told it is given up and what it says
-- of it is not heard). Where there are no params, every key of the method.
function Query:forget(method, params)
	check_self(self, "forget")
	check_call(method, params, "forget")
	for _, entry in entries_of(self, method, params) do
		drop(self, entry)
	end
end

return query
