-- @pencere/ui, the controls: a field of text, a choice of one among a few, the tabs of a page.
--
--   u:input("query", { state = Q, hint = "Search", label = "Search in files" })   -- change { text }, submit { text, mods }
--   u:segments("view", { items = { { key = "list", title = "List" }, ... }, value = S.view })   -- select { key }
--   u:tabs("tabs", { items = tabs, active = S.active, reorder = true })
--       -- select { key }, close { key }, move { key, before }, menu { key, at }

local core = require("./core")
local theme = require("./theme")
local look = require("./look")

local layout = pencere.layout
local alpha = pencere.alpha

local controls = {}

-- Whether a key event is enter or the space bar with no modifier.
local function presses(event)
	local mods = event.mods or {}
	if mods.command or mods.control or mods.alt or mods.shift then
		return false
	end
	return event.key == "enter" or event.key == " "
end

-- --- A field of text ---------------------------------------------------------------------------------

local KINDS = { line = true, secret = true, lines = true }

-- A field of text, the host's own (it is edited as the system's fields are) in a box of the theme.
-- `state.value` is what it holds: the page keeps the table and the field is made to hold what it says
-- whenever it says another thing (`u:field_text`); what the person types is put in it. `label` says
-- what it is to a screen reader (it is needed), `hint` what the field says while it is empty, `kind`
-- is "line" (the default), "secret" or "lines", `tone` ("bad" …) colours its edge, `lead` and `trail`
-- are elements before and after the text (an icon, the options of a search). With `select`, what is
-- put in the field is selected in it, as a palette opened again has its query. `escape` makes
-- escape in it an action of its own (`{ kind = "escape" }`), where it would close the layer it is in.
-- The field's own name is `u:name(id, "field")`; `u:focus(id)` puts the keyboard in it.
controls.input = core.component("input", {
	state = function()
		return { value = "" }
	end,
	check = function(props)
		if type(props.label) ~= "string" then
			return "a field says what it is in words: input(id, { label = })"
		end
		if props.kind ~= nil and not KINDS[props.kind] then
			return 'an input\'s kind is "line", "secret" or "lines", not ' .. tostring(props.kind)
		end
		return nil
	end,
	view = function(u, id, props, state)
		local t = u.theme
		local name = u:field(u:part(id, "field"))
		if type(state.value) ~= "string" then
			state.value = tostring(state.value or "")
		end
		u:field_text(name, state.value, props.select)
		local ringed = u:keyboard() == name
		local edge = ringed and t.ring or props.tone and theme.tone(t, props.tone) or t.line
		local field = pencere.div():text_field(name, props.kind or "line"):focusable():label(props.label):grow(1):min_w(0):min_h(t.size.control - 4)
		if props.hint ~= nil then
			field:child(pencere.text(props.hint):color(t.ink_quiet))
		end
		local box = pencere.div()
			:row()
			:items_center()
			:gap(6)
			:min_h(t.size.control)
			:px(8)
			:rounded(t.radius.control)
			:bg(t.field)
			:border(1, edge)
			:motion({ seconds = t.motion, color = true })
		if props.lead ~= nil then
			box:child(props.lead)
		end
		box:child(field)
		for _, element in props.trail or {} do
			box:child(element)
		end
		return box
	end,
	text = function(_, _, _, state, _, _, event)
		state.value = event.text
		return true, { kind = "change", text = event.text }
	end,
	key = function(_, _, props, state, event, part)
		if part ~= "field" then
			return nil
		end
		if event.key == "enter" and (props.kind or "line") ~= "lines" then
			return true, { kind = "submit", text = state.value, mods = event.mods }
		elseif event.key == "escape" and props.escape then
			return true, { kind = "escape" }
		end
		return nil
	end,
})

-- --- A choice of one among a few ----------------------------------------------------------------------

