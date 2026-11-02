-- The page API, part 7: the page itself and its program.
--
-- A script returns the page, a table of the functions the application calls:
--
--   render(size, now, reduce_motion) -> element[, animating[, wake_at]]
--       The page for a viewport `size` ({ w =, h = } points) at time `now` (seconds on the
--       application's clock). `reduce_motion` is the person's setting. `animating` true asks
--       for the next frame at once; a page at rest costs no frames. `wake_at`, a time on the
--       same clock (`now + 5`: in five seconds), asks a page at rest to be rendered again once
--       then, if nothing does before: what a page needs to show that what it waits for has not
--       come (a reading that stopped).
--   settle() -> boolean            (optional)
--       The page was laid out: bring what depends on the layout back within it. True when
--       something changed and the page should be rendered again. (A timer set here is asked for
--       by the kit, which has the page rendered again for it.)
--   handle(event) -> reply         (optional)
--       The person did something. `event` is { type = "move" | "leave" | "press" | "release" |
--       "wheel" | "scrolled" | "key" | "key_up" | "mods" | "text" | "focus" | "menu" | "command" |
--       "files" | "files_over" | "files_left" | "saving" | "terminal_look",
--       ... } in the page's points: move/press/release/wheel/files have x and y (press and
--       release a button, "left", "right" or "middle"; press its `count`, 2 for the second of a
--       double click; press, key, key_up and mods have mods = { shift =, control =, alt =,
--       command = }; wheel dx and dy, the points the content would move left and up by, for a
--       wheel no box the host scrolls took; scrolled the `boxes` the host scrolled, each
--       { id =, x =, y = }, for a page that builds only what is in view), key and
--       key_up have `key` ("up", "down", "left", "right", "page_up", "page_down", "home", "end",
--       "enter", "escape", "tab", "backspace", "delete", "f1" to "f20", or the character), text
--       has the `field` (its name) and the `text` it holds, focus the `field` the person clicked
--       into (nil: the keyboard is the page's again), menu has the `index` (from 1) picked of
--       the menu shown and the `entry` of it, the table the page wrote for that line (the menu
--       shown is not always the one the page asked for last: the application shows one at a time
--       and does not show one asked for while another is up; a pick of a menu that is no longer
--       known is not told), command the `id` of the command asked for (its keys are
--       not told as a key). "files": the person gave the page files — `source` "drop" (on the
--       page, at x and y), "paste" (at the caret) or "pick" (in the Open panel the page asked for;
--       none when the person turned it away) — and `files` lists them, each { handle =, name =,
--       size =, type =, path =, kind = "file" | "folder" }: the page draws one as
--       `pencere.image("file:" .. handle)` and passes the handle on to its program, which reads
--       the file by it; a folder has no handle; `path` only where the host knows the program reads
--       the same files as the person, and `paths` the paths there are. The name is the person's
--       and the type the host's guess: words, never a path. "files_over" (x, y and the `count` of
--       files dragged over the page from outside it) and "files_left" (they went without being
--       dropped) are for showing where they would go. "saving": the person chose a place in the
--       Save panel the page asked for, its `handle` (passed on to the program, which writes the
--       file by it) and the `name` given; neither when the person turned the panel away.
--       An editor (`pencere.editor`) tells: "editor_ready", the `editor` and its `version`, once the
--       host has made it (its text is posted: a page loads a document into it then); move, press
--       and release over it have a `place` ({ editor =, at =, inside = (the place was in a selection, or at a caret, before a press moved the caret), version = }, the place of its document under the
--       pointer); "edit" has the `editor`, the `version` it came to, its `origin` ("person",
--       "compose", "page", "undo", "redo", "load"), the `tag` of a change the page asked with one,
--       and its `changes` ({ { from =, to =, insert = }, ... }, in the places of the version
--       before, in order); "selection" the `editor`, the `version`, its `ranges`
--       ({ { anchor =, head = }, ... }), which is `main` (from 1) and whether an input method is
--       `composing`; "viewport" the `editor` and the `first` and `last` of its lines in view;
--       "editor_refused" the `editor` and the `reason` ("stale-version", "out-of-document", "mid-character", …) it
--       did not do what the page asked; "found", while it searches (after every "find" asked
--       for, and whenever these change; a `count` of 0 when the search stops), the `editor`, the
--       `version` searched, the `count` of matches (at most 10 000), which of them the main
--       selection is exactly on, `current` (from 1, as a find widget shows it; nil for none), and
--       `more` (the search stopped before the end: there may be more); "gutter", in a column of its
--       gutter that hears the pointer, the `editor`, the `column`, the `line` (from 1), the `kind`
--       ("press", "enter", "leave"), the `button`, `count` and modifiers of a press, the `marker`
--       under the pointer (`{ id =, layer =, from =, to = }`; nil where the cell is empty: the page
--       may put one there) and the `cell` ({ x =, y =, w =, h = }, the page's points); "hover", in an
--       editor with `events = { "hover" }`, when the pointer has rested `hover.delay` (0.3) seconds
--       over a word (`kind = "dwell"`: the `editor`, the place `at` under it, the word's `from`
--       and `to`, its `line`, the `hits` ({ { layer =, id =, from =, to = }, ... }: the styles with
--       an `id` that hold `at`), the `word`'s box ({ x =, y =, w =, h = }, the page's points),
--       the modifiers held and the `version` the places are of) and when it goes from a word it was
--       told of (`kind = "leave"`, at once, the same fields; over a widget with a name `over` is "widget" and
--       `id` its, or its part's); "target", in an editor with `events = {
--       "target" }`, the `editor`, `kind` ("enter", "leave": the pointer came to a word with the
--       modifiers of a press that is the page's held, at most once in `links.delay`, or went from
--       it; "press": that press), `source` ("word", "link": a held look with an `id`, whose range it
--       is, or "widget": a hint with a name, pressed with any modifiers), the place `at`, the range `from` and `to`, the link's `id`, the `word`'s box, the modifiers,
--       the `button` and `count` of a press and the `version`; "keymap_pending", the `editor` and the
--       `keys` of a chord waited on so far (none: not any more). Places are bytes of the text, counted
--       from 1 as a string's are (the end of the text is one past its last byte); lines are
--       counted from 1.
--       "wake" has the `now` it was woken at (seconds on `render`'s clock): the time the page asked
--       to be woken at, with a reply's `wake`, has come. Told only to a page that asked, after the
--       timers that were due ran (`pencere.time`).
--   message(message) -> reply      (optional)
--       The program said something that is not an answer to a call the page made or a push of a
--       subscription it made (those go to the functions given to `pencere.call` and
--       `pencere.subscribe`): a response or a push to a number the page did not ask with, or
--       anything else. `message` is the decoded JSON.
--   room() -> reply                (optional)
--       The program has read enough: what `pencere.send` answered false to may be sent again.
--
-- A reply is nil (nothing changed), true or false (a frame is worth drawing or not) or a table:
--   { redraw =, close =, copy = "text for the pasteboard", cursor = "arrow" | "pointer" | "text" |
--     "resize_horizontal" | "resize_vertical" | "not_allowed" | "grab" | "grabbing" | "crosshair",
--     keyboard = "page" | { field = name }, fill = { [name] = "text for that field", or
--     { text =, select = { from =, to = } (bytes from 1) or true (all of it) }, ... },
--     menu = { at = { x =, y = }, entries = { { title =, keys = "ctrl+w", enabled = }, "-" (a line),
--     { title =, keys =, enabled =, paste = editor } (a line the application performs itself: the
--     pasteboard's text pasted in that editor as its paste key would, honouring a read-only editor
--     and several carets; the page is told the pick as any and never reads the pasteboard, nor can
--     it ask an editor to paste), ... } (a line's table is told back with its pick, as `entry`, with
--     whatever else the page put in it) }, editor = { [name] = ask, or { ask, ... } }, link = "https://...", wake = seconds,
--     pick = true or { multiple = true } (the Open panel), save = "name" or { name =, type = } (the
--     Save panel) }
-- A panel is shown only for a reply to something the person did (a press or its release, a key, a
-- pick in a menu, a command): asked for in a reply to anything else, it is passed over; a reply asks
-- for one.
-- where an ask of an editor is one of:
--   { load = "text", more = true }             a piece of the document (the first in place of all)
--   { change = { { from =, to =, insert = }, ... }, base = version, history = false, tag = "disk" }
--     (`history` false: not undone, and what can be undone is carried over it; `tag` is told back
--     in the `edit` of it)
--   { select = { { anchor =, head = }, ... }, version =, main = (from 1) }
--   { reveal = place, version =, align = "nearest" | "start" | "center" | "end" }   scrolled into view
--   { command = "undo" }, { command = "move", movement = "word_left", extend = true },
--   { command = "insert", text = "x" }, { command = "find", pattern =, regex =, case =, word = }
--     (`word`: only a whole word, no letter or digit of any script or `_` beside it; the search
--     goes on as the document changes), "find_next", "find_previous", "stop_finding",
--     { command = "replace", text = }, { command = "replace_all", text = } (the main selection's
--     match, or every match, replaced; a regular expression's `text` takes what its groups hold:
--     "$1", "${1}", "$name", "${name}", "$$" a "$"), "select_found" (every match),
--     "select_occurrences" (every place of the main selection's text, or of the caret's word),
--     "toggle_comment" (the lines' line comment, else their block comment), "toggle_block_comment"
--     (each range in the block comment, or out of it; a caret's range its line's text), "fold_all",
--     "delete_lines", "select_line", "duplicate_lines_up", "join_lines", "insert_blank_line",
--     "split_line", "transpose_chars", "delete_to_line_start", "delete_to_line_end", "copy", "cut"
--     (what is selected put on the application's pasteboard, as the keys do, a caret's line when
--     nothing is selected; "cut" takes it out too. There is no command that pastes)
--   { decorate = "layer", version =, from =, to =, decorations = { d, ... } }, each `d`
--     { from =, to =, color =, background =, weight =, italic =, strike =,
--       underline = colour | { wavy = colour }, id = "name", held = "command", over = true,
--       cursor = "pointer" } (the `id` is told in the `hits` of a "hover" over it and in a "target"
--       press on it; `held` is the modifiers while which alone it is drawn, `mod` the host's main
--       one: a link that shows while ⌘ is held; `over` while the pointer is over its range; `cursor`
--       the pointer's shape there, the hand where it is `held`), or { from =, to =, line = colour },
--     { from =, to =, gutter = colour, column = "marks", shape = "dot" | "bar" | "above" | "below"
--       | "image", image = "assets/x.svg", id =, label =, cursor =, hover = colour, pressed = colour }
--     (a marker in a column of the gutter: a dot by the first line, a bar along every line of the
--     range, a triangle on the edge above or below its first line (where lines were taken away:
--     `from == to` is fine), a tinted picture of the bundle; `label` is what a screen reader says
--     of it and what the host says while the pointer rests on it), { from =, to =, ruler = colour,
--     lane = "full" | "left" | "center" | "right" } (a mark on the overview ruler),
--     { from =, to =, widget = "words" | { "words", { "more", color =, id = } }, side = "before" |
--     "after" | "end", group =, id =, label =, cursor =, anchor =, color =, background =, pad_left =,
--     pad_right =, margin_left =, margin_right = } (text that is not the document's, where the range is: an inlay hint "before" the
--     range's start or "after" its end, or after its line's end; at most 8 parts of 200
--     characters, 32 to a line), { from =, to =, anchor = "name" } (the range has a name an overlay can stand beside,
--     `pencere.range_of(editor, name)`), { from =, to =, zone = "name", height = points, side = "below" | "above" } (a gap kept open after the line the range
--     starts on, for the page's element that is `:zone("name")` of the editor), { from =, to =,
--     fold = true }
-- `link` asks the host to open an address in the person's browser, as a web page's `link` asks:
-- only an `http` or `https` address of printable ASCII without spaces, at most 8192 bytes; the
-- host should ask the person first, drops any other address, and never tells the page what came of
-- it. (It is not the `link` of a span, which only names a part of a text the pointer answers by:
-- a page whose span says `:link("docs")` replies `{ link = url }` when that part is pressed.)
-- `wake` asks the host to wake the page that many seconds from now (0 or more; 0: at the next
-- frame): its `handle` is told { type = "wake", now = } then. The earliest of the times asked
-- stands, and one that came is gone (`pencere.time` has the timers made on it).
--
-- What the page says to its program is JSON objects, messages of a view and its program:
-- `pencere.call` asks the program to do something and answers once, `pencere.subscribe` asks to hear
-- a topic as it changes, `pencere.cancel` gives either up. The program may not have read what it was
-- told before: what is said waits its turn and goes when the program can take it (`pencere.send`,
-- the raw word, says whether it took the message, and the page keeps what it was not).
--
-- `print(...)` and `warn(...)` are the page's two other functions that say something to its program,
-- and are globals like `require` (the script, the scripts it requires and the kit's library have
-- them): each is a `pencere.call` of the reserved method `pencere.log`, with `{ text =, level = }`,
-- that the program's library answers by writing the line to the program's log. The arguments are
-- each made text as `tostring` does and joined with tabs; a line longer than 8192 bytes is cut
-- (not inside a character) and ends with how much more there was. At most 64 lines are out (waiting
-- to be said, or said and not answered) at once: a line printed while 64 are is dropped, and the
-- next one that can be said is said after one line that tells how many were. A program that
-- answers that it has no `pencere.log` (`not-found`) is not told another line, for as long as the
-- page lives.

local internal = ...

local host = internal.host
local json = pencere.json
local time = internal.time
local merge = pencere.merge
local lists = internal.lists
local refuse = internal.refuse

-- A call may say this many messages; the application closes a page that says more.
local PER_CALL = 8

local page = nil
-- The page has been rendered: it has been laid out only after that.
local rendered = false
-- What is waiting to be said, in order: `head` is the next, `tail` the last. Counted, not
-- measured with `#`: the queue is emptied from the front, and the length of a table with holes is
-- any of its borders.
local queue, head, tail = {}, 1, 0
-- The program had no room for the next message: nothing is said until it has (`room`).
local full = false
-- The program has said something.
local heard = false
local next_id = 1
-- What is asked and not yet over, by the number the page gave it.
local asked = {}
-- The lines of `print` and `warn` that are waiting in the queue, by their place in it (the number
-- they were asked by): they are the one kind of message that is let go of when the program has said
-- it has nowhere to write them.
local queue_logs = {}
-- The program answered that it has no `pencere.log`: `print` and `warn` say nothing more.
local logs_off = false

