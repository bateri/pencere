-- @pencere/ui, the small parts: a button, a button of an icon alone, a toggle, a badge, the caps of
-- a shortcut, a rule and a banner.
--
--   u:button("save", { title = "Save", tone = "primary", keys = "cmd+s" })     -- press
--   u:icon_button("close", { icon = "close", label = "Close" })                -- press
--   u:toggle("case", { on = case, glyph = "Aa", label = "Match case" })        -- toggle { on }
--   u:badge("12", { tone = "info" }), u:kbd("cmd+k"), u:rule("x")
--   u:banner("conflict", { tone = "warn", words = "Changed on disk",
--       actions = { { key = "reload", title = "Reload" } } })                  -- press { key }

local core = require("./core")
local theme = require("./theme")
local look = require("./look")
local keys = require("@pencere/keys")

local alpha, mix = pencere.alpha, pencere.mix

local basic = {}

-- Whether a key event is enter or the space bar with no modifier: what presses what has the keyboard.
local function presses(event)
	local mods = event.mods or {}
	if mods.command or mods.control or mods.alt or mods.shift then
		return false
	end
	return event.key == "enter" or event.key == " "
end

local function check_icon(props, who)
	if props.icon ~= nil then
		local wrong = look.check(props.icon)
		if wrong then
			return who .. "'s icon: " .. wrong
		end
	end
	return nil
end

-- --- A button -----------------------------------------------------------------------------------------

local BUTTON_TONES = { neutral = true, primary = true, danger = true }

-- A button: its words (`title`), an icon before them, a tone ("neutral", "primary" for what a
-- person most likely does, "danger" for what cannot be taken back), the keys that do the same
-- (`keys`, said in its tip), the tip's words, a label for a screen reader where the words do not
-- say it, and `disabled`. Pressed with the pointer, or with enter or the space bar while it has the
-- keyboard: `{ kind = "press" }`.
basic.button = core.component("button", {
	check = function(props)
		if props.title == nil and props.label == nil then
			return "a button says what it does: its title, or the label of one that is an icon alone"
		end
		if props.title ~= nil and type(props.title) ~= "string" and type(props.title) ~= "number" then
			return "a button's title is words, not " .. type(props.title)
		end
		if props.tone ~= nil and not BUTTON_TONES[props.tone] then
			return 'a button\'s tone is "neutral", "primary" or "danger", not ' .. tostring(props.tone)
		end
		return check_icon(props, "a button")
	end,
	view = function(u, id, props)
		local t = u.theme
		local tone = props.tone or "neutral"
		local held, over = u:pressed(id), u:hovered(id)
		local ground, ink
		if tone == "primary" then
			ground = held and mix("accent", "foreground", 0.2) or over and mix("accent", "foreground", 0.12) or "accent"
			ink = "background"
		elseif tone == "danger" then
			ground = alpha("error", held and 0.3 or over and 0.24 or 0.14)
			ink = "error"
		else
			ground = held and t.control_press or over and t.control_hover or t.control
			ink = t.ink
		end
		local button = pencere.div()
			:press(id)
			:focusable()
			:role("button")
			:row()
			:items_center()
			:justify_center()
			:gap(t.size.gap)
			:h(t.size.control)
			:px(t.size.pad)
			:no_shrink()
			:rounded(t.radius.control)
			:bg(ground)
			:border(1, u:focus_visible(id) and t.ring or t.clear)
			:color(ink)
			:motion({ seconds = t.motion, color = true })
		local label = props.label or props.title
		button:label(tostring(label))
		if props.icon ~= nil then
			button:child(look.icon(props.icon, t.size.icon, ink))
		end
		if props.title ~= nil then
			button:child(pencere.text(props.title):weight(500))
		end
		u:tip(button, props.tip or (props.keys and tostring(label)), props.keys)
		if props.disabled then
			button:disabled(true):opacity(0.45)
		end
		return button
	end,
	click = function(_, _, props)
		if props.disabled then
			return nil
		end
		return true, { kind = "press" }
	end,
	key = function(_, _, props, _, event)
		if props.disabled or not presses(event) then
			return nil
		end
		return true, { kind = "press" }
	end,
})

-- --- A button of an icon alone ------------------------------------------------------------------------

