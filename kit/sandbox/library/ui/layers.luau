-- @pencere/ui, the layers: what is drawn over the page. A modal with its scrim and its card, a dialog
-- that asks something, toasts, the menu of the host's own, and a card that stands by an element.
--
--   u:modal("sheet", { width = 560, label = "Settings" }, body)       -- dismiss
--   u:dialog("unsaved", { title = "Save changes to a.lua?", message = "…",
--       actions = { { key = "save", title = "Save", primary = true }, { key = "drop", title = "Don't save", danger = true } } })
--       -- answer { key, value }; key is nil where the person gave it up (escape, a click on the scrim)
--   u:toasts("toasts", { items = S.toasts })                          -- close { key }
--   u:menu("tab-menu", { { key = "close", title = "Close", keys = "cmd+w" }, "-", ... }, { near = "tab/…" })  -- a reply
--   u:popover("hint", { anchor = "save", side = "below" }, body)      -- dismiss

local core = require("./core")
local theme = require("./theme")

local layout = pencere.layout
local alpha = pencere.alpha

local layers = {}

-- The room around a card from the edge of the page, points.
local EDGE = 16

-- --- The shell of a modal ------------------------------------------------------------------------------

-- The element every modal is: an overlay over the whole page with a scrim that answers the pointer
-- (`<id>/scrim`), and a card in it that answers it too, so that a click on the card is not one on the
-- scrim (`<id>/card`). `options`: `width`, `top` (how far the card stands from the page's top: by
-- default it is centred in the upper third), `label`, `initial` (where the keyboard goes when it
-- opens). The layer is `id`'s: while it is in front the keyboard steps among what is in it.
function layers.shell(u, id, options, body)
	local t = u.theme
	u:layer(id, { initial = options.initial })
	local width = math.min(options.width or 480, math.max(u.size.w - 2 * EDGE, 120))
	local top = options.top or math.max(math.min(u.size.h * 0.18, 140), EDGE)
	local card = pencere.div()
		:press(u:part(id, "card"))
		:w(width)
		:no_shrink()
		:bg(t.popup)
		:rounded(t.radius.card)
		:border(1, t.line)
		:shadow(t.elevation.dx, t.elevation.dy, t.elevation.blur, t.elevation.color)
		:motion({ seconds = 0.14, enter = { opacity = 0, dy = -8, seconds = 0.14 } })
		:child(body)
	if options.label ~= nil then
		card:label(options.label)
	end
	return pencere.div()
		:overlay()
		:inset(0, 0, 0, 0)
		:items_center()
		:pt(top)
		:id(id)
		:child(
			pencere.div()
				:absolute()
				:inset(0, 0, 0, 0)
				:press(u:part(id, "scrim"))
				:bg(t.scrim)
				:motion({ seconds = 0.14, enter = { opacity = 0, seconds = 0.14 } })
		)
		:child(card)
end

-- What a click does on the parts of a shell: the scrim gives up (unless `dismiss` is false), the card
-- swallows it.
local function shell_click(part, props, answer)
	if part == "scrim" then
		if props.dismiss == false then
			return false
		end
		return true, answer
	elseif part == "card" then
		return false
	end
	return nil
end
layers.shell_click = shell_click

-- A modal: `body` in a card over a scrim. The page keeps whether it is open, by drawing it or not;
-- escape and a click on the scrim say `{ kind = "dismiss" }` (not with `dismiss = false`), and the
-- page stops drawing it. The keyboard goes into it when it opens and back where it was when it closes.
local Modal = core.component("modal", {
	view = function(u, id, props)
		return layers.shell(u, id, props, props.body)
	end,
	click = function(_, _, props, _, part)
		return shell_click(part, props, { kind = "dismiss" })
	end,
	dismiss = function(_, _, props)
		if props.dismiss == false then
			return false
		end
		return true, { kind = "dismiss" }
	end,
})

-- The modal `id` with `props` (above) and `body` its content. With nothing given, the name of the
-- modal that holds the keyboard now (the last drawn), or nil.
function core.Ctx:modal(id, props, body)
	if id == nil then
		return self:front_modal()
	end
	props = table.clone(props or {})
	props.body = body
	return Modal(self, id, props)
end

-- --- A dialog ---------------------------------------------------------------------------------------------