-- A few words side by side, one of them chosen (`value`, a key of `items`): `{ kind = "select", key }`
-- by the pointer, or by the arrow keys, home and end while the chosen one has the keyboard (only the
-- chosen one is in the order of the tab key, as a group of radio buttons is). A screen reader hears
-- radio buttons.
controls.segments = core.component("segments", {
	check = function(props)
		if type(props.items) ~= "table" then
			return "segments are a choice among items: segments(id, { items = { { key =, title = } }, value = })"
		end
		return nil
	end,
	view = function(u, id, props)
		local t = u.theme
		local row = pencere.div()
			:row()
			:items_center()
			:gap(2)
			:p(2)
			:no_shrink()
			:rounded(t.radius.control + 2)
			:bg(t.control)
		local chosen = props.value
		if chosen == nil and props.items[1] then
			chosen = props.items[1].key
		end
		for _, item in props.items do
			local part = u:part(id, "item", item.key)
			local on = item.key == props.value
			local over = u:hovered(part)
			local segment = pencere.div()
				:press(part)
				:role("radio")
				:checked(on)
				:label(item.title or tostring(item.key))
				:row()
				:items_center()
				:justify_center()
				:gap(4)
				:h(t.size.control - 6)
				:px(10)
				:rounded(t.radius.control)
				:bg(on and t.chip or over and t.hover or nil)
				:color(on and t.ink or t.ink_dim)
				:border(1, u:focus_visible(part) and t.ring or t.clear)
				:motion({ seconds = t.motion, color = true })
			if item.key == chosen then
				segment:focusable()
			end
			if item.icon ~= nil then
				segment:child(look.icon(item.icon, t.size.icon - 2, on and t.ink or t.ink_dim))
			end
			if item.title ~= nil then
				segment:child(pencere.text(item.title):weight(on and 600 or 500))
			end
			u:tip(segment, item.tip)
			row:child(segment)
		end
		return row
	end,
	click = function(u, id, props, _, part, key)
		if part ~= "item" then
			return nil
		end
		-- The keyboard goes where the pointer chose, so that the arrow keys go on from there.
		local reply = u:focus(u:name(id, "item", key))
		if key == props.value then
			return reply
		end
		return reply, { kind = "select", key = key }
	end,
	key = function(u, id, props, _, event, part, key)
		if part ~= "item" then
			return nil
		end
		local mods = event.mods or {}
		if mods.command or mods.control or mods.alt then
			return nil
		end
		local at = nil
		for index, item in props.items do
			if item.key == key then
				at = index
			end
		end
		if at == nil then
			return nil
		end
		local to = at
		if event.key == "right" or event.key == "down" then
			to = math.min(at + 1, #props.items)
		elseif event.key == "left" or event.key == "up" then
			to = math.max(at - 1, 1)
		elseif event.key == "home" then
			to = 1
		elseif event.key == "end" then
			to = #props.items
		elseif presses(event) then
			return true, { kind = "select", key = key }
		else
			return nil
		end
		if to == at then
			return false
		end
		local item = props.items[to]
		return u:focus(u:name(id, "item", item.key)), { kind = "select", key = item.key }
	end,
})

-- --- The tabs of a page -----------------------------------------------------------------------------------

local TAB_WIDTH = 200

-- Tabs in a strip that scrolls across: `items` are `{ key, title, hint, icon, dirty, closable, tone,
-- tip }`, `active` the key of the one in front. The pointer chooses one as it goes down on it
-- (`select`), closes one with its cross or the middle button (`close`), asks for its menu with the
-- right button (`menu { key, at }`); with `reorder`, a tab dragged onto another is put in front of
-- it (`move { key, before }`: `before` is the key of the tab it goes in front of, nil for the end).
-- A tab with unsaved changes (`dirty`) shows a dot where its cross is until the pointer is over it.
-- The tab in front is scrolled into view once the page has been laid out. The arrow keys, home and
-- end on a tab move to another and choose it.
controls.tabs = core.component("tabs", {
	state = function()
		return {}
	end,
	check = function(props)
		if type(props.items) ~= "table" then
			return "tabs are items: tabs(id, { items = { { key =, title = } }, active = })"
		end
		return nil
	end,
	view = function(u, id, props, state)
		local t = u.theme
		local strip = pencere.div():id(u:part(id, "strip")):row():scroll():grow(1):min_w(0):h(t.size.bar)
		local dragging = props.reorder and u.pointer:dragging() or nil
		local active = props.active
		for _, item in props.items do
			local part = u:part(id, "tab", item.key)
			local on = item.key == active
			local over = u:within(part)
			local tone = item.tone and theme.tone(t, item.tone) or nil
			local ink = on and (tone or t.ink) or (tone or t.ink_dim)
			local tab = pencere.div()
				:press(part)
				:role("tab")
				:selected(on)
				:label(item.title)
				:row()
				:items_center()
				:gap(6)
				:h_full()
				:px(10)
				:no_shrink()
				:bg(on and t.chip or over and t.hover or nil)
				:color(ink)
				:border(1, u:focus_visible(part) and t.ring or t.clear)
				:motion({ seconds = t.motion, color = true })
			if on then
				tab:focusable()
			end
			if props.reorder then
				tab:draggable(part):drop_target(part)
			end
			if dragging and dragging.target == part then
				tab:border(1, t.ring)
			end
			if item.icon ~= nil then
				tab:child(look.icon(item.icon, t.size.icon - 1, ink))
			end
			tab:child(pencere.text(item.title):truncate():max_w(TAB_WIDTH):weight(on and 600 or 400))
			if item.hint ~= nil then
				tab:child(pencere.text(item.hint):text_size(t.text.small):color(t.ink_quiet):truncate():max_w(TAB_WIDTH))
			end
			if item.closable ~= false and props.closable ~= false then
				local close = pencere.div():size(16, 16):no_shrink():items_center():justify_center()
				if item.dirty and not over then
					close:child(look.glyph("dot", 12, t.ink_dim))
				elseif over or on then
					close:child(u:icon_button(u:part(id, "close", item.key), {
						icon = "close",
						label = "Close " .. item.title,
						size = 16,
					}))
				end
				tab:child(close)
			end
			u:tip(tab, item.tip)
			strip:child(tab)
		end
		return pencere.div():row():items_center():h(t.size.bar):bg(t.surface):child(strip):child(u:rule("y"))
	end,
	press = function(u, id, props, _, part, key, event)
		if part ~= "tab" then
			return nil
		end
		if event.button == "middle" then
			return true, { kind = "close", key = key }
		elseif event.button == "right" then
			return true, { kind = "menu", key = key, at = { x = event.x, y = event.y } }
		elseif event.button == "left" then
			local reply = u:focus(u:name(id, "tab", key))
			if key == props.active then
				return reply
			end
			return reply, { kind = "select", key = key }
		end
		return false
	end,
	action = function(u, id, _, _, act)
		local part = u:part_of(act.from)
		if part == nil or part.mount ~= id or part.part ~= "close" or act.kind ~= "press" then
			return nil
		end
		return true, { kind = "close", key = part.key }
	end,
	key = function(u, id, props, _, event, part, key)
		if part ~= "tab" then
			return nil
		end
		local mods = event.mods or {}
		if mods.command or mods.control or mods.alt then
			return nil
		end
		local at = nil
		for index, item in props.items do
			if item.key == key then
				at = index
			end
		end
		if at == nil then
			return nil
		end
		local to = at
		if event.key == "right" then
			to = math.min(at + 1, #props.items)
		elseif event.key == "left" then
			to = math.max(at - 1, 1)
		elseif event.key == "home" then
			to = 1
		elseif event.key == "end" then
			to = #props.items
		elseif presses(event) then
			return false
		else
			return nil
		end
		if to == at then
			return false
		end
		local item = props.items[to]
		return u:focus(u:name(id, "tab", item.key)), { kind = "select", key = item.key }
	end,
	drop = function(u, id, props, _, dropped)
		local source, target = u:part_of(dropped.source), dropped.target and u:part_of(dropped.target)
		if source == nil or target == nil or target.mount ~= id or source.part ~= "tab" or target.part ~= "tab" then
			return false
		end
		local from, to = nil, nil
		for index, item in props.items do
			if item.key == source.key then
				from = index
			end
			if item.key == target.key then
				to = index
			end
		end
		if from == nil or to == nil then
			return false
		end
		-- Dropped on a tab to the right, it goes after it: in front of the one that follows.
		local before = props.items[to]
		if to > from then
			before = props.items[to + 1]
		end
		return true, { kind = "move", key = source.key, before = before and before.key or nil }
	end,
	cursor = function()
		return nil
	end,
	settle = function(u, id, props, state)
		local active = props.active
		if active == nil or state.revealed == active then
			return false
		end
		local strip = u:name(id, "strip")
		local tab_box, strip_box = layout.bounds(u:name(id, "tab", active)), layout.bounds(strip)
		if tab_box == nil or strip_box == nil then
			return false
		end
		state.revealed = active
		local _, offset = layout.scroll_offset(strip)
		offset = offset or 0
		local left = tab_box.x - strip_box.x + offset
		local right = left + tab_box.w
		if left < offset then
			layout.scroll_to(strip, { x = left })
		elseif right > offset + strip_box.w then
			layout.scroll_to(strip, { x = right - strip_box.w })
		end
		return false
	end,
})

return controls