-- Says `message` (a table) to the program now. True when the program took it; false when it
-- has not the room (nothing was sent: keep it, `room` says when to try again).
function pencere.send(message)
	return host.send(json.encode(message))
end

-- Whether the program has said something: a page nobody opened has no one to talk to.
function pencere.heard()
	return heard
end

local function enqueue(message)
	tail += 1
	queue[tail] = json.encode(message)
end

-- Says what is waiting, as far as the program has the room and a call may say. True when there
-- is more that only a later call will say.
local function flush()
	local said = 0
	while head <= tail do
		local log = queue_logs[head]
		if log and logs_off then
			-- Not said: the program answered that it has no log, after the line was put in the queue.
			asked[log] = nil
			queue_logs[head] = nil
			queue[head] = nil
			head += 1
			continue
		end
		if full then
			return false
		end
		if said >= PER_CALL then
			return true
		end
		if host.send(queue[head]) then
			queue_logs[head] = nil
			queue[head] = nil
			head += 1
			said += 1
		else
			full = true
			return false
		end
	end
	queue, head, tail = {}, 1, 0
	return false
end

-- `reply`, asking for the next frame as well: there is more to say than one call may, and a
-- render says it on (and asks for the frame after it while there is more still).
local function with_frame(reply)
	return merge(reply, true)
