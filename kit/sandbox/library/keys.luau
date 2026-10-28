-- @pencere/keys: the keys of a shortcut, as a page writes them and as a person reads them.
--
-- A page writes keys as it writes a command's (`"cmd+shift+k"`, `"alt+backspace"`, `"f5"`: the
-- modifiers and a key joined by `+`, read by `pencere.keys`). This module says them as the
-- platform writes them, for a button's tip, a menu, a list of commands, the help of a page, and
-- tells whether a key the page is told of is them:
--
--   local keys = require("@pencere/keys")
--   keys.label("cmd+shift+k")             -- "⇧⌘K"
--   keys.label("ctrl+alt+delete", { platform = "other" })   -- "Ctrl+Alt+Delete"
--   keys.caps("cmd+enter")                -- { "⌘", "↩" }: one for each key cap drawn
--   keys.matches(event, "cmd+k")          -- whether a key event is that shortcut
--
-- The platform: the kit does not tell a page which platform its host runs on (the interface
-- carries no word of it). The labels are a Mac's, as Apple's guidelines write them (the modifiers
-- in the order ⌃⌥⇧⌘, then the key, with nothing between), unless the page says another platform
-- for one label (`options.platform`) or for all (`keys.set_platform("other")`): every host of the
-- kit today runs on macOS. When the kit's interface carries the host's word, that word will be the
-- default in place of "mac". On any other platform a label is the names of the keys joined by
-- `+`, in the order Ctrl, Alt, Shift, Super.
--
-- `cmd` is the command key. The kit reads `cmd`, `command`, `super` and `meta` as one modifier
-- (`mods.command`) and a host matches it as it is, never as control: off a Mac it is the key with
-- the system's logo, which the label calls "Super". A page whose shortcuts are control's off a Mac
-- writes `ctrl` there.
--
-- A letter is written as a capital, and a character is the same key in either case (shift says
-- which), as the host matches a command's keys: by Unicode's simple mapping for ASCII, Latin-1,
-- Latin Extended-A, Greek and Cyrillic; a character of another script is compared as it is.
--
-- It is written on the page API alone: `pencere.keys` reads the words.

local keys = {}

-- The words of a key that is not a character, as `pencere.keys` gives them (in small letters), and
-- the name a key event gives the key.
local NAMES = {
	up = "up",
	down = "down",
	left = "left",
	right = "right",
	page_up = "page_up",
	pageup = "page_up",
	page_down = "page_down",
	pagedown = "page_down",
	home = "home",
	["end"] = "end",
	enter = "enter",
	escape = "escape",
	tab = "tab",
	backspace = "backspace",
	delete = "delete",
	del = "delete",
}
for number = 1, 20 do
	NAMES["f" .. number] = "f" .. number
end

-- How each platform writes the modifiers, in its order, and the keys that are not characters.
local MODIFIERS = {
	mac = { { "control", "⌃" }, { "alt", "⌥" }, { "shift", "⇧" }, { "command", "⌘" } },
	other = { { "control", "Ctrl" }, { "alt", "Alt" }, { "shift", "Shift" }, { "command", "Super" } },
}
local KEYS = {
	mac = {
		up = "↑",
		down = "↓",
		left = "←",
		right = "→",
		page_up = "⇞",
		page_down = "⇟",
		home = "↖",
		["end"] = "↘",
		enter = "↩",
		escape = "⎋",
		tab = "⇥",
		backspace = "⌫",
		delete = "⌦",
		[" "] = "␣",
	},
	other = {
		up = "Up",
		down = "Down",
		left = "Left",
		right = "Right",
		page_up = "PageUp",
		page_down = "PageDown",
		home = "Home",
		["end"] = "End",
		enter = "Enter",
		escape = "Esc",
		tab = "Tab",
		backspace = "Backspace",
		delete = "Delete",
		[" "] = "Space",
	},
}
local MODIFIER_NAMES = { "shift", "control", "alt", "command" }

-- The platform the labels are for, unless a call says another.
local platform = "mac"

-- --- Letters ---------------------------------------------------------------------------------

-- A letter in the other case: SMALL[capital] is its small letter, CAPITAL[small] its capital.
local SMALL, CAPITAL = {}, {}
local function pair(capital, small)
	SMALL[capital] = small
	if CAPITAL[small] == nil then
		CAPITAL[small] = capital
	end
end
local function shifted(from, to, by)
	for code = from, to do
		pair(code, code + by)
	end
end
-- Letters that come in pairs, a capital and its small letter after it.
local function pairs_from(from, to)
	for code = from, to - 1, 2 do
		pair(code, code + 1)
	end
