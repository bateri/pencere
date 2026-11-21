-- The view of browser-lua: an address line, the buttons of a browser and the browser, which the host
-- keeps (`pencere.browser`): its engine loads the pages and draws them in the room. It is a page of the
-- kit's page API, and an example of every part of the browser's face:
--
-- * the room: `pencere.browser("web", { profile = "browser-lua", keys = { "mod+l" } })`. Its profile
--   keeps what its sites keep apart from other programs'; its keys are the page's while the browser
--   has the keyboard (⌘L goes to the address line, the rest are the browser's);
-- * where it goes is asked, in a reply: `{ browser = { web = { go = address } } }`, and
--   `{ command = "back" | "forward" | "reload" | "stop" }`. The first address is asked when the host
--   says the browser is ready (a browser tells where it is when it is made), and a host that shows
--   none says so (`status = "unsupported"`) and the page says it too;
-- * what it tells (`browser` events) is all of the browser, so the page keeps the last and draws from
--   it: the address and the title, whether it loads, whether there is a page to go back or on to, its
--   status and the sentence for the person that comes with every status but ready, and a load that
--   failed;
-- * what was not done (`browser_refused`): an address of a kind a browser does not go to, or one it
--   is not given, which the page says where the person typed it.

local b = pencere
local ui = b.Ui.new()

local WEB = "web"
local ADDRESS = "address"
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
			:child(b.browser(WEB, { profile = "browser-lua", keys = { "mod+l" } }):grow(1):border(1, "border"))
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
		elseif event.type == "focus" then
			writing = event.field == ADDRESS
		elseif event.type == "text" and event.field == ADDRESS then
			typed = event.text
		elseif event.type == "key" and event.key == "enter" and typed ~= "" then
			return { browser = { [WEB] = { go = address_of(typed) } } }
		elseif event.type == "key" and event.key == "l" and event.mods.command then
			-- The browser had the keyboard and kept this key for the page.
			writing = true
			return { keyboard = { field = ADDRESS }, fill = { [ADDRESS] = { select = true } } }
		end
	end,
}
