-- The view of browser-lua: an address line, the buttons of a browser and the browser, which the host
-- keeps (`pencere.browser`): its engine loads the pages and draws them in the room. It is a page of the
-- kit's page API, and an example of every part of the browser's face:
--
-- * the room: `pencere.browser("web", { profile = "browser-lua", windows = "page", keys = { "mod+l",
--   "mod+f", "mod+=", "mod+-", "mod+0" } })`. Its profile keeps what its sites keep apart from other
--   programs'; its keys are the page's while the browser has the keyboard (⌘L goes to the address
--   line, ⌘F to the find bar, ⌘= ⌘- ⌘0 zoom; the rest are the browser's, and the ones it has no use for
--   come to the page too: ⌘← goes back);
-- * where it goes is asked, in a reply: `{ browser = { web = { go = address } } }`, and
--   `{ command = "back" | "forward" | "reload" | "stop" }`. The first address is asked when the host
--   says the browser is ready (a browser tells where it is when it is made), and a host that shows
--   none says so (`status = "unsupported"`) and the page says it too. The same reply asks the rest:
--   `{ command = "find", pattern = words }` with `"find_next"`, `"find_previous"` and
--   `"stop_finding"` (the bar is the page's, the browser finds and marks), and `{ zoom = size }`
--   (the steps are the page's);
-- * what it tells (`browser` events) is all of the browser, so the page keeps the last and draws from
--   it: the address and the title, whether it loads, whether there is a page to go back or on to, its
--   status and the sentence for the person that comes with every status but ready, a load that
--   failed, what a search finds (`found`) and how large the page is shown (`zoom`);
-- * what was not done (`browser_refused`): an address of a kind a browser does not go to, or one it
--   is not given, which the page says where the person typed it;
-- * a link that opens a new window (`windows = "page"`): the page follows it, or, where the person
--   asked for it behind this page (`background`), says so and stays.

local b = pencere
local ui = b.Ui.new()

local WEB = "web"
local ADDRESS = "address"
local FIND = "find"
local HOME = "https://example.com/"

-- What the browser last said it is; nil before it has said anything.
local state = nil
-- The first address is asked once.
local asked = false
-- What the address line holds, and the address it was filled with last.
local typed = ""
local filled = nil
-- The person is writing in the address line: the keyboard is in it.
local writing = false
local note = ""
-- The find bar is open, and the field the keyboard is in.
local finding = false
local focused = nil

-- The sizes a page is shown at, one step to the next.
local STEPS = { 0.25, 0.33, 0.5, 0.67, 0.75, 0.8, 0.9, 1, 1.1, 1.25, 1.5, 1.75, 2, 2.5, 3, 4, 5 }

-- The step after `zoom` (`direction` 1) or before it (-1), and the last one there is at the end.
local function stepped(zoom, direction)
	local chosen = zoom
	for _, step in ipairs(STEPS) do
		if direction > 0 and step > zoom + 0.001 then
			return step
		elseif direction < 0 and step < zoom - 0.001 then
			chosen = step
		end
	end
	return chosen
end

local function status()
	return state and state.status or "starting"
end

-- An address as a person types one: with no kind of its own it is a web address.
local function address_of(words)
	words = string.match(words, "^%s*(.-)%s*$")
	if string.match(words, "^[%a][%w+.-]*:") then
		return words
	end
	return "https://" .. words
end

local function button(id, label, enabled)
	local item = b.div():id(id):px(10):py(5):rounded(6)
	if enabled then
		return item:press(id):bg(ui.hover == id and "raised3" or "raised2"):child(b.text(label))
	end
	return item:bg("raised1"):child(b.text(label):color("dim"))
end

local function title()
	if state == nil then
		return "Starting"
	end
	if state.error then
		return "Could not load " .. state.error.url .. ": " .. state.error.message
	end
	if status() ~= "ready" then
		return state.message or "The browser is not ready"
	end
	local words = state.title ~= "" and state.title or state.url
	return state.loading and ("Loading " .. words) or words
end

local function ask(command)
	return { browser = { [WEB] = { command = command } } }
end

-- What the search has found, in words.
local function found_words()
	local found = state and state.found
	if found == nil then
		return ""
	elseif found.current == nil then
		return "No matches"
	end
	return found.current .. " of " .. found.count
end

local function find_bar()
	return b.div():row():items_center():gap(8):px(8):pb(8)
		:child(b.div():text_field(FIND):grow(1):min_w(0):h(22):px(8):rounded(6):bg("raised1")
			:child(b.text("Find")))
		:child(b.text(found_words()):color("dim"))
		:child(button("find_done", "Done", true))
end

-- The bar is closed: the search ends, its marks go, and the keyboard is the page's.
local function close_find()
	finding = false
	focused = nil
	local reply = ask("stop_finding")
	reply.keyboard = "page"
	reply.redraw = true
	return reply
end

return {
	render = function()
		local ready = status() == "ready"
		return b.div():size_full():gap(0)
			:child(b.div():row():items_center():gap(8):p(8)
				:child(button("back", "Back", ready and state.back))
				:child(button("forward", "Forward", ready and state.forward))
				:child(button(state and state.loading and "stop" or "reload", state and state.loading and "Stop" or "Reload", ready))
				:child(b.div():text_field(ADDRESS):grow(1):min_w(0):h(22):px(8):rounded(6):bg("raised1")
					:child(b.text("Address"))))
			:child(finding and find_bar() or b.div())
			:child(b.browser(WEB, { profile = "browser-lua", windows = "page", keys = { "mod+l", "mod+f", "mod+=", "mod+-", "mod+0" } }):grow(1):border(1, "border"))
			:child(b.div():px(12):py(6):child(b.text(note ~= "" and note or title()):color("dim"):truncate()))
	end,

	handle = function(event)
		if event.type == "move" then
			return ui:point(b.layout, event)
		elseif event.type == "leave" then
			return ui:point(b.layout, nil)
		elseif event.type == "press" then
			ui:press(b.layout, event)
		elseif event.type == "release" then
			local released = ui:release(b.layout, event)
			if released == "back" or released == "forward" or released == "reload" or released == "stop" then
				return ask(released)
			elseif released == "find_done" then
				return close_find()
			end
		elseif event.type == "browser" and event.browser == WEB then
			state = event
			local reply = { redraw = true }
			-- A browser that is made asks nothing of the page and shows nothing: where to go first is
			-- the page's.
			if not asked and event.status == "ready" then
				asked = true
				reply.browser = { [WEB] = { go = HOME } }
			end
			-- The address line follows the browser, unless the person is writing in it.
			if event.url ~= "" and event.url ~= filled and not writing then
				filled = event.url
				typed = event.url
				reply.fill = { [ADDRESS] = event.url }
			end
			note = ""
			return reply
		elseif event.type == "browser_refused" then
			note = event.reason == "scheme" and "A browser goes to http, https, about:blank and data: addresses only."
				or "That is not an address a browser is given."
			return true
		elseif event.type == "browser_window" then
			if event.background then
				note = "A link to open behind this page: " .. event.url
				return true
			end
			return { browser = { [WEB] = { go = event.url } } }
		elseif event.type == "focus" then
			focused = event.field
			writing = event.field == ADDRESS
		elseif event.type == "text" and event.field == ADDRESS then
			typed = event.text
		elseif event.type == "text" and event.field == FIND then
			-- An empty search ends it.
			return { browser = { [WEB] = { command = "find", pattern = event.text, case = false } } }
		elseif event.type == "key" and focused == FIND and event.key == "enter" then
			return ask(event.mods.shift and "find_previous" or "find_next")
		elseif event.type == "key" and finding and event.key == "escape" then
			return close_find()
		elseif event.type == "key" and event.key == "f" and event.mods.command then
			-- The browser had the keyboard and kept this key for the page. The keyboard goes to the bar
			-- where the page sent it, which the page is not told.
			finding = true
			focused = FIND
			return { keyboard = { field = FIND }, fill = { [FIND] = { select = true } }, redraw = true }
		elseif event.type == "key" and event.mods.command and (event.key == "=" or event.key == "-" or event.key == "0") then
			local zoom = event.key == "0" and 1 or stepped(state and state.zoom or 1, event.key == "=" and 1 or -1)
			return { browser = { [WEB] = { zoom = zoom } } }
		elseif event.type == "key" and event.key == "left" and event.mods.command then
			-- Not a key of the page's list: the browser was given it and had no use for it.
			return ask("back")
		elseif event.type == "key" and event.key == "enter" and typed ~= "" then
			return { browser = { [WEB] = { go = address_of(typed) } } }
		elseif event.type == "key" and event.key == "l" and event.mods.command then
			-- The browser had the keyboard and kept this key for the page.
			writing = true
			return { keyboard = { field = ADDRESS }, fill = { [ADDRESS] = { select = true } } }
		end
	end,
}
