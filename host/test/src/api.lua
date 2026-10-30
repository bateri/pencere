-- `require("pencere.test")`: the API of a test file, written in Lua over the few natives of the
-- runner of tests (`native`). Tests register themselves; the loop at the end (`run`) runs them.
--
-- A test opens a page (`test.open`), does what a person does to it (`press`, `click`, `type`), and
-- looks at what it shows or says (`expect`). The page is found as a person finds it, by its words
-- and its role (`get_by_text`, `get_by_role`), and what is looked for is looked for again until it
-- is there when the page has a program of its own that keeps its own time.

local native = ...

local M = {}

-- Fails the test at the line of the test file that made the failing call: the first caller that is
-- not this module.
local function fail(message)
	local level = 2
	while true do
		local source = debug.info(level, "s")
		if source == nil then
			error(message, 0)
		end
		if source ~= "pencere.test" then
			error(message, level)
		end
		level += 1
	end
end

-- ---- values, as a failure shows them --------------------------------------------------------

local function sorted_keys(t)
	local keys = {}
	for key in pairs(t) do
		table.insert(keys, key)
	end
	table.sort(keys, function(a, b)
		local ta, tb = type(a), type(b)
		if ta ~= tb then
			return ta < tb
		end
		if ta == "number" or ta == "string" then
			return a < b
		end
		return tostring(a) < tostring(b)
	end)
	return keys
end

local function inspect(value, depth, seen)
	depth = depth or 0
	local kind = type(value)
	if kind == "string" then
		return string.format("%q", value)
	elseif kind ~= "table" then
		return tostring(value)
	end
	seen = seen or {}
	if seen[value] or depth > 6 then
		return "{...}"
	end
	seen[value] = true
	local count = 0
	for _ in pairs(value) do
		count += 1
	end
	if count == 0 then
		seen[value] = nil
		return "{}"
	end
	local parts = {}
	local list = #value == count
	for _, key in ipairs(sorted_keys(value)) do
		local shown = inspect(value[key], depth + 1, seen)
		if list then
			table.insert(parts, shown)
		elseif type(key) == "string" and string.match(key, "^[%a_][%w_]*$") then
			table.insert(parts, key .. " = " .. shown)
		else
			table.insert(parts, "[" .. inspect(key, depth + 1, seen) .. "] = " .. shown)
		end
	end
	seen[value] = nil
	local one = "{ " .. table.concat(parts, ", ") .. " }"
	if #one <= 100 or depth > 0 then
		return one
	end
	return "{\n  " .. table.concat(parts, ",\n  ") .. ",\n}"
end

-- Where two values differ, in words: the path to the first difference.
local function difference(a, b, path)
	path = path or "value"
	if rawequal(a, b) then
		return nil
	end
	if type(a) ~= type(b) then
		return string.format("%s is %s, expected %s", path, inspect(a), inspect(b))
	end
	if type(a) ~= "table" then
		return string.format("%s is %s, expected %s", path, inspect(a), inspect(b))
	end
	for _, key in ipairs(sorted_keys(b)) do
		if a[key] == nil then
			return string.format("%s has no %s, expected %s", path, inspect(key), inspect(b[key]))
		end
	end
	for _, key in ipairs(sorted_keys(a)) do
		if b[key] == nil then
			return string.format("%s has %s = %s, which is not expected", path, inspect(key), inspect(a[key]))
		end
		local found = difference(a[key], b[key], path .. "[" .. inspect(key) .. "]")
		if found then
			return found
		end
	end
	return nil
end

local function deep_equal(a, b)
	return difference(a, b) == nil
end

-- ---- registering tests ----------------------------------------------------------------------

local tests = {}
local suites = { { name = nil, before = {}, after = {} } }
local defaults = {}

local function position(level)
	local source, line = debug.info(level + 1, "sl")
	return source, line
end

local function register(name, a, b, mode)
	if type(name) ~= "string" then
		fail("a test is named by a string: test(name, fn)")
	end
	local options, fn = {}, a
	if type(a) == "table" then
		options, fn = a, b
	end
	if type(fn) ~= "function" then
		fail("a test is a function: test(name, fn)")
	end
	local _, line = position(3)
	local names, chain = {}, {}
	for _, suite in ipairs(suites) do
		if suite.name then
			table.insert(names, suite.name)
		end
		table.insert(chain, suite)
	end
	table.insert(names, name)
	table.insert(tests, {
		name = table.concat(names, " › "),
		fn = fn,
		line = line,
		timeout = options.timeout,
		mode = mode,
		chain = chain,
	})
