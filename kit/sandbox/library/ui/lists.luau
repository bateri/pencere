-- @pencere/ui, the lists: rows the person goes through with the keys and the pointer, a palette that
-- finds one among many, the completions that stand by a caret.
--
--   u:list("results", { items = rows, state = L, row = function(item, index, look) … end })
--       -- select { key }, open { key, how }, menu { key, at }, mark { keys }, need { first, last }
--   u:palette("palette", { items = cmds:items(), state = P, label = "Command palette" })
--       -- pick { item }, query { text }, back, dismiss
--   u:completions("complete", { editor = "doc", items = found, state = C })        -- pick { item }, dismiss

local core = require("./core")
local theme = require("./theme")
local fuzzy = require("@pencere/fuzzy")
local layers = require("./layers")

local layout = pencere.layout
local alpha = pencere.alpha

local lists = {}

-- --- The arithmetic of a list ----------------------------------------------------------------------------

local function key_of(props, item)
	if props.key ~= nil then
		return props.key(item)
	end
	return item.key
end

-- The place of the item with `key`, nil where none has it.
local function index_of(props, count, key)
	if key == nil then
		return nil
	end
	local items = props.items
	for index = 1, count do
		local item = items[index]
		if item ~= nil and key_of(props, item) == key then
			return index
		end
	end
	return nil
end

local function count_of(props)
	return props.count or #props.items
end

-- The rows of a list as the page draws them: all of one height, with `inset` points around them.
local function rows_of(u, props)
	local height = props.height or u.theme.size.row
	local inset = props.inset or 0
	return pencere.Rows.even(count_of(props), height):padded(inset, inset), height
end

-- Scrolls the list `id` to show row `index`.
local function reveal(u, id, props, index)
	local rows = rows_of(u, props)
	local extent = layout.scroll_extent(id)
	if extent == nil then
		return
	end
	local offset = layout.scroll_offset(id)
	local to = rows:reveal(index, offset, extent.view)
	if to ~= offset then
		layout.scroll_to(id, { y = to })
	end
end

local MOVES = { up = true, down = true, home = true, ["end"] = true, page_up = true, page_down = true }

-- Moves the cursor of the list in `mount` as `name` says ("up", "down", "home", "end", "page_up",
-- "page_down"), scrolls it into view and says what came of it: the reply and the `select` action, or
-- false where it stayed.
local function move(u, mount, name)
	local props, state, id = mount.props, mount.state, mount.id
	local count = count_of(props)
	if count == 0 then
		return false
	end
	local at = index_of(props, count, state.cursor)
	local rows, height = rows_of(u, props)
	local extent = layout.scroll_extent(id)
	local page = math.max(math.floor((extent and extent.view or 0) / height) - 1, 1)
	local to
	if name == "down" then
		to = at and at + 1 or 1
	elseif name == "up" then
		to = at and at - 1 or count
	elseif name == "home" then
		to = 1
	elseif name == "end" then
		to = count
	elseif name == "page_down" then
		to = at and at + page or 1
	else
		to = at and at - page or count
	end
	if props.wrap and (name == "down" or name == "up") then
		to = (to - 1) % count + 1
	end
	to = math.min(math.max(to, 1), count)
	local item = props.items[to]
	if item == nil then
		return false
	end
	local key = key_of(props, item)
	reveal(u, id, props, to)
	if key == state.cursor then
		return false
	end
	state.cursor = key
	return true, { kind = "select", key = key, how = "key" }
end
lists.move = move

-- --- A list -----------------------------------------------------------------------------------------------------

