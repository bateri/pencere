-- A page for the tests of what a terminal that draws the page in pixels tells it, and what the page can
-- ask of the terminal. Top left: a box that answers the pointer with a hand, a field of the host's, and an
-- editor. The page keeps a log of what it was told (the keys, the presses, the wheel, what a menu or a
-- panel gave) and says nothing on the screen: F6 opens a menu of one line whose pick puts the log and the
-- editor's text on the terminal's pasteboard (the person's own word, which is what a terminal is told).
-- F7 opens a menu of one line, F4 the Open panel, F5 the Save panel.

local b = pencere

local log = {}

local function say(text)
	log[#log + 1] = text
end

local function report()
	local length = pencere.layout.editor_length("doc") or 0
	local text = length > 0 and pencere.layout.editor_text("doc", 1, length + 1) or ""
	return table.concat(log, ";") .. "|" .. text
end

local function menu_of(title, tag)
	return { menu = { at = { x = 80, y = 54 }, entries = { { title = title, tag = tag } } } }
end

local function names(event)
	local mods = event.mods or {}
	return (mods.control and " ctrl" or "") .. (mods.shift and " shift" or "") .. (mods.alt and " alt" or "")
		.. (mods.command and " cmd" or "")
end

-- The box that is a hand: points, from the page's corner.
local function over_hand(event)
	return event.x >= 8 and event.x < 208 and event.y >= 8 and event.y < 68
end

return {
	render = function()
		return b.div():size_full():p(8):gap(8)
			:child(b.div():id("hand"):w(200):h(60))
			:child(b.div():text_field("find"):w_full():h(28):child(b.text("Find")))
			:child(b.editor("doc", { hint = "type here" }):w_full():h(72))
	end,

	handle = function(event)
		if event.type == "key" then
			if event.key == "f6" then
				return menu_of("report", "report:" .. report())
			elseif event.key == "f7" then
				return menu_of("Leave it", "leave")
			elseif event.key == "f4" then
				return { pick = { multiple = false } }
			elseif event.key == "f5" then
				return { save = { name = "out.txt" } }
			end
			say("key " .. event.key .. names(event))
		elseif event.type == "move" then
			return { cursor = over_hand(event) and "pointer" or "arrow" }
		elseif event.type == "press" then
			say(over_hand(event) and "press hand" or "press away")
		elseif event.type == "wheel" then
			say("wheel")
		elseif event.type == "menu" then
			if string.sub(event.entry.tag, 1, 7) == "report:" then
				return { copy = string.sub(event.entry.tag, 8) }
			end
			say("picked " .. event.entry.tag)
			return true
		elseif event.type == "files" then
			local file = event.files[1]
			say("given: " .. (file and (file.name .. " " .. file.size .. " " .. (file.path and "path" or "no path"))
				or "turned away"))
			return true
		elseif event.type == "saving" then
			say("given: " .. (event.handle and ("saving " .. event.name) or "turned away"))
			return true
		end
	end,
}
