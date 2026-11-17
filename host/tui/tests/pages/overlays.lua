-- A page for the tests of what a screen shows over a page: a menu, the panels, a link, and an
-- editor. Every one is asked for by a key (F1 a menu, F2 a link, F4 the Open panel, F5 the Save
-- panel, F8 and F9 links that are not for a browser) and what the person did with it is written on the page.

local b = pencere

local note = "ready"
local given = "nothing"

local function entries()
	return {
		{ title = "Open file", keys = "ctrl+o", tag = "open" },
		{ title = "Not now", enabled = false },
		"-",
		{ title = "Copy name", tag = "copy" },
		{ title = "Leave it", tag = "leave" },
	}
end

return {
	render = function()
		return b.div():size_full():p(8):gap(8)
			:child(b.text("Overlays"):weight(600))
			:child(b.text("note: " .. note))
			:child(b.text("given: " .. given))
			:child(b.editor("doc", { hint = "type here" }):w_full():h(72))
	end,

	handle = function(event)
		if event.type == "key" then
			if event.key == "f1" then
				return { menu = { at = { x = 80, y = 54 }, entries = entries() } }
			elseif event.key == "f2" then
				return { link = "https://example.org/pencere?x=1" }
			elseif event.key == "f3" then
				-- A page's own word on a key: not the person's, so not for the pasteboard.
				return { copy = "the page's own word" }
			elseif event.key == "f4" then
				return { pick = { multiple = false } }
			elseif event.key == "f5" then
				return { save = { name = "out.txt" } }
			elseif event.key == "f8" then
				-- Addresses the host does not open: not `http` or `https`.
				return { link = "javascript:alert(1)" }
			elseif event.key == "f9" then
				return { link = "file:///etc/passwd" }
			end
		elseif event.type == "menu" then
			note = "picked " .. event.entry.title
			if event.entry.tag == "copy" then
				return { copy = "from the menu", redraw = true }
			elseif event.entry.tag == "open" then
				return { pick = true, redraw = true }
			end
			return true
		elseif event.type == "files" then
			local file = event.files[1]
			given = file and (file.name .. " " .. file.size .. " " .. (file.path and "path" or "no path"))
				or "turned away"
			return true
		elseif event.type == "saving" then
			given = event.handle and ("saving " .. event.name) or "turned away"
			return true
		end
	end,
}