-- A question the page asks and the person answers: `title`, `message`, an `input` (`{ value, hint,
-- label }`: the answer is typed) and `actions` (`{ key, title, primary, danger }`, the last drawn
-- last). Enter does the `primary` one (the first, where none is); escape and a click on the scrim give
-- up. The answer is `{ kind = "answer", key = <the action's>, value = <the text> }`, with no `key` for
-- giving up: the page that keeps what it asks of the person stops drawing the dialog on any answer.
local Dialog = core.component("dialog", {
	state = function(props)
		return { value = props.input and props.input.value or "" }
	end,
	check = function(props)
		if type(props.title) ~= "string" then
			return "a dialog asks something: dialog(id, { title =, actions = { { key =, title = } } })"
		end
		if type(props.actions) ~= "table" or #props.actions == 0 then
			return "a dialog has actions to answer it with"
		end
		return nil
	end,
	view = function(u, id, props, state)
		local t = u.theme
		local primary = nil
		for _, act in props.actions do
			if act.primary then
				primary = act
				break
			end
		end
		primary = primary or props.actions[1]
		local body = pencere.div():p(20):gap(12)
		body:child(pencere.text(props.title):text_size(14):weight(600):wrap():role("heading", 2))
		if props.message ~= nil then
			body:child(pencere.text(props.message):color(t.ink_dim):wrap())
		end
		local initial = u:part(id, "action", primary.key)
		if props.input ~= nil then
			local field = u:input(u:part(id, "input"), {
				label = props.input.label or props.title,
				hint = props.input.hint,
				kind = props.input.kind,
				state = state,
				select = props.input.select,
			})
			body:child(field)
			initial = u:name(u:part(id, "input"), "field")
		end
		local row = pencere.div():row():justify_end():gap(8):pt(4)
		for _, act in props.actions do
			row:child(u:button(u:part(id, "action", act.key), {
				title = act.title,
				tone = act.danger and "danger" or act == primary and "primary" or nil,
			}))
		end
		body:child(row)
		return layers.shell(u, id, { width = props.width or 420, label = props.title, initial = initial }, body)
	end,
	click = function(_, _, _, _, part)
		return shell_click(part, {}, { kind = "answer" })
	end,
	dismiss = function()
		return true, { kind = "answer" }
	end,
	key = function(u, id, props, state, event)
		if event.key ~= "enter" then
			return nil
		end
		local mods = event.mods or {}
		if mods.command or mods.control or mods.alt or mods.shift then
			return nil
		end
		local primary = nil
		for _, act in props.actions do
			if act.primary then
				primary = act
				break
			end
		end
		primary = primary or props.actions[1]
		return true, { kind = "answer", key = primary.key, value = props.input and state.value or nil }
	end,
	action = function(u, id, props, state, act)
		local part = u:part_of(act.from)
		if part == nil then
			return nil
		end
		if part.mount == id and part.part == "action" and act.kind == "press" then
			return true, { kind = "answer", key = part.key, value = props.input and state.value or nil }
		elseif act.kind == "change" then
			-- The text typed in the dialog's field is its own: the page is told of the answer.
			return true
		elseif act.kind == "submit" then
			local primary = nil
			for _, candidate in props.actions do
				if candidate.primary then
					primary = candidate
					break
				end
			end
			primary = primary or props.actions[1]
			return true, { kind = "answer", key = primary.key, value = state.value }
		end
		return nil
	end,
})

-- --- Toasts -------------------------------------------------------------------------------------------------

local MAX_TOASTS = 4
local next_key = 0

-- A toast to add to the page's `items`: `words`, a tone ("info" by default), and how long it stays,
-- seconds (4 by default; 0 for one that stays until it is closed). The item.
function layers.toast(items, words, tone, seconds)
	next_key += 1
	seconds = seconds or 4
	local item = {
		key = next_key,
		words = words,
		tone = tone or "info",
		at = seconds > 0 and pencere.time.now() + seconds or nil,
	}
	table.insert(items, item)
	return item
end

-- The items that have not run out by `now`: the page's list of toasts, kept short.
function layers.toasts_prune(items, now)
	for index = #items, 1, -1 do
		local at = items[index].at
		if at ~= nil and at <= now then
			table.remove(items, index)
		end
	end
end

