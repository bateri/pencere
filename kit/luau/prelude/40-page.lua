-- The page API, part 5: the page itself and its program.
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
--       something changed and the page should be rendered again.
--   handle(event) -> reply         (optional)
--       The person did something. `event` is { type = "move" | "leave" | "press" | "release" |
--       "scroll" | "scrolled" | "key" | "key_up" | "mods" | "text" | "focus" | "menu" | "command" |
--       "files",
--       ... } in the page's points: move/press/release/scroll/files have x and y (press and
--       release a button, "left", "right" or "middle"; press its `count`, 2 for the second of a
--       double click; press, key, key_up and mods have mods = { shift =, control =, alt =,
--       command = }; scroll dx and dy, the points the content would move left and up by, for a
--       wheel no box the host scrolls took; scrolled the `boxes` the host scrolled, each
--       { id =, x =, y = }, for a page that builds only what is in view), key and
--       key_up have `key` ("up", "down", "left", "right", "page_up", "page_down", "home", "end",
--       "enter", "escape", "tab", "backspace", "delete", "f1" to "f20", or the character), text
--       has the `field` (its name) and the `text` it holds, focus the `field` the person clicked
--       into (nil: the keyboard is the page's again), menu has the `index` (from 1) picked of
--       the menu the page asked for, command the `id` of the command asked for (its keys are
--       not told as a key), files the `paths` of the files dropped on the page.
--       An editor (`pencere.editor`) tells: move, press and release over it have a `place`
--       ({ editor =, at = }, the place of its document under the pointer); "edit" has the `editor`,
--       the `version` it came to, its `origin` ("person", "compose", "page", "undo", "redo",
--       "load") and its `changes` ({ { from =, to =, insert = }, ... }, in the places of the
--       version before, in order); "selection" the `editor`, the `version`, its `ranges`
--       ({ { anchor =, head = }, ... }), which is `main` (from 1) and whether an input method is
--       `composing`; "viewport" the `editor` and the `first` and `last` of its lines in view;
--       "editor_refused" the `editor` and the `reason` ("stale-version", "out-of-document", …) it
--       did not do what the page asked; "found", while it searches (after every "find" asked
--       for, and whenever these change; a `count` of 0 when the search stops), the `editor`, the
--       `version` searched, the `count` of matches (at most 10 000), which of them the main
--       selection is exactly on, `current` (from 1, as a find widget shows it; nil for none), and
--       `more` (the search stopped before the end: there may be more). Places are characters
--       from 0, lines from 0.
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
--     keyboard = "page" | { field = name }, fill = { [name] = "text for that field", ... },
--     menu = { at = { x =, y = }, entries = { { title =, key =, enabled = }, "-" (a line), ... } },
--     editor = { [name] = ask, or { ask, ... } } }
-- where an ask of an editor is one of:
--   { load = "text", more = true }             a piece of the document (the first in place of all)
--   { change = { { from =, to =, insert = }, ... }, base = version }
--   { select = { { anchor =, head = }, ... }, version =, main = (from 1) }
--   { reveal = place, version = }              scrolled into view
--   { command = "undo" }, { command = "move", movement = "word_left", extend = true },
--   { command = "insert", text = "x" }, { command = "find", pattern =, regex =, case =, word = }
--     (`word`: only a whole word, no letter or digit of any script or `_` beside it; the search
--     goes on as the document changes), "find_next", "find_previous", "stop_finding",
--     { command = "replace", text = }, { command = "replace_all", text = } (the main selection's
--     match, or every match, replaced; a regular expression's `text` takes what its groups hold:
--     "$1", "${1}", "$name", "${name}", "$$" a "$"), "select_found" (every match),
--     "select_occurrences" (every place of the main selection's text, or of the caret's word), …
--   { decorate = "layer", version =, from =, to =, decorations = { d, ... } }, each `d`
--     { from =, to =, color =, background =, weight =, italic =, strike =,
--       underline = colour | { wavy = colour } }, or { from =, to =, line = colour },
--     { from =, to =, gutter = colour }, { from =, to =, fold = true }
--
-- What the page says to its program is JSON objects, the six messages of a view and its program:
-- `pencere.call` asks the program to do something and answers once, `pencere.subscribe` asks to hear
-- a topic as it changes, `pencere.cancel` gives either up. The program may not have read what it was
-- told before: what is said waits its turn and goes when the program can take it (`pencere.send`,
-- the raw word, says whether it took the message, and the page keeps what it was not).