end

-- Refuses what a request is not, at the line of the script that made it: the name of what is asked
-- is a string, the params nothing or a table (they are the object of the message), and what hears
-- the answer nothing or a function. A request of the wrong shape would otherwise fail later, in the
-- host, with no line to name, or be answered to nobody.
local function check_request(fname, noun, name, params, hearers)
	if type(name) ~= "string" then
		refuse("pencere." .. fname .. " takes the " .. noun .. " to ask for, a string, not a " .. type(name))
	end
	if params ~= nil and type(params) ~= "table" then
		refuse("pencere." .. fname .. " takes params that are nil or a table, not a " .. type(params))
	end
	for _, hearer in hearers do
		if hearer[2] ~= nil and type(hearer[2]) ~= "function" then
			refuse("pencere." .. fname .. " takes " .. hearer[1] .. " that is nil or a function, not a " .. type(hearer[2]))
		end
	end
end

-- Asks the program to do `method` (with `params`, a table) and answer once: `done(result, err)`
-- is called with the result, or with nil and { code =, message = }. `progress(report)` is called
-- with { done =, total =, note = } while a call that takes a while goes on. The number of the call.
function pencere.call(method, params, done, progress)
	check_request("call", "method", method, params, { { "done", done }, { "progress", progress } })
	local id = next_id
	next_id += 1
	asked[id] = { kind = "call", done = done, progress = progress }
	enqueue({ call = { id = id, method = method, params = params } })
	return id