-- A long list of rows, built as the person scrolls (`pencere.Rows`: every row of one `height`, the
-- theme's `row` by default, and `inset` points around them). `items` are the rows' data, each with a
-- stable `key` (or `props.key(item)` says it); `row(item, index, look)` makes what is in a row, where
-- `look` is `{ selected, hovered, marked, focused }` (the list draws the ground of the row, the
-- host-friendly role and the pointer's answers), and with no `row` a row says the item's `title` and
-- its `hint`. `state.cursor` is the key of the row chosen and `state.marks` a set of the keys marked
-- (with `multi`): both are the page's where it gives `state`.
--
-- The pointer chooses a row as it goes down on it (`select`), a second press opens it (`open`) and
-- the right button asks for its menu (`menu { key, at }`); the command or control key held marks or
-- unmarks it. A list that has the keyboard goes with the arrow keys, page up and down, home and end
-- (`props.keys` maps others: `{ j = "down", k = "up" }`), opens with enter (`open { how = "key" }`)
-- and marks with the space bar. `focusable = false` leaves the keyboard where it is (a palette's
-- field). `active` says it has the keyboard when it has not (the palette's field has it), and
-- `hover_selects` has the cursor follow the pointer. Rows the page has not read yet (`count` past
-- the rows of `items`, which hold a place with nil) say `need { first, last }` as they come into view.
local List = core.component("list", {
	state = function()
		return { cursor = nil, marks = {} }
	end,
	check = function(props)
		if type(props.items) ~= "table" then
			return "a list is made of items: list(id, { items = { { key =, title = } } })"
		end
		if props.row ~= nil and type(props.row) ~= "function" then
			return "a list's row is a function (item, index, look) that makes what is in the row"
		end
		return nil
	end,
	view = function(u, id, props, state)
		local t = u.theme
		local rows, height = rows_of(u, props)
		local has_keys = props.active or u:focus_within(id)
		state.marks = state.marks or {}
		local box = pencere.div():id(id):grow(1):min_h(0):scroll()
		if props.focusable ~= false then
			box:focusable()
		end
		return box:list(rows, function(index)
			local item = props.items[index]
			if item == nil then
				-- A row the page has not read yet.
				return pencere.div():h(height):px(8):items_center():child(
					pencere.div():h(8):w_pct(40):rounded(4):bg(alpha("foreground", 0.08))
				)
			end
			local key = key_of(props, item)
			local part = u:part(id, "row", key)
			local selected = key == state.cursor
			local over = u:hovered(part)
			local look = { selected = selected, hovered = over, marked = state.marks[key] == true, focused = has_keys, index = index }
			local content
			if props.row ~= nil then
				content = props.row(item, index, look)
			else
				content = pencere.div():row():items_center():gap(8):w_full():child(pencere.text(item.title or tostring(key)):truncate():grow(1):min_w(0))
				if item.hint ~= nil then
					content:child(pencere.text(item.hint):text_size(t.text.small):color(t.ink_quiet):no_shrink())
				end
			end
			local ground = nil
			if selected then
				ground = has_keys and t.chosen or t.chosen_blurred
			elseif look.marked then
				ground = t.on
			elseif over then
				ground = t.hover
			end
			local row = pencere.div()
				:press(part)
				:role(props.role or "menu_item")
				:selected(selected)
				:label(item.label or item.title or tostring(key))
				:h(height)
				:px(props.pad or 8)
				:row()
				:items_center()
				:bg(ground)
				:child(content)
			if item.expanded ~= nil then
				row:expanded(item.expanded)
			end
			if item.enabled == false then
				row:opacity(0.5)
			end
			return row
		end)
	end,
	press = function(u, id, props, state, part, key, event)
		if part ~= "row" then
			return nil
		end
		local mods = event.mods or {}
		if event.button == "right" then
			state.cursor = key
			return true, { kind = "menu", key = key, at = { x = event.x, y = event.y } }
		elseif event.button ~= "left" then
			return false
		end
		if (mods.command or mods.control) and props.multi then
			state.marks[key] = (not state.marks[key]) or nil
			local marked = {}
			for marked_key in state.marks do
				table.insert(marked, marked_key)
			end
			return true, { kind = "mark", keys = marked }
		end
		local changed = state.cursor ~= key
		state.cursor = key
		if event.count == 2 then
			return true, { kind = "open", key = key, how = "pointer" }
		end
		if changed then
			return true, { kind = "select", key = key, how = "pointer" }
		end
		return true
	end,
	hover = function(u, id, props, state, part, key)
		if part == "row" and props.hover_selects and state.cursor ~= key then
			state.cursor = key
			return true
		end
		return nil
	end,
	key = function(u, id, props, state, event, part)
		local mods = event.mods or {}
		if mods.command or mods.control or mods.alt then
			return nil
		end
		local name = (props.keys and props.keys[event.key]) or event.key
		if MOVES[name] then
			return move(u, u.mounts[id], name)
		end
		local count = count_of(props)
		local at = index_of(props, count, state.cursor)
		if name == "enter" and at then
			return true, { kind = "open", key = state.cursor, how = "key" }
		elseif event.key == " " and props.multi and at then
			state.marks[state.cursor] = (not state.marks[state.cursor]) or nil
			local marked = {}
			for marked_key in state.marks do
				table.insert(marked, marked_key)
			end
			return true, { kind = "mark", keys = marked }
		end
		return nil
	end,
	scrolled = function(u, id, props, state, _, _, box)
		if props.count == nil then
			return nil
		end
		local first, last = pencere.list_window(id)
		if first == nil then
			return nil
		end
		local from, to = nil, nil
		for index = first, last do
			if props.items[index] == nil then
				from = from or index
				to = index
			end
		end
		if from == nil or (state.asked_from == from and state.asked_to == to) then
			return nil
		end
		state.asked_from, state.asked_to = from, to
		return true, { kind = "need", first = from, last = to }
	end,
	settle = function(u, id, props, state)
		if state.cursor == nil or state.revealed == state.cursor then
			return false
		end
		local at = index_of(props, count_of(props), state.cursor)
		if at == nil then
			return false
		end
		if layout.scroll_extent(id) == nil then
			return false
		end
		state.revealed = state.cursor
		reveal(u, id, props, at)
		return false
	end,
})
lists.list = List

-- --- A palette -------------------------------------------------------------------------------------------------------

-- Past this many items a search is made a step at a time, as the person types, and not at once.
local ALL_AT_ONCE = 3000

-- The items of the palette that its query is in, best first: `{ key, source = item, positions }`.
-- Kept for as long as the query and the items stay as they are.
local function found(u, id, props, state)
	local query = state.query or ""
	local identity = props.version or props.items
	local kept = state.found
	if kept ~= nil and kept.query == query and kept.identity == identity then
		return kept.rows
	end
	local rows = {}
	local items = props.items
	if type(props.filter) == "function" then
		for _, item in props.filter(query, items) do
			table.insert(rows, { key = item.key, label = item.title, source = item })
		end
	elseif query == "" then
		for index = 1, math.min(#items, props.limit or 200) do
			local item = items[index]
			table.insert(rows, { key = item.key, label = item.title, source = item })
		end
	elseif #items <= ALL_AT_ONCE then
		for _, match in fuzzy.filter(query, items, { text = "title", limit = props.limit or 200 }) do
			table.insert(rows, { key = match.item.key, label = match.item.title, source = match.item, positions = match.positions })
		end
	else
		if kept ~= nil and kept.search ~= nil then
			kept.search:cancel()
		end
		local search = fuzzy.search(query, items, { text = "title", limit = props.limit or 200 })
		local entry = { query = query, identity = identity, rows = rows, search = search }
		state.found = entry
		local function collect()
			table.clear(rows)
			for _, match in search:results() do
				table.insert(rows, { key = match.item.key, label = match.item.title, source = match.item, positions = match.positions })
			end
			return true
		end
		pencere.time.after(0, function()
			return search:run(collect)
		end)
		entry.collect = collect
		return rows
	end
	state.found = { query = query, identity = identity, rows = rows }
	return rows
end

-- A palette: a field and under it the items that its query is in (fuzzily: `@pencere/fuzzy`; with
-- `filter`, a function (query, items) → items, the page's own way: its modes, its prefixes), the
-- best first, the first one chosen. `items` are `{ key, title, hint, keys, enabled, icon }` (commands,
-- files …): pass the same table while they are the same (or `version`), or the search is made again
-- at every frame. Over a few thousand items the search goes on a step to a frame while the person
-- types. `state` is `{ query, cursor }`: the page keeps it (a palette opened again has its query
-- selected, so that typing replaces it) and puts the query in it to make one open on a word. The
-- field has the keyboard: the arrow keys and page up and down move the cursor, enter picks
-- (`pick { item }`), a click on a row picks it, escape and a click outside it give up (`dismiss`),
-- and backspace in the empty field is `back` (the page leaves a mode). Each change of the query is
-- `query { text }`. `footer` is the keys it says at its foot: `{ { keys = "enter", title = "Run" } }`.
local Palette = core.component("palette", {
	state = function()
		return { query = "", cursor = nil, field = { value = "" } }
	end,
	check = function(props)
		if type(props.items) ~= "table" then
			return "a palette finds one of items: palette(id, { items = { { key =, title = } } })"
		end
		if type(props.label) ~= "string" then
			return "a palette says what it is in words: palette(id, { label = \"Command palette\" })"
		end
		return nil
	end,
	view = function(u, id, props, state)
		local t = u.theme
		state.field = state.field or { value = state.query or "" }
		state.field.value = state.query or ""
		local rows = found(u, id, props, state)
		-- The cursor is on a row that is there: the first, where it is on none.
		local at = nil
		for index, row in rows do
			if row.key == state.cursor then
				at = index
			end
		end
		if at == nil then
			state.cursor = rows[1] and rows[1].key or nil
		end
		local select_first = not state.opened
		state.opened = true
		local body = pencere.div():gap(0)
		body:child(
			pencere.div():p(8):child(u:input(u:part(id, "input"), {
				state = state.field,
				hint = props.hint,
				label = props.label,
				select = select_first,
			}))
		)
		body:child(u:rule("x"))
		local list_height = math.min(math.max(#rows, 1), props.rows or 8) * (props.height or 32) + 8
		if #rows == 0 then
			body:child(
				pencere.div():h(props.height or 40):items_center():justify_center():child(
					pencere.text(props.empty or "No results"):color(t.ink_quiet)
				)
			)
		else
			body:child(u:list(u:part(id, "list"), {
				items = rows,
				state = state,
				active = true,
				focusable = false,
				hover_selects = true,
				height = props.height or 32,
				inset = 4,
				pad = 10,
				label = props.label,
				row = function(item, _, look)
					local source = item.source
					local title = pencere.text(source.title):truncate():grow(1):min_w(0)
					if item.positions ~= nil then
						title = pencere.rich(fuzzy.spans(source.title, item.positions, { color = "accent", weight = 700 }))
							:truncate()
							:grow(1)
							:min_w(0)
					end
					local row = pencere.div():row():items_center():gap(10):w_full():child(title)
					if source.hint ~= nil then
						row:child(pencere.text(source.hint):text_size(t.text.small):color(t.ink_quiet):no_shrink())
					end
					if source.keys ~= nil then
						local first = type(source.keys) == "table" and source.keys[1] or source.keys
						row:child(u:kbd(first, { quiet = true }))
					end
					if source.enabled == false then
						row:opacity(0.5)
					end
					return row
				end,
			}):h(list_height):grow(0))
		end
		if props.footer ~= nil then
			local foot = pencere.div():row():items_center():gap(14):px(12):py(6):bg(alpha("foreground", 0.03))
			for _, hint in props.footer do
				foot:child(
					pencere.div():row():items_center():gap(6)
						:child(u:kbd(hint.keys))
						:child(pencere.text(hint.title):text_size(t.text.small):color(t.ink_dim))
				)
			end
			body:child(u:rule("x")):child(foot)
		end
		return layers.shell(u, id, {
			width = props.width or 560,
			label = props.label,
			initial = u:name(u:part(id, "input"), "field"),
		}, body)
	end,
	click = function(u, id, _, _, part, _, event)
		return layers.shell_click(u, id, part, {}, { kind = "dismiss" }, event)
	end,
	dismiss = function()
		return true, { kind = "dismiss" }
	end,
	key = function(u, id, props, state, event)
		local mods = event.mods or {}
		local list = u.mounts[u:name(id, "list")]
		if event.key == "enter" and not mods.command and not mods.control and not mods.alt then
			for _, row in state.found and state.found.rows or {} do
				if row.key == state.cursor then
					if row.source.enabled == false then
						return false
					end
					return true, { kind = "pick", item = row.source }
				end
			end
			return false
		elseif MOVES[event.key] and not (mods.command or mods.control or mods.alt) and event.key ~= "home" and event.key ~= "end" then
			if list == nil then
				return false
			end
			local reply = move(u, list, event.key)
			return reply or false
		elseif event.key == "backspace" and (state.query or "") == "" then
			return true, { kind = "back" }
		end
		return nil
	end,
	action = function(u, id, props, state, act)
		local part = u:part_of(act.from)
		if part == nil or part.mount ~= id then
			return nil
		end
		if part.part == "input" then
			if act.kind == "change" then
				state.query = act.text
				return true, { kind = "query", text = act.text }
			elseif act.kind == "submit" then
				-- Enter in the field picks what the cursor is on.
				for _, row in state.found and state.found.rows or {} do
					if row.key == state.cursor then
						if row.source.enabled == false then
							return false
						end
						return true, { kind = "pick", item = row.source }
					end
				end
				return false
			end
		elseif part.part == "list" then
			if act.kind == "select" and act.how == "pointer" or act.kind == "open" then
				for _, row in state.found and state.found.rows or {} do
					if row.key == act.key and row.source.enabled ~= false then
						return true, { kind = "pick", item = row.source }
					end
				end
				return true
			end
			return true
		end
		return nil
	end,
})
lists.palette = Palette

-- --- Completions ----------------------------------------------------------------------------------------------------------

-- The completions of an editor: `items` (`{ key, title, hint, detail, icon }`) in a card under the caret of
-- the editor `editor` (above it where there is no room below), drawn while the page gives items and
-- stops when it gives none. The editor keeps the keyboard; its keys are asked for as commands while the
-- card is there: up and down move the cursor, enter and tab `pick { item }` it, escape `dismiss`es.
-- The page says what to complete with (it reads the editor's text at the caret) and what picking does.
-- `state` is `{ cursor }`.
local Completions = core.component("completions", {
	state = function()
		return { cursor = nil, marks = {} }
	end,
	check = function(props)
		if type(props.editor) ~= "string" then
			return "completions are for an editor: completions(id, { editor = <its name>, items = })"
		end
		if type(props.items) ~= "table" then
			return "completions are items: completions(id, { editor =, items = { { key =, title = } } })"
		end
		return nil
	end,
	view = function(u, id, props, state)
		local t = u.theme
		if #props.items == 0 then
			return pencere.div():hidden()
		end
		u:layer(id, { modal = false })
		local at = nil
		for index, item in props.items do
			if item.key == state.cursor then
				at = index
			end
		end
		if at == nil then
			state.cursor = props.items[1].key
		end
		local height = props.height or t.size.row
		local shown = math.min(#props.items, props.rows or 8)
		u:root_command(u:part(id, "next"), "Next completion", "down")
		u:root_command(u:part(id, "previous"), "Previous completion", "up")
		u:root_command(u:part(id, "pick"), "Accept completion", "enter")
		u:root_command(u:part(id, "tab"), "Accept completion", "tab")
		u:root_command(u:part(id, "dismiss"), "Close completions", "escape")
		local card = pencere.div()
			:anchor(pencere.caret_of(props.editor), "below", { gap = 4 })
			:id(id)
			:w(props.width or 320)
			:bg(t.popup)
			:rounded(t.radius.card)
			:border(1, t.line)
			:shadow(0, 8, 24, alpha("shadow", 0.4))
			:child(u:list(u:part(id, "list"), {
				items = props.items,
				state = state,
				active = true,
				focusable = false,
				hover_selects = true,
				height = height,
				inset = 3,
				row = props.row,
			}):h(shown * height + 6):grow(0))
		return card
	end,
	command = function(u, id, props, state, part)
		local list = u.mounts[u:name(id, "list")]
		if part == "dismiss" then
			return true, { kind = "dismiss" }
		elseif part == "next" or part == "previous" then
			if list == nil then
				return false
			end
			local reply = move(u, list, part == "next" and "down" or "up")
			return reply or false
		elseif part == "pick" or part == "tab" then
			for _, item in props.items do
				if item.key == state.cursor then
					return true, { kind = "pick", item = item }
				end
			end
			return false
		end
		return nil
	end,
	action = function(u, id, props, state, act)
		local part = u:part_of(act.from)
		if part == nil or part.mount ~= id or part.part ~= "list" then
			return nil
		end
		if act.kind == "select" and act.how == "pointer" or act.kind == "open" then
			for _, item in props.items do
				if item.key == act.key then
					return true, { kind = "pick", item = item }
				end
			end
		end
		return true
	end,
	away = function(_, _, props)
		if #props.items == 0 then
			return nil
		end
		return true, { kind = "dismiss" }
	end,
})
lists.completions = Completions

return lists
