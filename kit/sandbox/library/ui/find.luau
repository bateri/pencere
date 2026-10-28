-- @pencere/ui, the find bar: the search and replace of an editor, as a bar over it.
--
--   u:find_bar("find", { editor = "doc", state = S.find, read_only = doc.read_only })
--       -- close, found { count, current }
--
-- The bar drives the editor with the editor's own asks (`find`, `find_next`, `find_previous`,
-- `replace`, `replace_all`, `stop_finding`: Section 2.3) and hears what it found (`found`,
-- `editor_refused`): it keeps no search of its own.

local core = require("./core")

local find = {}

-- The ask that begins (or changes) the search for what the bar says.
local function search_of(state)
	return {
		command = "find",
		pattern = state.query,
		regex = state.regex == true,
		case = state.case == true,
		word = state.word == true,
	}
end

-- What the bar says of what was found: "3 / 12", "No results", "Invalid pattern", and "10 000+" where the
-- editor stopped counting.
local function count_words(state)
	if state.query == "" then
		return ""
	elseif state.invalid then
		return "Invalid pattern"
	elseif state.count == nil then
		return ""
	elseif state.count == 0 then
		return "No results"
	end
	local total = tostring(state.count) .. (state.more and "+" or "")
	if state.current then
		return state.current .. " / " .. total
	end
	return total .. " found"
end

-- Puts what the bar says in the proxies of its fields, which the fields' own components keep.
local function sync(state)
	state.fields = state.fields or { query = { value = "" }, replace = { value = "" } }
	state.fields.query.value = state.query
	state.fields.replace.value = state.replacement
end

