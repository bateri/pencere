-- @pencere/ui, the structure of a page: a tree, an edge that is dragged to size what is beside it, a
-- bar that says where things stand.
--
--   u:tree("files", { roots = { "src" }, children = list_of, item = describe, state = T })
--       -- select { key }, open { key, how }, expand { key, open }, menu { key, at }
--   u:split("side", { state = { size = 240 }, min = 160, max = 520, first = side, second = main })
--       -- resize { size }, resized { size }, drawer { open }
--   u:status("status", { start = { { key = "branch", words = "main" } }, finish = { … } })   -- press { key }

local core = require("./core")
local lists = require("./lists")
local look = require("./look")
local theme = require("./theme")

local alpha = pencere.alpha

local structure = {}

-- --- A tree ---------------------------------------------------------------------------------------------

-- The width of the cell each level of depth takes, points.
local INDENT = 12

-- The rows of the tree as they stand now: `{ key, depth, parent, open }` for each node that is shown,
-- `open` being nil for a leaf (a node `children` says nothing of), true or false for one that has
-- children.
local function flatten(props, state)
	local rows, parent_of = {}, {}
	local function visit(key, depth, parent)
		local kids = props.children(key)
		local open = nil
		if kids ~= nil then
			open = state.expanded[key] == true
		end
		parent_of[key] = parent
		table.insert(rows, { key = key, depth = depth, parent = parent, open = open })
		if open then
			for _, child in kids do
				visit(child, depth + 1, key)
			end
		end
	end
	for _, root in props.roots do
		visit(root, 0, nil)
	end
	return rows, parent_of
end