end

-- Asks to hear `topic` as it changes: `on_value(value)` is called for each value the program
-- pushes. If the program refuses the subscription, or ends it with an error, `on_error(err)` is
-- called once ({ code =, message = }, as a call's) and no value follows; without an `on_error` the
-- subscription ends without a word. The number of the subscription.
function pencere.subscribe(topic, params, on_value, on_error)
	check_request("subscribe", "topic", topic, params, { { "on_value", on_value }, { "on_error", on_error } })
	local id = next_id
	next_id += 1
	asked[id] = { kind = "subscribe", on_value = on_value, on_error = on_error }
	enqueue({ subscribe = { id = id, topic = topic, params = params } })
	return id
end

-- Gives up a call that is still out, or a subscription. What the program says of it after this is
-- not told to the page.
function pencere.cancel(id)
	if type(id) ~= "number" or id < 1 or id % 1 ~= 0 then
		refuse(
			"pencere.cancel takes the number a call or a subscription was given, not "
				.. (type(id) == "number" and tostring(id) or "a " .. type(id))
		)
	end
	asked[id] = nil
	enqueue({ cancel = { id = id } })
end

-- --- print and warn ------------------------------------------------------------------------------

-- A line is cut at this many bytes.
local LOG_LINE = 8192
-- This many lines may be out at once: waiting in the queue, or said and not answered. What the page
-- remembers of its lines (the entries of `asked`, the queue) is held to it.
local LOG_OUT = 64
local logs_out = 0
-- How many lines were let go of for want of room, and are not yet told.
local logs_dropped = 0
-- How many of the lines still waiting are said when a call has failed (`entry.last_words`): the last ones.
local LOG_LAST = 16

-- The answer to a line: it is no longer out; and a program that has no log is not told more.
local function logged(_, err)
	logs_out -= 1
	if type(err) == "table" and err.code == "not-found" then
		logs_off = true
	end
end

-- Puts a line in the queue as a call of `pencere.log`, marked as the prelude's own: its answer
-- is for `logged` and never for the page's `message`.
local function say_log(level, text)
	local id = next_id
	next_id += 1
	asked[id] = { kind = "call", log = true, done = logged }
	logs_out += 1
	enqueue({ call = { id = id, method = "pencere.log", params = { text = text, level = level } } })
	queue_logs[tail] = id
end

-- The arguments as one line: each as `tostring` makes it, joined with tabs. A line too long is cut
-- before the character it would end in the middle of (a byte of 0x80 to 0xBF continues the
-- character before it), and says how much was left out.
local function log_line(...)
	local given = table.pack(...)
	local parts = table.create(given.n)
	for index = 1, given.n do
		parts[index] = tostring(given[index])
	end
	local text = table.concat(parts, "\t")
	local length = #text
	if length <= LOG_LINE then
		return text
	end
	local at = LOG_LINE
	for _ = 1, 3 do
		local byte = string.byte(text, at + 1)
		if byte < 0x80 or byte >= 0xC0 then
			break
		end
		at -= 1
	end
	return string.sub(text, 1, at) .. "… (" .. (length - at) .. " more bytes)"
end

local function log(level, ...)
	if logs_off then
		return
	end
	-- Made first: what the arguments say as text may fail, and a line that fails is not counted.
	local text = log_line(...)
	local told = logs_dropped > 0
	if logs_out + (told and 2 or 1) > LOG_OUT then
		logs_dropped += 1
		return
	end
	if told then
		say_log("warn", logs_dropped .. " lines of print were dropped")
		logs_dropped = 0
	end
	say_log(level, text)
end

-- The page's `print` and `warn` say to the program's log, as the program's own would: in the
-- global table of the prelude, so that the script, the scripts it requires and the library have
-- them, each in the table of globals it has of its own over this one.
function print(...)
	log("info", ...)
end

function warn(...)
	log("warn", ...)
end

-- --- The menus the page asked for ---------------------------------------------------------------

-- The application shows one menu at a time: one a reply asks for while another is up is not shown,
-- and the page is not told. A pick comes back as the index of the menu the person saw, which is not
-- always the menu the page asked for last, so the page must not read it against its latest. Each menu
-- that goes out in a reply is given a number, and the entries it asked for are kept by it, the last
-- MENUS_KEPT menus; the application hands the number back with the pick, and the page is told the
-- pick with the entry of that menu (`event.entry`), the table it wrote with its fields and its
-- functions. A pick of a number not kept, of a menu already picked from, or of a line that is no
-- entry is not told the page.
local MENUS_KEPT = 16
local menus = {}
local menus_asked = 0

-- The reply as it goes to the host, its menu (if it asks for one) with its number. The page's own
-- tables are not changed: the reply and the menu are copies, and so is the list of entries kept.
local function numbered(reply)
	if type(reply) ~= "table" or type(reply.menu) ~= "table" then
		return reply
	end
	menus_asked = menus_asked + 1
	local entries = reply.menu.entries
	local kept = {}
	if type(entries) == "table" then
		table.move(entries, 1, #entries, 1, kept)
	end
	menus[(menus_asked - 1) % MENUS_KEPT + 1] = { serial = menus_asked, entries = kept }
	local menu = table.clone(reply.menu)
	menu.serial = menus_asked
	local out = table.clone(reply)
	out.menu = menu
	return out
end

-- The pick `event` (`serial` and `index` as the application gave them) as the page is told it: with
-- the `entry` of the menu it was a pick of, and without the number. False where there is no such
-- entry: a menu is picked from once.
local function picked(event)
	local serial = event.serial
	event.serial = nil
	if type(serial) ~= "number" then
		return false
	end
	local at = (serial - 1) % MENUS_KEPT + 1
	local menu = menus[at]
	if not menu or menu.serial ~= serial then
		return false
	end
	local entry = menu.entries[event.index]
	if type(entry) ~= "table" then
		return false
	end
	menus[at] = false
	event.entry = entry
	return true
end

-- The end of a call of the application's: what the page's reply comes to (`time.finish`), with the
-- number of its menu.
local function finish(reply)
	return numbered(time.finish(reply))
end

-- --- What the application calls ----------------------------------------------------------------

-- The functions below are the application's entry points, called by the interpreter's bridge,
-- which takes them from `internal.entry` when the prelude has run. They are not in `pencere`: a
-- script that could call them could answer for the program, or start another page.
local entry = {}
internal.entry = entry

function entry.start(returned)
	if type(returned) ~= "table" or type(returned.render) ~= "function" then
		error("the script must return a page: a table with a render function", 0)
	end
	page = returned
end

function entry.render(width, height, now, reduce_motion)
	time.given(now)
	lists.begin(height)
	local root, animating, wake_at = page.render({ w = width, h = height }, now, reduce_motion)
	lists.finish()
	-- What render handed back is wrong in these three: no line of the script is on the stack to
	-- name (the application called, and the script's function has returned), so they say their
	-- words as they are.
	if type(root) ~= "table" then
		error("render must return an element, not a " .. type(root), 0)
	end
	if wake_at ~= nil and type(wake_at) ~= "number" then
		error("render's third value, when to render again, must be a time (a number), not a " .. type(wake_at), 0)
	end
	rendered = true
	local more = flush()
	-- A timer set as the page was drawn (or as the script started) has no reply to be asked in.
	return root, (animating or more) and true or false, wake_at, time.asked()
end

function entry.settle()
	-- Before the first page there is no layout to settle to: the application asks once as the
	-- script starts, and that is a call that only starts it.
	if not rendered then
		return false
	end
	-- A long list whose window the rows made no longer fill is built again, whatever the page says.
	local long = lists.settle()
	local settled = page.settle and page.settle() or false
	-- A timer set as the page settled has no reply to be asked in: the page is rendered again, and
	-- its frame asks.
	local timed = time.owed()
	return (long or settled or timed) and true or false
end

function entry.handle(event)
	local reply = nil
	local long = false
	if event.type == "menu" and not picked(event) then
		return finish(nil)
	end
	if event.type == "wake" then
		-- The timers that are due, then the page where it asked a wake itself.
		reply = time.woken(event.now, page.handle)
	else
		if event.type == "scrolled" then
			internal.heard(event.boxes)
			long = lists.scrolled(event.boxes)
		end
		if page.handle then
			reply = time.taken(page.handle(event))
		end
	end
	-- A long list scrolled near the end of the rows made is built again, whatever the page says.
	if flush() or long then
		reply = with_frame(reply)
	end
	return finish(reply)
end

-- A message of the program: the answer to what the page asked is for the one who asked, the rest
-- for the page.
function entry.message(text)
	heard = true
	local message = json.decode(text)
	if type(message) ~= "table" then
		return finish(nil)
	end
	local reply = nil
	local taken = false
	-- What the page asked is found by the number it gave and by what it asked: an answer is for a
	-- call, a push for a subscription.
	local body = message.push or message.response or message.progress
	local number = type(body) == "table" and type(body.id) == "number" and body.id or nil
	local request = number and asked[number]
	if message.push then
		if request and request.kind == "subscribe" then
			taken = true
			if request.on_value then
				reply = time.taken(request.on_value(body.value))
			end
		end
	elseif message.response then
		if request and request.kind == "call" then
			taken = true
			asked[number] = nil
			if request.done then
				reply = time.taken(request.done(body.result, body.error))
			end
		elseif request and request.kind == "subscribe" and body.error ~= nil then
			-- The program ended the subscription with an error: it is over, and told once.
			taken = true
			asked[number] = nil
			if request.on_error then
				reply = time.taken(request.on_error(body.error))
			end
		end
	elseif message.progress then
		if request and request.kind == "call" then
			taken = true
			if request.progress then
				reply = time.taken(request.progress(body))
			end
		end
	end
	-- What is left with a number the kit gave out is not the page's: it comes after the page gave a
	-- request up, or after a subscription ended, or it is what the protocol does not have (a push to
	-- a call, a value in the answer to a subscribe). `message` has the numbers the kit did not give,
	-- which a page that numbers its own messages (`pencere.send`) takes from beyond these.
	if not taken and number and number >= 1 and number % 1 == 0 and number < next_id then
		taken = true
	end
	if not taken and page.message then
		reply = time.taken(page.message(message))
	end
	if flush() then
		reply = with_frame(reply)
	end
	return finish(reply)
end

-- The bridge calls this when a call of the page has failed, before the page is closed: the lines of
-- `print` and `warn` that waited for the end of the call are said now, the last LOG_LAST of them (the
-- ones nearest the failure), so that a page that printed and then failed is not a page that said
-- nothing. Nothing else that waits is said: what the page asked of its program in a call that failed
-- is not asked.
function entry.last_words()
	if logs_off then
		return
	end
	local lines = {}
	for at = head, tail do
		if queue_logs[at] then
			table.insert(lines, queue[at])
		end
	end
	for index = math.max(1, #lines - LOG_LAST + 1), #lines do
		if not host.send(lines[index]) then
			return
		end
	end
end

function entry.room()
	full = false
	flush()
	local reply = nil
	if page.room then
		reply = time.taken(page.room())
	end
	if flush() then
		reply = with_frame(reply)
	end
	return finish(reply)
end

-- --- What the script is given is not changed under it --------------------------------------------

-- Every table the prelude made public (`pencere.json`, `pencere.layout`, `pencere.time`, the
-- classes of the elements, of the timers and of the rest, with the metatables they and their
-- members have) is frozen now that all of it is made: one page, or one module of the library,
-- cannot change what another is given. The tables the page makes with them are the page's own.
local function lock(value, seen)
	if type(value) ~= "table" or seen[value] then
		return
	end
	seen[value] = true
	for _, member in value do
		lock(member, seen)
	end
	lock(getmetatable(value), seen)
	table.freeze(value)
end

local seen = {}
lock(pencere, seen)
for _, class in internal.classes do
	lock(class, seen)
end