-- A button that is an icon alone: its `label` says what it does, to a screen reader and in its tip
-- (with its `keys`). `on` (true or false) makes it one that stays down while what it stands for is
-- on (a panel shown); `size` is its side (the theme's control). Pressed: `{ kind = "press" }`.
basic.icon_button = core.component("icon_button", {
	check = function(props)
		if type(props.label) ~= "string" then
			return "a button of an icon alone says what it does in words: icon_button(id, { icon =, label = })"
		end
		if props.icon == nil then
			return "an icon button has an icon"
		end
		return check_icon(props, "an icon button")
	end,
	view = function(u, id, props)
		local t = u.theme
		local side = props.size or t.size.control
		local held, over = u:pressed(id), u:hovered(id)
		local ground = nil
		if held then
			ground = t.control_press
		elseif props.on then
			ground = t.on
		elseif over then
			ground = t.control_hover
		end
		local ink = props.on and "accent" or (over and t.ink or t.ink_dim)
		local button = pencere.div()
			:press(id)
			:focusable()
			:size(side, side)
			:no_shrink()
			:items_center()
			:justify_center()
			:rounded(t.radius.control)
			:bg(ground)
			:border(1, u:focus_visible(id) and t.ring or t.clear)
			:motion({ seconds = t.motion, color = true })
			:label(props.label)
			:child(look.icon(props.icon, math.min(t.size.icon, side - 6), ink))
		if props.on ~= nil then
			button:role("switch"):checked(props.on == true)
		else
			button:role("button")
		end
		u:tip(button, props.label, props.keys)
		if props.disabled then
			button:disabled(true):opacity(0.45)
		end
		return button
	end,
	click = function(_, _, props)
		if props.disabled then
			return nil
		end
		return true, { kind = "press" }
	end,
	key = function(_, _, props, _, event)
		if props.disabled or not presses(event) then
			return nil
		end
		return true, { kind = "press" }
	end,
})

-- --- A toggle -----------------------------------------------------------------------------------------

-- An option that is on or off, drawn as a small cap of words (`glyph`: "Aa", ".*") or an icon: the
-- options of a search. `label` says what it is, to a screen reader and in its tip (with `keys`).
-- Pressed, or enter or the space bar on it: `{ kind = "toggle", on = <what it is to become> }`; the
-- page keeps whether it is on and gives it back as `on`.
basic.toggle = core.component("toggle", {
	check = function(props)
		if type(props.label) ~= "string" then
			return "a toggle says what it is in words: toggle(id, { glyph =, label = })"
		end
		if props.glyph == nil and props.icon == nil then
			return "a toggle shows a glyph (\"Aa\") or an icon"
		end
		return check_icon(props, "a toggle")
	end,
	view = function(u, id, props)
		local t = u.theme
		local held, over = u:pressed(id), u:hovered(id)
		local ground = nil
		if held then
			ground = t.control_press
		elseif props.on then
			ground = t.on
		elseif over then
			ground = t.control_hover
		end
		local ink = props.on and t.ink or t.ink_dim
		local toggle = pencere.div()
			:press(id)
			:focusable()
			:role("checkbox")
			:checked(props.on == true)
			:label(props.label)
			:row()
			:items_center()
			:justify_center()
			:h(t.size.control - 4)
			:min_w(t.size.control - 2)
			:px(4)
			:no_shrink()
			:rounded(t.radius.chip + 1)
			:bg(ground)
			:border(1, u:focus_visible(id) and t.ring or t.clear)
			:color(ink)
			:motion({ seconds = t.motion, color = true })
		if props.icon ~= nil then
			toggle:child(look.icon(props.icon, t.size.icon - 2, ink))
		else
			toggle:child(pencere.text(props.glyph):text_size(t.text.small):weight(600))
		end
		u:tip(toggle, props.label, props.keys)
		if props.disabled then
			toggle:disabled(true):opacity(0.45)
		end
		return toggle
	end,
	click = function(_, _, props)
		if props.disabled then
			return nil
		end
		return true, { kind = "toggle", on = not props.on }
	end,
	key = function(_, _, props, _, event)
		if props.disabled or not presses(event) then
			return nil
		end
		return true, { kind = "toggle", on = not props.on }
	end,
})

-- --- A badge, the caps of a shortcut, a rule ---------------------------------------------------------

-- A badge: a count, a state in a word. `props.tone` colours it ("info", "ok", "warn", "bad",
-- "quiet" or a role); `props.round` makes it a pill. It answers nothing and is named by nothing.
function core.Ctx:badge(words, props)
	if type(words) ~= "string" and type(words) ~= "number" then
		error("a badge shows words or a number, not " .. type(words), 2)
	end
	props = props or {}
	local t = self.theme
	local high = props.h or 18
	local badge = pencere.div()
		:row()
		:items_center()
		:h(high)
		:px(6)
		:no_shrink()
		:rounded(props.round and high / 2 or t.radius.chip)
		:text_size(t.text.tiny)
		:weight(600)
	if props.tone ~= nil then
		local tone = theme.tone(t, props.tone)
		badge:bg(alpha(tone, 0.16)):color(tone)
	else
		badge:bg(t.chip):color(t.ink_dim)
	end
	return badge:child(pencere.text(words))
end

-- The keys of a shortcut ("cmd+shift+k") as the platform writes them: a cap for each, or, with
-- `props.quiet`, the label alone in quiet words.
function core.Ctx:kbd(shortcut, props)
	if type(shortcut) ~= "string" then
		error("kbd takes keys as words: \"cmd+k\", not " .. type(shortcut), 2)
	end
	props = props or {}
	local t = self.theme
	if props.quiet then
		return pencere.text(keys.label(shortcut)):text_size(t.text.small):color(t.ink_quiet)
	end
	local caps = pencere.div():row():items_center():gap(2):no_shrink()
	for _, cap in keys.caps(shortcut) do
		caps:child(
			pencere.div()
				:row()
				:items_center()
				:justify_center()
				:h(16)
				:min_w(16)
				:px(4)
				:rounded(t.radius.chip)
				:bg(t.chip)
				:color(t.ink_dim)
				:text_size(t.text.tiny)
				:child(pencere.text(cap))
		)
	end
	return caps
end

-- A rule: a line of the theme's `line` (or `color`) a point thick, across ("x") or down ("y") what
-- it is in.
function core.Ctx:rule(axis, color)
	color = color or self.theme.line
	if axis == "y" then
		return pencere.div():w(1):no_shrink():bg(color)
	elseif axis == nil or axis == "x" then
		return pencere.div():h(1):no_shrink():bg(color)
	end
	error('a rule goes across ("x") or down ("y"), not ' .. tostring(axis), 2)
end

-- --- A banner -----------------------------------------------------------------------------------------

-- A strip that says something of what is below it, in a tone, with buttons that answer it: a file
-- changed on disk, a draft to restore. `words` is a string or an element (a `pencere.rich`);
-- `actions` are `{ key, title, primary, keys }`; `dismiss` (true, or the words of its label) adds a
-- button that closes it. Its buttons: `{ kind = "press", key = <the action's key> }` and
-- `{ kind = "dismiss" }`.
basic.banner = core.component("banner", {
	check = function(props)
		if props.words == nil then
			return "a banner says something: banner(id, { words = })"
		end
		if props.actions ~= nil and type(props.actions) ~= "table" then
			return "a banner's actions are a list of { key =, title = }"
		end
		return check_icon(props, "a banner")
	end,
	view = function(u, id, props)
		local t = u.theme
		local tone = theme.tone(t, props.tone or "info")
		local row = pencere.div()
			:row()
			:items_center()
			:gap(8)
			:min_h(t.size.control + 8)
			:px(t.size.pad)
			:py(4)
		if props.icon ~= nil then
			row:child(look.icon(props.icon, t.size.icon, tone))
		end
		local words = props.words
		if type(words) == "string" or type(words) == "number" then
			words = pencere.text(words):wrap()
		end
		row:child(pencere.div():grow(1):min_w(0):child(words))
		for _, act in props.actions or {} do
			row:child(u:button(u:part(id, "action", act.key), {
				title = act.title,
				tone = act.primary and "primary" or nil,
				keys = act.keys,
			}))
		end
		if props.dismiss then
			row:child(u:icon_button(u:part(id, "dismiss"), {
				icon = "close",
				label = type(props.dismiss) == "string" and props.dismiss or "Dismiss",
				size = t.size.control - 4,
			}))
		end
		return pencere.div():bg(alpha(tone, 0.1)):child(row):child(u:rule("x", alpha(tone, 0.3)))
	end,
	action = function(u, id, _, _, act)
		local part = u:part_of(act.from)
		if act.kind ~= "press" or part == nil or part.mount ~= id then
			return nil
		end
		if part.part == "dismiss" then
			return true, { kind = "dismiss" }
		end
		return true, { kind = "press", key = part.key }
	end,
})

return basic