-- A tree of what has children (files, symbols, a schema): `roots` are the keys of its top level,
-- `children(key)` the keys under one (nil for what has none; an empty list for a node whose children
-- the page has not read yet, which it reads when it hears `expand`), `item(key)` what to show of it:
-- `{ title, icon, tone, badge, hint }`. `state` is `{ expanded, cursor }`: a set of the keys open and the
-- key chosen, the page's, so that what is open outlives the tree being drawn. A long tree is built as it
-- is scrolled, as a list is.
--
-- The pointer chooses a row (`select`), opens a folder with a press on it or on its arrow
-- (`expand { key, open }`), and opens what has no children with a second press (`open { key, how }`); the
-- right button asks for a menu (`menu { key, at }`). With the keyboard (`keys` maps others, as a list's):
-- the arrow keys, home, end and page up and down move, right opens a folder and goes into it, left
-- closes it and goes out of it, enter opens a file or toggles a folder. `guides` (the default) draws a
-- line down each level of depth.
local Tree = core.component("tree", {
	state = function()
		return { expanded = {}, cursor = nil, marks = {} }
	end,
	check = function(props)
		if type(props.roots) ~= "table" or type(props.children) ~= "function" then
			return "a tree has roots and a way to their children: tree(id, { roots = { key }, children = function(key) … end, item = … })"
		end
		return nil
	end,
	view = function(u, id, props, state)
		local t = u.theme
		state.expanded = state.expanded or {}
		local flat = flatten(props, state)
		local at = {}
		for index, row in flat do
			at[row.key] = row
			row.index = index
			local shown = props.item and props.item(row.key) or { title = tostring(row.key) }
			row.shown = shown
			row.label = shown.title or tostring(row.key)
			row.expanded = row.open
		end
		state.flat = flat
		local guides = props.guides ~= false
		return pencere.div():grow(1):min_h(0):child(u:list(u:part(id, "list"), {
			items = flat,
			state = state,
			height = props.height,
			keys = props.keys,
			role = "button",
			row = function(row, _, look_of)
				local shown = row.shown
				local line = pencere.div():row():items_center():w_full():h_full()
				for _ = 1, row.depth do
					line:child(
						pencere.div():w(INDENT):h_full():no_shrink():items_center():justify_center():child(
							guides and pencere.div():w(1):h_full():bg(alpha(t.line, 0.6)) or pencere.div()
						)
					)
				end
				local chevron = pencere.div():size(16, 16):no_shrink():items_center():justify_center()
				if row.open ~= nil then
					chevron:press(u:part(id, "chevron", row.key))
					chevron:child(look.glyph(row.open and "chevron_down" or "chevron_right", 12, t.ink_dim))
				end
				line:child(chevron)
				local tone = shown.tone and theme.tone(t, shown.tone) or nil
				local ink = tone or t.ink
				if shown.icon ~= nil then
					line:child(pencere.div():mr(6):no_shrink():child(look.icon(shown.icon, t.size.icon - 1, ink)))
				end
				line:child(pencere.text(row.label):color(ink):truncate():grow(1):min_w(0))
				if shown.hint ~= nil then
					line:child(pencere.text(shown.hint):text_size(t.text.small):color(t.ink_quiet):no_shrink():ml(6))
				end
				if shown.badge ~= nil then
					line:child(pencere.div():ml(6):no_shrink():child(u:badge(shown.badge, { tone = shown.badge_tone })))
				end
				return line
			end,
		}))
	end,
	click = function(u, id, props, state, part, key)
		if part ~= "chevron" then
			return nil
		end
		local open = not (state.expanded[key] == true)
		state.expanded[key] = open or nil
		return true, { kind = "expand", key = key, open = open }
	end,
	key = function(u, id, props, state, event)
		local mods = event.mods or {}
		if mods.command or mods.control or mods.alt then
			return nil
		end
		local name = (props.keys and props.keys[event.key]) or event.key
		local flat = state.flat or {}
		local at = nil
		for index, row in flat do
			if row.key == state.cursor then
				at = index
			end
		end
		if at == nil or (name ~= "left" and name ~= "right") then
			return nil
		end
		local row = flat[at]
		local list = u.mounts[u:name(id, "list")]
		if name == "right" then
			if row.open == false then
				state.expanded[row.key] = true
				return true, { kind = "expand", key = row.key, open = true }
			elseif row.open and flat[at + 1] and flat[at + 1].parent == row.key then
				state.cursor = flat[at + 1].key
				return true, { kind = "select", key = state.cursor, how = "key" }
			end
			return false
		end
		if row.open then
			state.expanded[row.key] = nil
			return true, { kind = "expand", key = row.key, open = false }
		elseif row.parent ~= nil then
			state.cursor = row.parent
			return true, { kind = "select", key = row.parent, how = "key" }
		end
		return false
	end,
	action = function(u, id, props, state, act)
		local part = u:part_of(act.from)
		if part == nil or part.mount ~= id or part.part ~= "list" then
			return nil
		end
		local row = nil
		for _, candidate in state.flat or {} do
			if candidate.key == act.key then
				row = candidate
			end
		end
		if act.kind == "select" then
			if act.how == "pointer" and row ~= nil and row.open ~= nil then
				-- A press on a folder opens or closes it.
				local open = not (state.expanded[row.key] == true)
				state.expanded[row.key] = open or nil
				return true, { kind = "expand", key = row.key, open = open }
			end
			return true, { kind = "select", key = act.key, how = act.how }
		elseif act.kind == "open" then
			if row ~= nil and row.open ~= nil and act.how == "key" then
				local open = not (state.expanded[row.key] == true)
				state.expanded[row.key] = open or nil
				return true, { kind = "expand", key = row.key, open = open }
			elseif row ~= nil and row.open ~= nil then
				return true
			end
			return true, { kind = "open", key = act.key, how = act.how }
		elseif act.kind == "menu" then
			return true, { kind = "menu", key = act.key, at = act.at }
		end
		return true
	end,
})
structure.tree = Tree

-- --- An edge that sizes ----------------------------------------------------------------------------------

local GRIP = 7
local STEP = 16

-- Two panes side by side and an edge between them that the person drags to size one: `first` and
-- `second` are elements; `side` ("start", the default, or "end") is the one that has the width,
-- `state.size` points (240 unless `default` says), between `min` and `max`. It says `resize { size }` as
-- the edge goes and `resized { size }` when it is let go (the page keeps it where it keeps its
-- settings); the arrow keys on the edge size it by sixteen points, as a window splitter does. Under
-- `narrow_below` points of page the sized pane leaves the row: `second` has all of it, and the sized
-- pane comes over it with a scrim behind it while `state.drawer` is true (the page opens it, the scrim
-- and escape say `drawer { open = false }`).
local Split = core.component("split", {
	state = function(props)
		return { size = props.default or 240, drawer = false }
	end,
	check = function(props)
		if props.first == nil or props.second == nil then
			return "a split has two panes: split(id, { first =, second =, label = })"
		end
		if type(props.label) ~= "string" then
			return "the edge of a split says what it sizes in words: split(id, { label = \"Sidebar width\" })"
		end
		return nil
	end,
	view = function(u, id, props, state)
		local t = u.theme
		local min, max = props.min or 120, props.max or 640
		state.size = math.min(math.max(state.size or 240, min), max)
		local at_start = props.side ~= "end"
		local sized = at_start and props.first or props.second
		local other = at_start and props.second or props.first
		local narrow = props.narrow_below ~= nil and u.size.w < props.narrow_below
		local root = pencere.div():id(id):row():size_full():min_h(0):min_w(0)
		if narrow then
			root:child(other)
			if state.drawer then
				u:layer(id, { modal = false })
				local card = pencere.div()
					:h_full()
					:w(math.min(state.size, u.size.w - 40))
					:no_shrink()
					:bg(t.surface)
					:border(1, t.line)
					:shadow(0, 0, 24, alpha("shadow", 0.4))
					:motion({ seconds = 0.16, enter = { opacity = 0, dx = at_start and -16 or 16, seconds = 0.16 } })
					:child(sized)
				local drawer = pencere.div():overlay():inset(0, 0, 0, 0):row()
				if not at_start then
					drawer:justify_end()
				end
				local scrim = pencere.div():absolute():inset(0, 0, 0, 0):press(u:part(id, "scrim")):bg(t.scrim)
				drawer:child(scrim):child(card)
				root:child(drawer)
			end
			return root
		end
		local pane = pencere.div():w(state.size):h_full():no_shrink():min_h(0):clip():child(sized)
		local held = u.capture ~= nil and u.capture.mount == id
		local over = u:hovered(u:name(id, "handle")) or held
		local line = pencere.div():w(over and 2 or 1):h_full():bg(over and t.ring or t.line):motion({ seconds = t.motion, color = true })
		local handle = pencere.div()
			:press(u:part(id, "handle"))
			:focusable()
			:role("slider")
			:label(props.label)
			:value(string.format("%d pt", state.size))
			:w(GRIP)
			:h_full()
			:no_shrink()
			:items_center()
			:justify_center()
			:child(line)
		if u:focus_visible(u:name(id, "handle")) then
			handle:bg(alpha("accent", 0.2))
		end
		local rest = pencere.div():grow(1):min_w(0):h_full():min_h(0):child(other)
		if at_start then
			root:child(pane):child(handle):child(rest)
		else
			root:child(rest):child(handle):child(pane)
		end
		return root
	end,
	press = function(u, id, props, state, part, _, event)
		if part == "handle" and event.button == "left" then
			state.origin = state.size
			return false, nil, true
		elseif part == "handle" then
			return false
		end
		return nil
	end,
	drag = function(u, id, props, state, part, _, at)
		if part ~= "handle" or state.origin == nil then
			return nil
		end
		local size = state.origin + (props.side == "end" and -at.dx or at.dx)
		size = math.min(math.max(size, props.min or 120), props.max or 640)
		if size == state.size then
			return false
		end
		state.size = size
		return true, { kind = "resize", size = size }
	end,
	release = function(u, id, props, state, part)
		if part ~= "handle" or state.origin == nil then
			return nil
		end
		state.origin = nil
		return true, { kind = "resized", size = state.size }
	end,
	cursor = function(_, _, _, _, part)
		if part == "handle" then
			return "resize_horizontal"
		end
		return nil
	end,
	click = function(_, _, _, state, part)
		if part == "scrim" then
			state.drawer = false
			return true, { kind = "drawer", open = false }
		end
		return nil
	end,
	dismiss = function(_, _, _, state)
		state.drawer = false
		return true, { kind = "drawer", open = false }
	end,
	key = function(u, id, props, state, event, part)
		if part ~= "handle" then
			return nil
		end
		local mods = event.mods or {}
		if mods.command or mods.control or mods.alt then
			return nil
		end
		local grow = (event.key == "right") == (props.side ~= "end")
		if event.key ~= "left" and event.key ~= "right" then
			return nil
		end
		local size = math.min(math.max(state.size + (grow and STEP or -STEP), props.min or 120), props.max or 640)
		if size == state.size then
			return false
		end
		state.size = size
		return true, { kind = "resized", size = size }
	end,
})
structure.split = Split

-- --- A status bar -------------------------------------------------------------------------------------------

-- How wide an item is as the bar will lay it out, points: the words as a guess (the page cannot
-- measure a text), an icon, and the room around.
local function width_of(item)
	local width = 16
	if item.icon ~= nil then
		width += 18
	end
	if type(item.words) == "string" then
		width += utf8.len(item.words) * 6.6
	elseif type(item.words) == "number" then
		width += #tostring(item.words) * 6.6
	else
		width += 60
	end
	return width
end

-- A bar of what stands where: `start` (before), `center` and `finish` (after) are lists of items `{ key,
-- words, icon, tip, keys, priority, press, tone }`. An item with `press` answers the pointer (`press {
-- key }`) and is a button to a screen reader. Where the page is too narrow for all of them, the items
-- of the least `priority` (5 unless said; the higher stays) go first, and then, among those, the
-- last of `finish`, then `center`, then `start`; how wide an item is is a guess of its words'
-- length until a page can measure text.
local Status = core.component("status", {
	check = function(props)
		for _, group in { props.start or {}, props.center or {}, props.finish or {} } do
			for _, item in group do
				if item.key == nil then
					return "an item of a status bar has a key: { key = \"branch\", words = \"main\" }"
				end
			end
		end
		return nil
	end,
	view = function(u, id, props)
		local t = u.theme
		local groups = { start = props.start or {}, center = props.center or {}, finish = props.finish or {} }
		local order = { "finish", "center", "start" }
		local dropped = {}
		local total = 24
		for _, name in order do
			for _, item in groups[name] do
				total += width_of(item)
			end
		end
		while total > u.size.w do
			local worst, worst_name, worst_priority = nil, nil, math.huge
			for _, name in order do
				local group = groups[name]
				for index = #group, 1, -1 do
					local item = group[index]
					local priority = item.priority or 5
					if not dropped[item] and priority < worst_priority then
						worst, worst_name, worst_priority = item, name, priority
					end
				end
			end
			if worst == nil then
				break
			end
			dropped[worst] = true
			total -= width_of(worst)
		end
		local function item_of(item)
			local tone = item.tone and theme.tone(t, item.tone) or t.ink_dim
			local cell = pencere.div():row():items_center():gap(5):h_full():px(8):no_shrink():color(tone):text_size(t.text.small)
			if item.icon ~= nil then
				cell:child(look.icon(item.icon, t.size.icon - 3, tone))
			end
			local words = item.words
			if type(words) == "string" or type(words) == "number" then
				words = pencere.text(words)
			end
			cell:child(words)
			if item.press then
				local part = u:part(id, "item", item.key)
				cell:press(part):role("button"):label(tostring(type(item.words) == "string" and item.words or item.key))
				if u:hovered(part) then
					cell:bg(t.hover)
				end
				cell:motion({ seconds = t.motion, color = true })
			end
			u:tip(cell, item.tip, item.keys)
			return cell
		end
		local function group_of(name)
			local row = pencere.div():row():items_center():h_full()
			for _, item in groups[name] do
				if not dropped[item] then
					row:child(item_of(item))
				end
			end
			return row
		end
		return pencere.div()
			:row()
			:items_center()
			:h(t.size.status)
			:w_full()
			:bg(t.surface)
			:px(4)
			:child(group_of("start"))
			:child(pencere.div():grow(1):min_w(0):row():justify_center():h_full():child(group_of("center")))
			:child(group_of("finish"))
	end,
	click = function(_, _, _, _, part, key)
		if part ~= "item" then
			return nil
		end
		return true, { kind = "press", key = key }
	end,
})
structure.status = Status

return structure
