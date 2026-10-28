-- @pencere/ui, the grid: rows and columns of values, a header that stays, columns that are sized.
--
--   u:grid("result", { columns = { { key = "id", title = "id", type = "int", align = "end" }, … },
--       count = 120000, cell = function(row, column) return rows[row] and rows[row][column.key] end,
--       state = G })
--       -- select { row, column }, open { row, column }, sort { column }, resize { column, width },
--       -- resized { column, width }, need { first, last }, menu { row, column, at }

local core = require("./core")
local look = require("./look")

local layout = pencere.layout
local alpha = pencere.alpha

local grid = {}

-- How wide a column is where nothing says: a guess of its title's length (the page cannot measure
-- text), at least this much and at most that.
local MIN_WIDTH, MAX_WIDTH, EDGE = 56, 480, 7
local CHAR = 7.4

local function widths_of(props, state)
	local widths = {}
	local total = 0
	for index, column in props.columns do
		local width = state.widths[column.key] or column.width
		if width == nil then
			width = math.min(math.max(utf8.len(column.title or tostring(column.key)) * CHAR + 40, 96), MAX_WIDTH)
		end
		widths[index] = math.max(width, MIN_WIDTH)
		total += widths[index]
	end
	return widths, total
end

local function number_width(props, count)
	if props.numbers == false then
		return 0
	end
	return math.max(#tostring(count) * CHAR + 22, 44)
end

local function column_at(props, widths, number, x)
	local left = number
	for index, width in widths do
		if x < left + width then
			return x >= number and index or 1
		end
		left += width
	end
	return #props.columns
end

-- The columns to build with the grid scrolled `x` across: those in view and two views of them each way,
-- for the host scrolls across before the page is made again; and where the first starts.
local function columns_near(widths, number, x, view)
	local from, to = x - view * 2, x + view * 3
	local left = number
	local first, last, start = nil, 0, number
	for index, width in widths do
		if left + width > from and left < to then
			if first == nil then
				first, start = index, left
			end
			last = index
		end
		left += width
	end
	return first or 1, last, start
end

local function rows_of(props, state, height)
	return pencere.Rows.even(props.count, height)
end

-- Scrolls the least that shows the chosen cell: down in the rows, across in the outer box.
local function reveal(u, id, props, state, cell)
	local t = u.theme
	local height = props.row_height or t.size.row
	local body = u:name(id, "body")
	local rows = rows_of(props, state, height)
	local extent = layout.scroll_extent(body)
	if extent ~= nil then
		local y = layout.scroll_offset(body)
		local to = rows:reveal(cell.row, y, extent.view)
		if to ~= y then
			layout.scroll_to(body, { y = to })
		end
	end
	local widths = widths_of(props, state)
	local number = number_width(props, props.count)
	local left = number
	for index = 1, cell.column - 1 do
		left += widths[index]
	end
	local right = left + widths[cell.column]
	local across = layout.scroll_extent(id)
	if across ~= nil then
		local _, x = layout.scroll_offset(id)
		x = x or 0
		if left - number < x then
			layout.scroll_to(id, { x = math.max(left - number, 0) })
		elseif right > x + across.view_w then
			layout.scroll_to(id, { x = right - across.view_w })
		end
	end
end

-- The cell the cursor moves to from `name`, or nil where it stays.
local function moved(props, state, name, page)
	local cell = state.cursor or { row = 1, column = 1 }
	local row, column = cell.row, cell.column
	if state.cursor == nil then
		return { row = 1, column = 1 }
	end
	if name == "up" then
		row -= 1
	elseif name == "down" then
		row += 1
	elseif name == "left" then
		column -= 1
	elseif name == "right" then
		column += 1
	elseif name == "page_up" then
		row -= page
	elseif name == "page_down" then
		row += page
	elseif name == "home" then
		column = 1
	elseif name == "end" then
		column = #props.columns
	else
		return nil
	end
	row = math.min(math.max(row, 1), math.max(props.count, 1))
	column = math.min(math.max(column, 1), #props.columns)
	if row == cell.row and column == cell.column then
		return nil
	end
	return { row = row, column = column }
end

-- A grid of what a query or a table gives: `columns` are `{ key, title, type, align, width }` (`align`
-- "end" is for numbers), `count` how many rows there are, `cell(row, column)` the value of one cell as
-- words, an element, or nil where its row has not been read yet. The rows are a long list's: only
-- those near what is in view are made, and the columns the same across. The header does not scroll
-- down, and goes across with the rows; the person drags the edge of a column's name to size it
-- (`resize { column, width }` as it goes, `resized` when it is let go: `state.widths` keeps what the
-- page has not said), and presses its name to sort (`sort { column }`: the page orders its rows and
-- says it in `state.sort = { column = key, descending = }`, which the header draws). A press on a cell
-- chooses it (`select { row, column }`), a second opens it (`open`), the right button asks for a menu
-- (`menu { row, column, at }`); with the keyboard the arrow keys, page up and down, home and end (a row's
-- first and last column) go from cell to cell and enter opens. Rows it has no cell for are asked as
-- `need { first, last }` when they come into view. The width of a column is a guess of its title's
-- length until a page can measure text; give `width` or let the person size it.
local Grid = core.component("grid", {
	state = function()
		return { cursor = nil, widths = {}, sort = nil }
	end,
	check = function(props)
		if type(props.columns) ~= "table" or #props.columns == 0 then
			return "a grid has columns: grid(id, { columns = { { key =, title = } }, count =, cell = function(row, column) … end })"
		end
		if type(props.count) ~= "number" then
			return "a grid says how many rows it has: grid(id, { count = })"
		end
		if type(props.cell) ~= "function" then
			return "a grid asks for each cell: cell = function(row, column) → words, an element or nil"
		end
		return nil
	end,
	view = function(u, id, props, state)
		local t = u.theme
		state.widths = state.widths or {}
		local height = props.row_height or t.size.row
		local head = props.head_height or (t.size.row + 4)
		local widths, content = widths_of(props, state)
		local number = number_width(props, props.count)
		local body_name = u:part(id, "body")
		local _, x = layout.scroll_offset(id)
		x = x or 0
		local view_w = state.view_w or u.size.w
		local first_column, last_column, start = columns_near(widths, number, x, view_w)
		state.built_x = x
		local room = math.max(number + content, view_w)
		local rows = pencere.Rows.even(props.count, height)
		local body_h = math.max((state.view_h or u.size.h) - head - 1, height * 2)
		local keyboard = u:focus_within(body_name)
		local chosen = state.cursor

		local header = pencere.div():row():h(head):w(room):no_shrink():bg(t.surface)
		header:child(
			pencere.div():w(number):h_full():no_shrink():items_center():justify_end():pr(10):child(
				pencere.text(props.numbers == false and "" or "#"):text_size(t.text.tiny):color(t.ink_quiet)
			)
		)
		for index = first_column, last_column do
			local column = props.columns[index]
			local part = u:part(id, "head", column.key)
			local sorted = state.sort ~= nil and state.sort.column == column.key
			local over = u:hovered(part)
			local cell = pencere.div()
				:press(part)
				:role("button")
				:label((column.title or tostring(column.key)) .. (sorted and (state.sort.descending and ", sorted descending" or ", sorted ascending") or ""))
				:row()
				:items_center()
				:gap(4)
				:w(widths[index])
				:h_full()
				:px(8)
				:no_shrink()
				:bg(over and t.hover or nil)
				:motion({ seconds = t.motion, color = true })
			if column.align == "end" then
				cell:justify_end()
			end
			local title = pencere.div():col():min_w(0):child(
				pencere.text(column.title or tostring(column.key)):weight(600):truncate():color(over and t.ink or t.ink_dim)
			)
			if column.type ~= nil and column.type ~= "" then
				title:child(pencere.text(column.type):text_size(t.text.tiny):color(t.ink_quiet):truncate())
			end
			cell:child(title)
			if sorted then
				cell:child(look.glyph(state.sort.descending and "chevron_down" or "chevron_up", 12, "accent"))
			end
			-- The edge that sizes: a handle the width of a finger on the column's right, with a line in it
			-- that shows under the pointer.
			local edge = u:part(id, "edge", column.key)
			local held = u.capture ~= nil and u.capture.mount == id and u.capture.key == column.key
			local active = u:hovered(edge) or held
			cell:child(
				pencere.div():press(edge):absolute():inset(0, 0, 0, nil):w(EDGE):child(
					pencere.div():absolute():inset(active and 0 or 8, 0, active and 0 or 8, nil):w(active and 2 or 1):bg(
						active and t.ring or alpha(t.line, 0.7)
					)
				)
			)
			header:child(cell)
		end

		local body = pencere.div():id(body_name):col():w(room):h(body_h):no_shrink():scroll():focusable()
		body:list(rows, function(place)
			local row_part = u:part(id, "row", place)
			local line = pencere.div()
				:press(row_part)
				:role("menu_item")
				:selected(chosen ~= nil and chosen.row == place)
				:label("Row " .. place)
				:row()
				:items_center()
				:h(height)
				:w(room)
				:no_shrink()
			if u:hovered(row_part) then
				line:bg(t.hover)
			elseif place % 2 == 0 then
				line:bg(alpha("foreground", 0.025))
			end
			if number > 0 then
				line:child(
					pencere.div():w(number):h_full():no_shrink():items_center():justify_end():pr(10):child(
						pencere.text(tostring(place)):text_size(t.text.small):color(t.ink_quiet)
					)
				)
			end
			if start > number then
				line:child(pencere.div():w(start - number):h_full():no_shrink())
			end
			for index = first_column, last_column do
				local column = props.columns[index]
				local width = widths[index]
				local value = props.cell(place, column)
				local content
				if value == nil then
					content = pencere.text("·"):color(t.ink_quiet)
				elseif type(value) == "string" or type(value) == "number" then
					content = pencere.text(value):truncate()
				else
					content = value
				end
				local cell = pencere.div()
					:w(width)
					:h_full()
					:no_shrink()
					:px(8)
					:row()
					:items_center()
					:clip()
					:child(pencere.div():grow(1):min_w(0):row():items_center():child(content))
				if column.align == "end" then
					cell:justify_end()
				end
				if chosen ~= nil and chosen.row == place and chosen.column == index then
					cell:bg(alpha("accent", keyboard and 0.22 or 0.12)):border(1, alpha("accent", keyboard and 0.9 or 0.45))
				end
				line:child(cell)
			end
			return line
		end)

		return pencere.div()
			:id(id)
			:col()
			:grow(1)
			:min_h(0)
			:min_w(0)
			:scroll()
			:child(header)
			:child(u:rule("x"):w(room))
			:child(body)
	end,
	press = function(u, id, props, state, part, key, event)
		if part == "edge" then
			local widths = widths_of(props, state)
			for index, column in props.columns do
				if column.key == key then
					state.origin = { key = key, width = widths[index] }
				end
			end
			return false, nil, true
		elseif part == "head" then
			return nil
		elseif part ~= "row" then
			return nil
		end
		local box = layout.bounds(u:name(id, "row", key))
		local widths = widths_of(props, state)
		local number = number_width(props, props.count)
		local column = box ~= nil and column_at(props, widths, number, event.x - box.x) or 1
		if event.button == "right" then
			state.cursor = { row = key, column = column }
			return true, { kind = "menu", row = key, column = column, at = { x = event.x, y = event.y } }
		elseif event.button ~= "left" then
			return false
		end
		local was = state.cursor
		state.cursor = { row = key, column = column }
		if event.count == 2 then
			return true, { kind = "open", row = key, column = column }
		end
		if was == nil or was.row ~= key or was.column ~= column then
			return true, { kind = "select", row = key, column = column }
		end
		return true
	end,
	click = function(_, _, props, _, part, key)
		if part ~= "head" then
			return nil
		end
		return true, { kind = "sort", column = key }
	end,
	drag = function(_, _, props, state, part, key, at)
		if part ~= "edge" or state.origin == nil then
			return nil
		end
		local width = math.min(math.max(state.origin.width + at.dx, MIN_WIDTH), MAX_WIDTH * 2)
		if state.widths[key] == width then
			return false
		end
		state.widths[key] = width
		return true, { kind = "resize", column = key, width = width }
	end,
	release = function(_, _, _, state, part, key)
		if part ~= "edge" or state.origin == nil then
			return nil
		end
		state.origin = nil
		return true, { kind = "resized", column = key, width = state.widths[key] }
	end,
	cursor = function(_, _, _, _, part)
		if part == "edge" then
			return "resize_horizontal"
		end
		return nil
	end,
	key = function(u, id, props, state, event, part)
		local mods = event.mods or {}
		if part ~= "body" or mods.command or mods.control or mods.alt then
			return nil
		end
		local body = u:name(id, "body")
		local extent = layout.scroll_extent(body)
		local height = props.row_height or u.theme.size.row
		local page = math.max(math.floor((extent and extent.view or 0) / height) - 1, 1)
		if event.key == "enter" and state.cursor ~= nil then
			return true, { kind = "open", row = state.cursor.row, column = state.cursor.column }
		end
		local cell = moved(props, state, event.key, page)
		if cell == nil then
			if state.cursor == nil and props.count > 0 and (event.key == "down" or event.key == "right") then
				cell = { row = 1, column = 1 }
			else
				return nil
			end
		end
		state.cursor = cell
		reveal(u, id, props, state, cell)
		return true, { kind = "select", row = cell.row, column = cell.column }
	end,
	scrolled = function(u, id, props, state, part, _, box)
		if part == nil then
			-- Across: the columns are made again where it has gone past what was made.
			if state.built_x == nil or math.abs(box.x - state.built_x) > (state.view_w or u.size.w) then
				return true
			end
			return nil
		end
		if part ~= "body" then
			return nil
		end
		local first, last = pencere.list_window(u:name(id, "body"))
		if first == nil then
			return nil
		end
		local from, to = nil, nil
		for row = first, last do
			if props.cell(row, props.columns[1]) == nil then
				from = from or row
				to = row
			end
		end
		if from == nil or (state.asked_from == from and state.asked_to == to) then
			return nil
		end
		state.asked_from, state.asked_to = from, to
		return true, { kind = "need", first = from, last = to }
	end,
	settle = function(u, id, props, state)
		local extent = layout.scroll_extent(id)
		if extent == nil then
			return false
		end
		local again = false
		if extent.view_w ~= nil and extent.view_w ~= state.view_w then
			state.view_w = extent.view_w
			again = true
		end
		if extent.view ~= nil and extent.view ~= state.view_h then
			state.view_h = extent.view
			again = true
		end
		if state.cursor ~= nil and state.revealed ~= state.cursor then
			state.revealed = state.cursor
			reveal(u, id, props, state, state.cursor)
		end
		return again
	end,
})
grid.grid = Grid

return grid