-- The find bar of the editor `editor`: a field for what to look for with the options of a search (match
-- case, whole word, regular expression: `state.case`, `word`, `regex`), the count of what is found,
-- the buttons that go to the next and the one before, and, with `state.replacing`, a field for what
-- to put in place and the buttons that replace one or all (not for a `read_only` editor). `state` is
-- the page's `{ query, replacement, case, word, regex, replacing }`: put what is selected in
-- `state.query` before opening the bar, and `u:focus(id)` to send the keyboard into it. The editor is
-- asked to find as the query and the options change, enter goes to the next (shift enter, the one
-- before) and escape closes the bar (and the editor stops finding). A bar for another editor than the
-- one it had asks the new one to find, and the old to stop. `compact` leaves the replace row and its
-- button out.
local Bar = core.component("find_bar", {
	state = function()
		return {
			query = "",
			replacement = "",
			case = false,
			word = false,
			regex = false,
			replacing = false,
		}
	end,
	check = function(props)
		if type(props.editor) ~= "string" then
			return "a find bar searches an editor: find_bar(id, { editor = <the editor's name> })"
		end
		return nil
	end,
	view = function(u, id, props, state)
		local t = u.theme
		sync(state)
		-- The bar was made, or made for another editor: the editor is asked, at the next frame.
		if state.editor ~= props.editor then
			local old = state.editor
			state.editor = props.editor
			state.count, state.current, state.invalid = nil, nil, false
			pencere.time.after(0, function()
				local asks = {}
				if old ~= nil then
					asks[old] = { command = "stop_finding" }
				end
				if state.query ~= "" then
					asks[props.editor] = search_of(state)
				end
				return next(asks) ~= nil and { editor = asks } or nil
			end)
		end
		local query = u:input(u:part(id, "query"), {
			state = state.fields.query,
			hint = "Find",
			label = "Find",
			escape = true,
			tone = state.invalid and "bad" or nil,
			select = true,
			trail = {
				u:toggle(u:part(id, "case"), { on = state.case, glyph = "Aa", label = "Match case", keys = "alt+mod+c" }),
				u:toggle(u:part(id, "word"), { on = state.word, glyph = "W", label = "Whole word", keys = "alt+mod+w" }),
				u:toggle(u:part(id, "regex"), { on = state.regex, glyph = ".*", label = "Regular expression", keys = "alt+mod+r" }),
			},
		})
		local row = pencere.div():row():items_center():gap(6):px(t.size.pad):py(6)
		if not props.compact then
			row:child(u:icon_button(u:part(id, "replacing"), {
				icon = state.replacing and "chevron_down" or "chevron_right",
				label = "Toggle replace",
				on = state.replacing,
				size = t.size.control - 4,
			}))
		end
		row:child(pencere.div():grow(1):min_w(120):max_w(360):child(query))
		row:child(
			pencere.div():min_w(78):no_shrink():child(
				pencere.text(count_words(state)):text_size(t.text.small):color(state.invalid and "error" or t.ink_dim)
			)
		)
		row:child(u:icon_button(u:part(id, "previous"), { icon = "chevron_up", label = "Previous match", keys = "shift+enter", size = t.size.control - 4 }))
		row:child(u:icon_button(u:part(id, "next"), { icon = "chevron_down", label = "Next match", keys = "enter", size = t.size.control - 4 }))
		row:child(u:icon_button(u:part(id, "close"), { icon = "close", label = "Close find", keys = "escape", size = t.size.control - 4 }))
		local bar = pencere.div():bg(t.surface):child(row)
		if state.replacing and not props.compact then
			local second = pencere.div():row():items_center():gap(6):px(t.size.pad):pb(6):pl(t.size.pad + t.size.control)
			second:child(
				pencere.div():grow(1):min_w(120):max_w(360):child(u:input(u:part(id, "replace"), {
					state = state.fields.replace,
					hint = "Replace",
					label = "Replace with",
					escape = true,
				}))
			)
			second:child(u:button(u:part(id, "replace_one"), { title = "Replace", disabled = props.read_only }))
			second:child(u:button(u:part(id, "replace_all"), { title = "All", disabled = props.read_only }))
			bar:child(second)
		end
		return bar:child(u:rule("x"))
	end,
	action = function(u, id, props, state, act)
		local part = u:part_of(act.from)
		if part == nil or part.mount ~= id then
			return nil
		end
		local editor = props.editor
		local function ask(command)
			return { editor = { [editor] = command } }
		end
		local function again()
			state.invalid, state.count, state.current = false, nil, nil
			if state.query == "" then
				state.count = 0
				return ask({ command = "stop_finding" })
			end
			return ask(search_of(state))
		end
		local name = part.part
		if name == "query" or name == "replace" then
			if act.kind == "change" then
				if name == "query" then
					state.query = act.text
					return again()
				end
				state.replacement = act.text
				return true
			elseif act.kind == "submit" then
				local mods = act.mods or {}
				if name == "replace" then
					return ask({ command = "replace", text = state.replacement })
				end
				return ask({ command = mods.shift and "find_previous" or "find_next" })
			elseif act.kind == "escape" then
				return pencere.merge(ask({ command = "stop_finding" }), true), { kind = "close" }
			end
		elseif name == "case" or name == "word" or name == "regex" then
			if act.kind == "toggle" then
				state[name] = act.on
				return again()
			end
		elseif name == "next" and act.kind == "press" then
			return ask({ command = "find_next" })
		elseif name == "previous" and act.kind == "press" then
			return ask({ command = "find_previous" })
		elseif name == "close" and act.kind == "press" then
			return pencere.merge(ask({ command = "stop_finding" }), true), { kind = "close" }
		elseif name == "replacing" and act.kind == "press" then
			state.replacing = not state.replacing
			return true
		elseif name == "replace_one" and act.kind == "press" then
			return ask({ command = "replace", text = state.replacement })
		elseif name == "replace_all" and act.kind == "press" then
			return ask({ command = "replace_all", text = state.replacement })
		end
		return nil
	end,
	editor = function(u, id, props, state, event)
		if event.type == "found" then
			state.count, state.current, state.more = event.count, event.current, event.more
			-- What the editor refused (a pattern that is none) stays refused until the query changes.
			if event.count > 0 then
				state.invalid = false
			end
			return true, { kind = "found", count = event.count, current = event.current }
		elseif event.type == "editor_refused" and event.reason == "bad-pattern" then
			state.invalid = true
			state.count, state.current = nil, nil
			return true
		end
		return nil
	end,
})
find.find_bar = Bar

return find
