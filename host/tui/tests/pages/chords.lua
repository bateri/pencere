-- A page for the tests of the keys a terminal has for a Mac's ⌘ (⌃⇧ and the letter): an editor at the
-- top left, and commands the page binds to ⌘ keys and to the chords a terminal sends. A command, when
-- it is done, opens a menu of one line named for it, and picking that line copies its name: which
-- command the page did is what the terminal's pasteboard is told. F1 opens a menu whose lines show
-- the ⌘ keys of an editor's own, F3 the same lines with the chords a terminal sends. Under the editor is a box that scrolls and zooms (the wheel with the
-- command key held is a zoom): F2 opens the same kind of menu, named for what the wheel did to it.

local b = pencere

local function lines()
	local box = b.div():id("list"):scroll():zoom():w_full():h(120):gap(2)
	for n = 1, 40 do
		box:child(b.text("line " .. n))
	end
	return box
end

-- A menu of what an editor's menu has, its keys written for a Mac (F1) or as a terminal sends them
-- (F3): the one a host shows for a Mac's keys is the other, but for the key the page took itself.
local function editor_menu(keys)
	return { menu = { at = { x = 80, y = 54 }, entries = {
		{ title = "Cut", keys = keys.cut },
		{ title = "Copy", keys = keys.copy },
		{ title = "Toggle Line Comment", keys = keys.comment },
		{ title = "Taken", keys = keys.taken },
		{ title = "Home", keys = keys.home },
	} } }
end

local function line(title)
	return { menu = { at = { x = 80, y = 54 }, entries = { { title = title, tag = title } } } }
end

return {
	render = function()
		return b.div():size_full():p(8):gap(8)
			:command("jay", "Jay", "cmd+j")
			:command("kay", "Kay", "cmd+k")
			:command("kay_own", "Kay's own", "ctrl+shift+k")
			:child(b.editor("doc", { hint = "type here" }):w_full():h(72))
			:child(lines())
	end,

	handle = function(event)
		if event.type == "command" then
			return line(event.id)
		elseif event.type == "key" and event.key == "f1" then
			-- The page binds ctrl+shift+k itself: a line with cmd+k has no key to show, and one with
			-- cmd+left (no chord) none either, nor has one with cmd+/ (⌃⇧/ is a sign's key, not a chord).
			return editor_menu({ cut = "cmd+x", copy = "cmd+c", comment = "cmd+/", taken = "cmd+k", home = "cmd+left" })
		elseif event.type == "key" and event.key == "f3" then
			return editor_menu({ cut = "ctrl+shift+x", copy = "ctrl+shift+c" })
		elseif event.type == "key" and event.key == "f2" then
			return line(pencere.layout.zoom_of("list") > 1.001 and "zoomed" or "scrolled")
		elseif event.type == "menu" then
			return { copy = event.entry.tag, redraw = true }
		end
	end,
}