end

local function hook(list)
	return function(fn)
		if type(fn) ~= "function" then
			fail("a hook is a function")
		end
		table.insert(suites[#suites][list], fn)
	end
end

M.before_each = hook("before")
M.after_each = hook("after")

function M.describe(name, fn)
	if type(name) ~= "string" or type(fn) ~= "function" then
		fail("a group is test.describe(name, fn)")
	end
	table.insert(suites, { name = name, before = {}, after = {} })
	local ok, err = pcall(fn)
	table.remove(suites)
	if not ok then
		error(err, 0)
	end
end

function M.only(name, a, b)
	register(name, a, b, "only")
end

function M.skip(name, a, b)
	register(name, a, b, "skip")
end

-- Defaults for the options of every `open` of this file.
function M.defaults(options)
	for key, value in pairs(options) do
		defaults[key] = value
	end
end

function M.temp_dir()
	return native.temp_dir()
end

-- ---- a page's description, as a failure shows it --------------------------------------------

local function describe_page(page)
	local snapshot = page.raw:snapshot()
	local lines = {}
	local things = {}
	for _, item in ipairs(snapshot.accessible) do
		if item.role ~= "text" then
			local role = item.role == "heading" and ("heading " .. tostring(item.level)) or item.role
			table.insert(things, string.format("  %s %q", role, item.name))
		end
	end
	if #things > 0 then
		table.insert(lines, "the page has:")
		for index, thing in ipairs(things) do
			if index > 20 then
				table.insert(lines, string.format("  … and %d more", #things - 20))
				break
			end
			table.insert(lines, thing)
		end
	end
	local words = page.raw:words()
	if #words > 0 then
		local shown = {}
		for index, word in ipairs(words) do
			if index > 30 then
				table.insert(shown, "…")
				break
			end
			table.insert(shown, string.format("%q", word))
		end
		table.insert(lines, "the page says: " .. table.concat(shown, " "))
	else
		table.insert(lines, "the page says nothing")
	end
	return table.concat(lines, "\n")
end

-- ---- locators -------------------------------------------------------------------------------

local ROLES = {
	button = "button",
	field = "field",
	textbox = "field",
	editor = "editor",
	text = "text",
	link = "link",
	checkbox = "checkbox",
	switch = "switch",
	radio = "radio",
	tab = "tab",
	menuitem = "menuitem",
	heading = "heading",
	image = "image",
	progress = "progress",
	progressbar = "progress",
	slider = "slider",
}

local function lower(text)
	return string.lower(text or "")
end

-- Whether `words` is what `wanted` asks for: the whole of it with `exact`, else a part of it, in
-- either case.
local function matches(words, wanted, exact)
	if wanted == nil then
		return true
	end
	if exact then
		return words == wanted
	end
	return string.find(lower(words), lower(wanted), 1, true) ~= nil
end

local function inside(inner, outer)
	local x, y = inner.x + 1, inner.y + 1
	return x >= outer.x and y >= outer.y and x < outer.x + outer.w and y < outer.y + outer.h
end

local Locator = {}
Locator.__index = Locator

local function locator(page, kind, args, scope)
	return setmetatable({ page = page, kind = kind, args = args, scope = scope, pick = nil }, Locator)
end

local function describe_locator(loc)
	local args = loc.args
	local text
	if loc.kind == "text" then
		text = string.format("text %q", args.words)
	elseif loc.kind == "role" then
		text = "role " .. args.role
		if args.name then
			text ..= string.format(" named %q", args.name)
		end
	elseif loc.kind == "field" then
		text = string.format("field %q", args.words)
	else
		text = string.format("id %q", args.id)
	end
	if loc.scope then
		text ..= " in (" .. describe_locator(loc.scope) .. ")"
	end
	if loc.pick then
		text ..= " (" .. tostring(loc.pick) .. ")"
	end
	return text
end

-- Everything on the page the locator names, as the page is now: { x, y, w, h, words, role, state }.
local function resolve(loc)
	local snapshot = loc.page.raw:snapshot()
	local found = {}
	local args = loc.args
	if loc.kind == "text" then
		for _, run in ipairs(snapshot.runs) do
			if matches(run.text, args.words, args.exact) then
				table.insert(found, { x = run.x, y = run.y, w = run.w, h = run.h, words = run.text, role = "text", reachable = true })
			end
		end
	elseif loc.kind == "role" then
		for _, item in ipairs(snapshot.accessible) do
			local state = item.state
			if
				item.role == args.role
				and matches(item.name, args.name, args.exact)
				and (args.level == nil or item.level == args.level)
				and (args.checked == nil or state.checked == args.checked)
				and (args.selected == nil or state.selected == args.selected)
				and (args.disabled == nil or state.disabled == args.disabled)
				and (args.expanded == nil or state.expanded == args.expanded)
			then
				table.insert(found, item)
			end
		end
	elseif loc.kind == "field" then
		for _, field in ipairs(snapshot.fields) do
			if matches(field.label, args.words, args.exact) or matches(field.hint, args.words, args.exact) then
				table.insert(found, { x = field.x, y = field.y, w = field.w, h = field.h, words = field.label or field.hint or "", role = "field", field = field, reachable = true })
			end
		end
	else
		local rect = loc.page.raw:bounds(args.id)
		if rect then
			table.insert(found, { x = rect.x, y = rect.y, w = rect.w, h = rect.h, words = args.id, role = "id", id = args.id, reachable = loc.page.raw:reaches(args.id) })
		end
	end
	if loc.scope then
		local within = resolve(loc.scope)
		local first = within[1]
		if #within > 1 then
			fail("a locator's scope names " .. #within .. " things; it must name one")
		end
		local kept = {}
		for _, item in ipairs(found) do
			if first and inside(item, first) then
				table.insert(kept, item)
			end
		end
		found = kept
	end
	if loc.pick == "first" then
		found = { found[1] }
	elseif loc.pick == "last" then
		found = { found[#found] }
	elseif type(loc.pick) == "number" then
		found = { found[loc.pick] }
	end
	return found
end

local function where(item)
	return string.format("at %d,%d (%d × %d)", item.x, item.y, item.w, item.h)
end

-- The one thing the locator names, or the failure that says what is on the page instead.
local function the_one(loc, what)
	local found = resolve(loc)
	if #found == 0 then
		fail(string.format("%s: nothing on the page matches %s\n%s", what, describe_locator(loc), describe_page(loc.page)))
	end
	if #found > 1 then
		local lines = {}
		for index, item in ipairs(found) do
			table.insert(lines, string.format("  %d: %s %q %s", index, item.role, item.words, where(item)))
		end
		fail(string.format(
			"%s: %d things on the page match %s (pick one with :nth(i), :first() or :last(), or say more)\n%s",
			what,
			#found,
			describe_locator(loc),
			table.concat(lines, "\n")
		))
	end
	return found[1]
end

local function point_of(item)
	if item.role == "text" and item.id == nil and item.h and item.h > 0 then
		return item.x + 1, item.y + item.h / 2
	end
	return item.x + item.w / 2, item.y + item.h / 2
end

local function reach(loc, what)
	local item = the_one(loc, what)
	if item.role == "id" then
		return item, nil, nil
	end
	if not item.reachable then
		fail(string.format(
			"%s: %s is on the page, but a press at its middle does not reach it (it is covered or scrolled away)",
			what,
			describe_locator(loc)
		))
	end
	local x, y = point_of(item)
	return item, x, y
end

local function by_point(name, raw_at, raw_id)
	Locator[name] = function(self, ...)
		local item, x, y = reach(self, name)
		if item.role == "id" then
			return self.page.raw[raw_id](self.page.raw, item.id, ...)
		end
		return self.page.raw[raw_at](self.page.raw, x, y, ...)
	end
end

by_point("click", "click_at", "click_id")
by_point("double_click", "double_click_at", "double_click_id")
by_point("right_click", "right_click_at", "right_click_id")
by_point("hover", "move_to", "hover_id")

-- locator:click({ x =, y = }): the point is that far from the top-left corner of the thing.
local plain_click = Locator.click
function Locator:click(options)
	if options and (options.x or options.y) then
		local item = the_one(self, "click")
		return self.page.raw:click_at(item.x + (options.x or 0), item.y + (options.y or 0))
	end
	return plain_click(self)
end

function Locator:wheel(dx, dy)
	local item, x, y = reach(self, "wheel")
	if item.role == "id" then
		return self.page.raw:wheel_id(item.id, dx or 0, dy or 0)
	end
	return self.page.raw:wheel_at(x, y, dx or 0, dy or 0)
end

function Locator:scroll_to(y)
	local item = the_one(self, "scroll_to")
	if item.role ~= "id" then
		fail("scroll_to: a scrolling box is found by its id (get_by_id)")
	end
	return self.page.raw:scroll_to(item.id, y)
end

function Locator:drop_files(paths)
	local item = the_one(self, "drop_files")
	if item.role ~= "id" then
		fail("drop_files: the thing files are dropped on is found by its id (get_by_id)")
	end
	return self.page.raw:drop_files(item.id, paths)
end

-- Files the person gives the page on this element: `{ { name =, data = }, ... }`, dropped (the
-- default) or pasted (`{ source = "paste" }`). The handles the host made, in order.
function Locator:give_files(files, options)
	local item = the_one(self, "give_files")
	if item.role ~= "id" then
		fail("give_files: the thing files are given on is found by its id (get_by_id)")
	end
	return self.page.raw:give_files(item.id, files, options and options.source)
end

-- Files dragged from outside the page are over this element.
function Locator:drag_files_over(count)
	local item = the_one(self, "drag_files_over")
	if item.role ~= "id" then
		fail("drag_files_over: the thing files are dragged over is found by its id (get_by_id)")
	end
	return self.page.raw:drag_files_over(item.id, count or 1)
end

function Locator:drag_to(other)
	local from, x1, y1 = reach(self, "drag_to")
	local to, x2, y2 = reach(other, "drag_to")
	if from.role == "id" and to.role == "id" then
		return self.page.raw:drag_ids(from.id, to.id)
	end
	x1 = x1 or (from.x + from.w / 2)
	y1 = y1 or (from.y + from.h / 2)
	x2 = x2 or (to.x + to.w / 2)
	y2 = y2 or (to.y + to.h / 2)
	return self.page.raw:drag_between(x1, y1, x2, y2)
end

function Locator:fill(text)
	local item = the_one(self, "fill")
	local id = item.id or (item.field and item.field.id)
	if not id then
		fail("fill: " .. describe_locator(self) .. " is not a field of the page")
	end
	return self.page.raw:fill_id(id, text)
end

function Locator:count()
	return #resolve(self)
end

function Locator:exists()
	return #resolve(self) > 0
end

function Locator:all()
	local all = {}
	for index = 1, #resolve(self) do
		table.insert(all, self:nth(index))
	end
	return all
end

function Locator:nth(index)
	local copy = locator(self.page, self.kind, self.args, self.scope)
	copy.pick = index
	return copy
end

function Locator:first()
	local copy = locator(self.page, self.kind, self.args, self.scope)
	copy.pick = "first"
	return copy
end

function Locator:last()
	local copy = locator(self.page, self.kind, self.args, self.scope)
	copy.pick = "last"
	return copy
end

function Locator:words()
	return the_one(self, "words").words
end

function Locator:bounds()
	local item = the_one(self, "bounds")
	return { x = item.x, y = item.y, w = item.w, h = item.h }
end

function Locator:state()
	local item = the_one(self, "state")
	return item.state or {}
end

-- Locators inside this one's rectangle.
function Locator:get_by_text(words, options)
	return locator(self.page, "text", { words = words, exact = options and options.exact }, self)
end

function Locator:get_by_role(role, options)
	return M.__role(self.page, role, options, self)
end

function Locator:get_by_field(words, options)
	return locator(self.page, "field", { words = words, exact = options and options.exact }, self)
end

function Locator:get_by_id(id)
	return locator(self.page, "id", { id = id }, self)
end

function M.__role(page, role, options, scope)
	local canonical = ROLES[role]
	if not canonical then
		fail("there is no role " .. tostring(role) .. " (the roles are button, field, editor, text, link, checkbox, switch, radio, tab, menuitem, heading, image, progress, slider)")
	end
	options = options or {}
	return locator(page, "role", {
		role = canonical,
		name = options.name,
		exact = options.exact,
		level = options.level,
		checked = options.checked,
		selected = options.selected,
		disabled = options.disabled,
		expanded = options.expanded,
	}, scope)
end

-- ---- the page --------------------------------------------------------------------------------

local Page = {}
Page.__index = Page

-- What a page does, passed on to the page of the runner as it is.
for _, name in ipairs({
	"press", "insert", "paste", "compose", "unmark", "move_to", "leave", "drag_between", "wheel_at",
	"click_at", "double_click_at", "right_click_at", "focus_editor", "select", "caret_to", "edit",
	"command", "pick", "resize", "zoom", "set_active", "set_terminal_look", "refresh", "advance", "step", "idle", "hide",
	"now", "asks_at", "settle", "tell", "words", "says", "words_in", "tree_words", "bounds",
	"editor", "editor_text", "focus", "field", "outline", "cursor", "pasteboard", "links", "menu",
	"closed", "diagnostics", "ended", "sent", "received", "push", "answer", "fail", "lent",
	"give_back", "log", "fill", "scroll_to", "wheel", "drop_files", "click_gutter", "hover_gutter",
	"gutter_cell", "zone", "anchor_rect", "dwell", "hold", "release", "click_holding",
	"give_files", "drag_files_over", "drag_files_away", "panel", "choose_files", "choose_place",
	"attach",
}) do
	Page[name] = function(self, ...)
		return self.raw[name](self.raw, ...)
	end
end

-- page:type(text): characters, one key each.
Page["type"] = function(self, text)
	return self.raw:type_text(text)
end

-- page:click_at(x, y), … by id.
function Page:click_id(id)
	return self.raw:click_id(id)
end

function Page:calls(method)
	local all = self.raw:calls()
	if method == nil then
		return all
	end
	local some = {}
	for _, call in ipairs(all) do
		if call.method == method then
			table.insert(some, call)
		end
	end
	return some
end

function Page:last_call(method)
	local some = self:calls(method)
	return some[#some]
end

function Page:subscriptions(topic)
	local all = self.raw:subscriptions()
	if topic == nil then
		return all
	end
	local some = {}
	for _, subscription in ipairs(all) do
		if subscription.topic == topic then
			table.insert(some, subscription)
		end
	end
	return some
end

function Page:get_by_text(words, options)
	return locator(self, "text", { words = words, exact = options and options.exact })
end

function Page:get_by_role(role, options)
	return M.__role(self, role, options, nil)
end

function Page:get_by_field(words, options)
	return locator(self, "field", { words = words, exact = options and options.exact })
end

function Page:get_by_id(id)
	return locator(self, "id", { id = id })
end

-- Waits, at most `within` seconds, for `fn()` to be true, giving the page and its program the time
-- between looks. The page's view being over (its program closed it, showed another, lent the
-- terminal) does not end the wait: what the program does next is what `fn` may be waiting for, and a
-- program goes on after its `serve` on its own time. If it never comes, the failure says the view
-- is over.
function Page:wait_for(fn, options)
	local within = options and options.within or self.within
	local deadline = native.clock() + within
	local over = nil
	while not fn() do
		if native.clock() > deadline then
			fail(string.format(
				"the page did not come to what was waited for within %gs%s\n%s",
				within,
				over and ("; its view is over: " .. over) or "",
				describe_page(self)
			))
		end
		over = self.raw:wait()
	end
end

function Page:reopen()
	return self.raw:reopen()
end

function Page:shot(name, options)
	return self.raw:shot(name or "shot", options and options.theme or "dark")
end

-- Whether the page has a program that keeps its own time: what is looked for is looked for again.
function Page:waits()
	return self.raw:waits()
end

-- ---- opening ---------------------------------------------------------------------------------

-- What a native raises comes with the library's label and where in Rust it was: the test is told
-- the words, and the line of the test that made the call.
local function plain(message)
	message = string.gsub(tostring(message), "^runtime error: ", "")
	message = string.gsub(message, "\nstack traceback:.*$", "")
	return message
end

local function guarded(raw)
	return setmetatable({}, {
		__index = function(_, name)
			local method = raw[name]
			if type(method) ~= "function" then
				return method
			end
			return function(_, ...)
				local results = table.pack(pcall(method, raw, ...))
				if results[1] then
					return table.unpack(results, 2, results.n)
				end
				fail(plain(results[2]))
			end
		end,
	})
end

function M.open(options)
	options = options or {}
	local merged = {}
	for key, value in pairs(defaults) do
		merged[key] = value
	end
	for key, value in pairs(options) do
		merged[key] = value
	end
	-- The stand-in program's handlers say how they ended, so that a failure of one is the call's
	-- answer and not the test's: `error({ code = "not-found", message = "…" })`.
	if merged.calls then
		local wrapped = {}
		for method, handler in pairs(merged.calls) do
			wrapped[method] = function(params)
				if type(handler) ~= "function" then
					return true, handler
				end
				local ok, result = pcall(handler, params)
				if ok then
					return true, result
				end
				if type(result) == "table" then
					return false, result
				end
				return false, { code = "internal", message = tostring(result) }
			end
		end
		merged.calls = wrapped
	end
	local ok, raw = pcall(native.open, merged)
	if not ok then
		fail(plain(raw))
	end
	return setmetatable({ raw = guarded(raw), within = merged.within or 5 }, Page)
end

-- ---- expect ----------------------------------------------------------------------------------

local Expect = {}
Expect.__index = Expect

local function kind_of(subject)
	if type(subject) == "table" then
		local meta = getmetatable(subject)
		if meta == Page then
			return "page"
		elseif meta == Locator then
			return "locator"
		end
	end
	return "value"
end


-- Checks `check()` (→ ok, detail) against what was expected: once for a page whose program is not
-- waited for, else again until it comes out as wanted or the time is over.
local function eventually(self, check)
	local want = not self.negated
	local page = self.subject
	if kind_of(page) == "locator" then
		page = page.page
	end
	if kind_of(page) ~= "page" or not page:waits() then
		local ok, detail = check()
		return ok == want, detail
	end
	local deadline = native.clock() + page.within
	while true do
		local ok, detail = check()
		if ok == want then
			return true, detail
		end
		if native.clock() > deadline then
			return false, detail
		end
		page.raw:tick()
	end
end

local function expectation(self, ok, detail, expected)
	if not ok then
		local message = expected
		if self.negated then
			message = "expected not: " .. expected
		end
		fail(message .. (detail and ("\n" .. detail) or ""))
	end
	return self
end

function M.expect(subject)
	local object = setmetatable({ subject = subject, negated = false }, Expect)
	object.never = setmetatable({ subject = subject, negated = true }, Expect)
	return object
end

local function value_check(self, truth, expected, detail)
	local ok = truth
	if self.negated then
		ok = not truth
	end
	return expectation(self, ok, detail, expected)
end

local function need_value(self, matcher)
	if kind_of(self.subject) ~= "value" then
		fail(matcher .. " compares a value; a page or a locator is looked at with to_say, to_exist …")
	end
end

function Expect:to_be(expected)
	need_value(self, "to_be")
	local detail
	if not self.negated then
		detail = "received " .. inspect(self.subject)
	end
	return value_check(self, rawequal(self.subject, expected), "expected " .. inspect(self.subject) .. " to be " .. inspect(expected), detail)
end

function Expect:to_equal(expected)
	need_value(self, "to_equal")
	local found = difference(self.subject, expected)
	return value_check(self, found == nil, "expected values to be equal", found and ("difference: " .. found .. "\nreceived " .. inspect(self.subject) .. "\nexpected " .. inspect(expected)))
end

function Expect:to_be_truthy()
	need_value(self, "to_be_truthy")
	return value_check(self, self.subject and true or false, "expected " .. inspect(self.subject) .. " to be truthy")
end

function Expect:to_be_nil()
	need_value(self, "to_be_nil")
	return value_check(self, self.subject == nil, "expected " .. inspect(self.subject) .. " to be nil")
end

function Expect:to_contain(item)
	need_value(self, "to_contain")
	local subject = self.subject
	local has = false
	if type(subject) == "string" then
		has = string.find(subject, item, 1, true) ~= nil
	elseif type(subject) == "table" then
		for _, element in ipairs(subject) do
			if type(element) == "string" and type(item) == "string" then
				if string.find(element, item, 1, true) then
					has = true
					break
				end
			elseif deep_equal(element, item) then
				has = true
				break
			end
		end
	else
		fail("to_contain looks in a string or a list, not " .. inspect(subject))
	end
	return value_check(self, has, "expected " .. inspect(subject) .. " to contain " .. inspect(item))
end

function Expect:to_match(pattern)
	need_value(self, "to_match")
	local text = tostring(self.subject)
	return value_check(self, string.find(text, pattern) ~= nil, "expected " .. inspect(self.subject) .. " to match the pattern " .. inspect(pattern))
end

function Expect:to_have_length(length)
	need_value(self, "to_have_length")
	local subject = self.subject
	local actual = type(subject) == "string" and #subject or (type(subject) == "table" and #subject or nil)
	if actual == nil then
		fail("to_have_length measures a string or a list, not " .. inspect(subject))
	end
	return value_check(self, actual == length, string.format("expected a length of %d, received %d", length, actual or -1))
end

function Expect:to_be_close_to(number, digits)
	need_value(self, "to_be_close_to")
	digits = digits or 2
	local tolerance = 0.5 * 10 ^ -digits
	local ok = type(self.subject) == "number" and math.abs(self.subject - number) < tolerance
	return value_check(self, ok, string.format("expected %s to be within %g of %s", inspect(self.subject), tolerance, inspect(number)))
end

function Expect:to_throw(words)
	need_value(self, "to_throw")
	if type(self.subject) ~= "function" then
		fail("to_throw takes a function to call, not " .. inspect(self.subject))
	end
	local ok, err = pcall(self.subject)
	local threw = not ok
	if threw and words ~= nil then
		threw = string.find(tostring(err), words, 1, true) ~= nil
	end
	local expected = "expected the function to throw"
	if words ~= nil then
		expected ..= " " .. inspect(words)
	end
	return value_check(self, threw, expected, ok and "it returned" or ("it threw " .. inspect(err)))
end

-- ---- what is expected of a page ----------------------------------------------------------------

local function need_page(self, matcher)
	if kind_of(self.subject) ~= "page" then
		fail(matcher .. " looks at a page: expect(page)")
	end
	return self.subject
end

local function says(page, words, across)
	if across then
		local joined = table.concat(page.raw:words(), " ")
		return string.find(joined, words, 1, true) ~= nil
	end
	return page.raw:says(words)
end

function Expect:to_say(words, options)
	local kind = kind_of(self.subject)
	if kind == "locator" then
		local loc = self.subject
		local ok, detail = eventually(self, function()
			local found = resolve(loc)
			for _, item in ipairs(found) do
				if string.find(item.words, words, 1, true) then
					return true
				end
			end
			return false, #found == 0 and ("nothing matches " .. describe_locator(loc) .. "\n" .. describe_page(loc.page)) or nil
		end)
		return expectation(self, ok, ok and nil or detail, string.format("expected %s to say %q", describe_locator(loc), words))
	end
	local page = need_page(self, "to_say")
	local across = options and options.across
	local ok, detail = eventually(self, function()
		if says(page, words, across) then
			return true
		end
		return false, describe_page(page)
	end)
	return expectation(self, ok, (not ok) and detail or nil, string.format("expected the page to say %q", words))
end

function Expect:to_have_called(method, params)
	local page = need_page(self, "to_have_called")
	local ok, detail = eventually(self, function()
		local made = page:calls(method)
		if #made == 0 then
			return false, "the page called: " .. (#page:calls() == 0 and "nothing" or (function()
				local names = {}
				for _, call in ipairs(page:calls()) do
					table.insert(names, call.method)
				end
				return table.concat(names, ", ")
			end)())
		end
		if params == nil then
			return true
		end
		for _, call in ipairs(made) do
			if deep_equal(call.params, params) then
				return true
			end
		end
		return false, "its calls had these params: " .. inspect((function()
			local all = {}
			for _, call in ipairs(made) do
				table.insert(all, call.params)
			end
			return all
		end)()) .. "\nexpected " .. inspect(params)
	end)
	return expectation(self, ok, (not ok) and detail or nil, string.format("expected the page to have called %q", method))
end

function Expect:to_have_subscribed(topic)
	local page = need_page(self, "to_have_subscribed")
	local ok = eventually(self, function()
		return #page:subscriptions(topic) > 0
	end)
	return expectation(self, ok, nil, string.format("expected the page to have subscribed to %q", topic))
end

function Expect:to_have_link(url)
	local page = need_page(self, "to_have_link")
	local ok, detail = eventually(self, function()
		for _, link in ipairs(page.raw:links()) do
			if link == url then
				return true
			end
		end
		return false, "the page opened these links: " .. inspect(page.raw:links())
	end)
	return expectation(self, ok, (not ok) and detail or nil, string.format("expected the page to have opened the link %q", url))
end

function Expect:to_have_focus(id)
	local page = need_page(self, "to_have_focus")
	local ok, detail = eventually(self, function()
		return page.raw:has_focus(id), "the keyboard is at " .. inspect(page.raw:focus())
	end)
	return expectation(self, ok, (not ok) and detail or nil, string.format("expected the keyboard to be at %s", inspect(id)))
end

function Expect:to_be_closed()
	local page = need_page(self, "to_be_closed")
	local ok = eventually(self, function()
		return page.raw:closed()
	end)
	return expectation(self, ok, nil, "expected the page to have asked to be closed")
end

-- ---- what is expected of a locator --------------------------------------------------------------

local function need_locator(self, matcher)
	if kind_of(self.subject) ~= "locator" then
		fail(matcher .. " looks at a thing of the page: expect(page:get_by_…())")
	end
	return self.subject
end

local function on_locator(self, matcher, expected, test)
	local loc = need_locator(self, matcher)
	local ok, detail = eventually(self, function()
		local found = resolve(loc)
		if #found == 0 then
			return false, "nothing on the page matches " .. describe_locator(loc) .. "\n" .. describe_page(loc.page)
		end
		if #found > 1 then
			return false, tostring(#found) .. " things match " .. describe_locator(loc)
		end
		return test(found[1])
	end)
	return expectation(self, ok, (not ok) and detail or nil, string.format(expected, describe_locator(loc)))
end

function Expect:to_exist()
	local loc = need_locator(self, "to_exist")
	local ok, detail = eventually(self, function()
		return #resolve(loc) > 0, describe_page(loc.page)
	end)
	return expectation(self, ok, (not ok) and detail or nil, "expected " .. describe_locator(loc) .. " to exist")
end

function Expect:to_have_count(count)
	local loc = need_locator(self, "to_have_count")
	local ok, detail = eventually(self, function()
		local actual = #resolve(loc)
		return actual == count, "found " .. actual
	end)
	return expectation(self, ok, (not ok) and detail or nil, string.format("expected %s to match %d things", describe_locator(loc), count))
end

function Expect:to_be_checked()
	return on_locator(self, "to_be_checked", "expected %s to be checked", function(item)
		return item.state ~= nil and item.state.checked == true
	end)
end

function Expect:to_be_selected()
	return on_locator(self, "to_be_selected", "expected %s to be selected", function(item)
		return item.state ~= nil and item.state.selected == true
	end)
end

function Expect:to_be_disabled()
	return on_locator(self, "to_be_disabled", "expected %s to be disabled", function(item)
		return item.state ~= nil and item.state.disabled == true
	end)
end

function Expect:to_have_value(value)
	return on_locator(self, "to_have_value", "expected %s to have the value " .. inspect(value), function(item)
		local actual = item.state and item.state.value
		return actual == value, "its value is " .. inspect(actual)
	end)
end

function Expect:to_be_reachable()
	return on_locator(self, "to_be_reachable", "expected %s to be reachable by a press", function(item)
		return item.reachable == true
	end)
end

-- ---- running the tests of the file --------------------------------------------------------------

local function failure_of(err)
	local message = err
	if type(err) == "table" then
		message = err.message or inspect(err)
	end
	message = tostring(message)
	-- What a native or the interpreter raised comes with the library's label.
	return (string.gsub(message, "^runtime error: ", ""))
end

function M.__run()
	local config = native.config()
	local only = false
	for _, t in ipairs(tests) do
		if t.mode == "only" then
			only = true
			break
		end
	end
	if only and config.ci then
		native.file_failed("test.only is not allowed when CI is set: it would make a gate pass by looking at one test")
		return
	end
	local selected = {}
	local names = {}
	for _, t in ipairs(tests) do
		local wanted = true
		if config.grep and not string.find(t.name, config.grep, 1, true) then
			wanted = false
		end
		if config.line and t.line ~= config.line then
			wanted = false
		end
		if only and t.mode ~= "only" then
			wanted = false
		end
		if wanted then
			table.insert(selected, t)
			table.insert(names, t.name)
		end
	end
	native.plan(names)
	for _, t in ipairs(selected) do
		if t.mode == "skip" then
			native.skipped(t.name, t.line)
		else
			native.begin(t.name, t.line, t.timeout)
			local ok, err = xpcall(function()
				for _, suite in ipairs(t.chain) do
					for _, before in ipairs(suite.before) do
						before()
					end
				end
				t.fn()
			end, function(e)
				return { message = failure_of(e), trace = debug.traceback("", 2) }
			end)
			for index = #t.chain, 1, -1 do
				for _, after in ipairs(t.chain[index].after) do
					local fine, problem = xpcall(after, function(e)
						return { message = failure_of(e), trace = debug.traceback("", 2) }
					end)
					if not fine and ok then
						ok, err = false, problem
					end
				end
			end
			if ok then
				native.finished(true)
			else
				native.finished(false, err.message, err.trace)
			end
		end
	end
	if #tests == 0 then
		native.file_failed("no test is registered in this file (test(name, fn))")
	end
end

setmetatable(M, {
	__call = function(_, name, a, b)
		register(name, a, b, nil)
	end,
})

return M