-- The toasts of the page, in a stack at the foot of the page's right edge, the newest below: `items`
-- are `{ key, words, tone, at }` (`ui.toast`). One that has run out by the time of the render is not
-- drawn, and the page is drawn again when the next does. At most four are shown. Closed with its
-- cross: `{ kind = "close", key }`.
local Toasts = core.component("toasts", {
	check = function(props)
		if type(props.items) ~= "table" then
			return "toasts are items: toasts(id, { items = })"
		end
		return nil
	end,
	view = function(u, id, props)
		local t = u.theme
		local stack = pencere.div():overlay():inset(nil, EDGE, EDGE, nil):items_end():gap(8):w(360):id(id)
		local shown = {}
		for _, item in props.items do
			if item.at == nil or item.at > u.now then
				table.insert(shown, item)
				if item.at ~= nil then
					u:wake_at(item.at)
				end
			end
		end
		for index = math.max(#shown - MAX_TOASTS + 1, 1), #shown do
			local item = shown[index]
			local tone = theme.tone(t, item.tone or "info")
			local close = u:icon_button(u:part(id, "close", item.key), {
				icon = "close",
				label = "Dismiss",
				size = 20,
			})
			stack:child(
				pencere.div()
					:row()
					:items_center()
					:gap(10)
					:w_full()
					:py(8)
					:pl(12)
					:pr(8)
					:bg(t.popup)
					:rounded(t.radius.card)
					:border(1, alpha(tone, 0.5))
					:shadow(0, 6, 18, alpha("shadow", 0.35))
					:motion({ seconds = 0.14, enter = { opacity = 0, dy = 8, seconds = 0.14 } })
					:id(u:part(id, "toast", item.key))
					:child(pencere.div():size(3, 14):no_shrink():rounded(2):bg(tone))
					:child(pencere.div():grow(1):min_w(0):child(pencere.text(item.words):wrap()))
					:child(close)
			)
		end
		return stack
	end,
	action = function(u, id, _, _, act)
		local part = u:part_of(act.from)
		if part == nil or part.mount ~= id or act.kind ~= "press" then
			return nil
		end
		return true, { kind = "close", key = part.key }
	end,
})

-- --- A menu of the host's ---------------------------------------------------------------------------------------

-- The host's menu, at the element `options.near` (below its left edge), or at `options.at` (`{ x, y }`):
-- the reply that asks for it, to be returned from `handle`. `entries` are `{ key, title, keys, enabled }`
-- and `"-"` for a rule. What the person picks comes back as `{ from = id, kind = "menu", entry = key }`.
function core.Ctx:menu(id, entries, options)
	options = options or {}
	local at = options.at
	if at == nil and options.near ~= nil then
		local box = layout.bounds(options.near)
		if box ~= nil then
			at = { x = box.x, y = box.y + box.h }
		end
	end
	if at == nil then
		error("a menu is at an element (near =) or a place (at = { x =, y = })", 2)
	end
	local asked, keys = {}, {}
	for index, entry in entries do
		if entry == "-" then
			asked[index] = "-"
		else
			asked[index] = { title = entry.title, keys = entry.keys, enabled = entry.enabled }
			keys[index] = entry.key
		end
	end
	self.asked = { id = id, keys = keys }
	return { redraw = true, menu = { at = at, entries = asked } }
end

-- --- A card by an element ----------------------------------------------------------------------------------------

local SIDES = { above = true, below = true, left = true, right = true }

-- `body` in a card that stands beside an element (`anchor`: its name, or `pencere.caret_of(editor)`),
-- on `side` of it ("below" by default) `gap` points away. It leaves the page as it is, the keyboard
-- where it is: escape, and a press outside it (not on its anchor), say `{ kind = "dismiss" }`.
local Popover = core.component("popover", {
	check = function(props)
		if type(props.anchor) ~= "string" then
			return "a popover stands by an element: popover(id, { anchor = <name of an element> }, body)"
		end
		if props.side ~= nil and not SIDES[props.side] then
			return 'a popover\'s side is "above", "below", "left" or "right", not ' .. tostring(props.side)
		end
		return nil
	end,
	view = function(u, id, props)
		local t = u.theme
		u:layer(id, { modal = false })
		local card = pencere.div()
			:anchor(props.anchor, props.side or "below", { gap = props.gap or 4, align = props.align })
			:id(id)
			:bg(t.popup)
			:rounded(t.radius.card)
			:border(1, t.line)
			:shadow(0, 8, 24, alpha("shadow", 0.4))
			:motion({ seconds = 0.12, enter = { opacity = 0, seconds = 0.12 } })
			:child(props.body)
		if props.width ~= nil then
			card:w(props.width)
		end
		if props.label ~= nil then
			card:label(props.label)
		end
		return card
	end,
	away = function(_, _, props, _, _, hit)
		if hit ~= nil and hit == props.anchor then
			return nil
		end
		return true, { kind = "dismiss" }
	end,
})

function core.Ctx:popover(id, props, body)
	props = table.clone(props or {})
	props.body = body
	return Popover(self, id, props)
end

layers.dialog = Dialog
layers.toasts = Toasts

return layers