end
shifted(0x41, 0x5A, 32)
shifted(0xC0, 0xD6, 32)
shifted(0xD8, 0xDE, 32)
pairs_from(0x100, 0x12F)
-- İ is i, and ı a small letter whose capital is I (which is i's).
pair(0x130, 0x69)
CAPITAL[0x131] = 0x49
pairs_from(0x132, 0x137)
pairs_from(0x139, 0x148)
pairs_from(0x14A, 0x177)
pair(0x178, 0xFF)
pairs_from(0x179, 0x17E)
shifted(0x391, 0x3A1, 32)
shifted(0x3A3, 0x3AB, 32)
shifted(0x400, 0x40F, 80)
shifted(0x410, 0x42F, 32)

-- One character in small letters, or as a capital; any other text as it is.
local function cased(character, table_of)
	if utf8.len(character) ~= 1 then
		return character
	end
	local code = utf8.codepoint(character)
	local other = table_of[code]
	return other and utf8.char(other) or character
end

-- --- Words -------------------------------------------------------------------------------------

-- `words` read as keys: { key =, mods = }, the key as a key event names it; or nil and why.
local function read(words)
	local ok, parsed = pcall(pencere.keys, words)
	if not ok then
		return nil, tostring(parsed)
	end
	local key = parsed.key
	local length = utf8.len(key)
	if length == 0 then
		return nil, '"' .. words .. '" says no key after its modifiers'
	elseif length ~= 1 then
		local name = NAMES[key]
		if name == nil then
			return nil,
				'"'
					.. key
					.. '" is not a key: one character, or up, down, left, right, page_up, page_down, home, end, '
					.. "enter, escape, tab, backspace, delete, space or f1 to f20"
		end
		parsed.key = name
	end
	return parsed
end

-- Keys a function of this module was given: words, or what `keys.parse` made of some. An error at
-- the caller's line (`level`) for what is neither.
local function keys_of(given, name, level)
	local kind = type(given)
	if kind == "string" then
		local parsed, why = read(given)
		if parsed == nil then
			error(why, level + 1)
		end
		return parsed
	elseif kind == "table" and type(given.key) == "string" and type(given.mods) == "table" then
		return given
	end
	error(name .. ' takes keys written as words ("cmd+k") or what keys.parse made of them, not a ' .. kind, level + 1)
end

local function platform_of(options, name, level)
	if options == nil then
		return platform
	end
	if type(options) ~= "table" then
		error(name .. " takes options as a table ({ platform = }), not a " .. type(options), level + 1)
	end
	for key in next, options do
		if key ~= "platform" then
			error(name .. " has no option " .. tostring(key) .. " (platform)", level + 1)
		end
	end
	local chosen = options.platform
	if chosen == nil then
		return platform
	elseif not MODIFIERS[chosen] then
		error('a platform is "mac" or "other", not ' .. tostring(chosen), level + 1)
	end
	return chosen
end

-- Keys written as words, `"cmd+shift+k"`, read: `{ key =, mods = { shift =, control =, alt =,
-- command = } }`, the key named as a key event names it (`"k"`, `"enter"`, `"page_up"`, `"f5"`,
-- `" "` for the space bar); or nil and why where the words are not keys (a shortcut a person typed
-- in a setting). It is `pencere.keys`, with the key's name checked.
function keys.parse(words)
	if type(words) ~= "string" then
		error('keys.parse takes keys written as words ("cmd+k"), not a ' .. type(words), 2)
	end
	return read(words)
end

-- The keys as the platform writes them, one for each key cap: `{ "⇧", "⌘", "K" }` on a Mac,
-- `{ "Ctrl", "Shift", "K" }` elsewhere. `words` are keys written as words or what `keys.parse`
-- made of them; `options.platform` is "mac" or "other" (`keys.platform()` where it says none).
function keys.caps(words, options)
	local parsed = keys_of(words, "keys.caps", 2)
	local chosen = platform_of(options, "keys.caps", 2)
	local caps = {}
	for _, modifier in ipairs(MODIFIERS[chosen]) do
		if parsed.mods[modifier[1]] then
			caps[#caps + 1] = modifier[2]
		end
	end
	local key = parsed.key
	local cap = KEYS[chosen][key]
	if cap == nil then
		if string.match(key, "^f%d%d?$") then
			cap = string.upper(key)
		else
			cap = cased(key, CAPITAL)
		end
	end
	caps[#caps + 1] = cap
	return caps
end

-- The keys as the platform writes them: `"⇧⌘K"`, `"⌘↩"`, `"⌥⌫"` on a Mac, `"Ctrl+Shift+K"`
-- elsewhere (options as `keys.caps`).
function keys.label(words, options)
	local parsed = keys_of(words, "keys.label", 2)
	local chosen = platform_of(options, "keys.label", 2)
	return table.concat(keys.caps(parsed, { platform = chosen }), chosen == "mac" and "" or "+")
end

-- Whether the key event `event` (`{ type = "key", key =, mods = }`, or a `key_up`) is the
-- shortcut `words`: the same modifiers, no more and no fewer, and the same key, a character in
-- either case. A command's keys are matched so by the host. An event of another type is not.
function keys.matches(event, words)
	local parsed = keys_of(words, "keys.matches", 2)
	if type(event) ~= "table" then
		error("keys.matches takes a key event, then keys, not a " .. type(event), 2)
	end
	local kind = event.type
	if kind ~= nil and kind ~= "key" and kind ~= "key_up" then
		return false
	end
	local mods = event.mods
	if type(mods) ~= "table" then
		mods = {}
	end
	for _, name in ipairs(MODIFIER_NAMES) do
		if (mods[name] == true) ~= (parsed.mods[name] == true) then
			return false
		end
	end
	local key = event.key
	if type(key) ~= "string" then
		return false
	end
	if key == parsed.key then
		return true
	end
	return utf8.len(key) == 1 and cased(key, SMALL) == cased(parsed.key, SMALL)
end

-- The platform the labels are for: "mac" (every host of the kit today) unless the page said
-- another.
function keys.platform()
	return platform
end

-- Says the platform the labels are for, "mac" or "other": a page that knows better than the kit.
function keys.set_platform(name)
	if not MODIFIERS[name] then
		error('a platform is "mac" or "other", not ' .. tostring(name), 2)
	end
	platform = name
end

return keys