local host = pencere.host
local json = pencere.json

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
		if full then
			return false
		end
		if said >= PER_CALL then
			return true
		end
		if host.send(queue[head]) then
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
	if type(reply) == "table" then
		reply.redraw = true
		return reply
	end
	return true
end

-- Asks the program to do `method` (with `params`, a value) and answer once: `done(result, err)`
-- is called with the result, or with nil and { code =, message = }. `progress(report)` is called
-- with { done =, total =, note = } while a call that takes a while goes on. The number of the call.
function pencere.call(method, params, done, progress)
	local id = next_id
	next_id += 1
	asked[id] = { kind = "call", done = done, progress = progress }
	enqueue({ call = { id = id, method = method, params = params } })
	return id
end

-- Asks to hear `topic` as it changes: `on_value(value)` is called for each value the program
-- pushes. The number of the subscription.
function pencere.subscribe(topic, params, on_value)
	local id = next_id
	next_id += 1
	asked[id] = { kind = "subscribe", on_value = on_value }
	enqueue({ subscribe = { id = id, topic = topic, params = params } })
	return id
end

-- Gives up a call that is still out, or a subscription.
function pencere.cancel(id)
	asked[id] = nil
	enqueue({ cancel = { id = id } })
end

-- --- What the application calls ----------------------------------------------------------------

-- The functions below are the application's entry points (`pencere.start` .. `pencere.room`),
-- called by the interpreter's bridge; the script does not call them.

function pencere.start(returned)
	if type(returned) ~= "table" or type(returned.render) ~= "function" then
		error("the script must return a page: a table with a render function", 0)
	end
	page = returned
end

function pencere.render(width, height, now, reduce_motion)
	pencere._lists.begin(height)
	local root, animating, wake_at = page.render({ w = width, h = height }, now, reduce_motion)
	pencere._lists.finish()
	if type(root) ~= "table" then
		error("render must return an element, not a " .. type(root), 0)
	end
	if wake_at ~= nil and type(wake_at) ~= "number" then
		error("render's third value, when to render again, must be a time (a number), not a " .. type(wake_at), 0)
	end
	rendered = true
	local more = flush()
	return root, (animating or more) and true or false, wake_at
end

function pencere.settle()
	-- Before the first page there is no layout to settle to: the application asks once as the
	-- script starts, and that is a call that only starts it.
	if not rendered then
		return false
	end
	-- A long list whose window the rows made no longer fill is built again, whatever the page says.
	local lists = pencere._lists.settle()
	local settled = page.settle and page.settle() or false
	return (lists or settled) and true or false
end

function pencere.handle(event)
	local reply = nil
	local lists = false
	if event.type == "scrolled" then
		pencere.layout._heard(event.boxes)
		lists = pencere._lists.scrolled(event.boxes)
	end
	if page.handle then
		reply = page.handle(event)
	end
	-- A long list scrolled near the end of the rows made is built again, whatever the page says.
	if flush() or lists then
		reply = with_frame(reply)
	end
	return reply
end

-- A message of the program: the answer to what the page asked is for the one who asked, the rest
-- for the page.
function pencere.message(text)
	heard = true
	local message = json.decode(text)
	if type(message) ~= "table" then
		return nil
	end
	local reply = nil
	local taken = false
	-- What the page asked is found by the number it gave and by what it asked: an answer is for a
	-- call, a push for a subscription.
	local function asked_as(body, kind)
		local request = type(body) == "table" and type(body.id) == "number" and asked[body.id]
		if request and request.kind == kind then
			return request
		end
		return nil
	end
	if message.push then
		local request = asked_as(message.push, "subscribe")
		if request then
			taken = true
			if request.on_value then
				reply = request.on_value(message.push.value)
			end
		end
	elseif message.response then
		local request = asked_as(message.response, "call")
		if request then
			taken = true
			asked[message.response.id] = nil
			if request.done then
				reply = request.done(message.response.result, message.response.error)
			end
		end
	elseif message.progress then
		local request = asked_as(message.progress, "call")
		if request then
			taken = true
			if request.progress then
				reply = request.progress(message.progress)
			end
		end
	end
	if not taken and page.message then
		reply = page.message(message)
	end
	if flush() then
		reply = with_frame(reply)
	end
	return reply
end

function pencere.room()
	full = false
	flush()
	local reply = nil
	if page.room then
		reply = page.room()
	end
	if flush() then
		reply = with_frame(reply)
	end
	return reply
end
