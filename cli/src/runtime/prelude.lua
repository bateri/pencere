-- The part of the runtime that is written in Lua: tasks, and the calls that wait for something
-- without holding the loop (`serve`, `time.sleep`, `process.run`).
--
-- Everything the app runs is a task: a coroutine. The app itself, each call of the view, each
-- callback of a timer or a process. A call that waits (`process.run`) suspends the task and
-- the loop goes on with the others; what it waits for resumes it.
--
-- The chunk is given `native`, the functions the runner made in Rust, and returns the module that
-- `require("pencere")` is, and the function that starts the app.

local native = ...

local module = {}

-- ---- tasks --------------------------------------------------------------------------------

-- How each task ends, by coroutine: { done = function(result), fail = function(error) }.
local tasks = setmetatable({}, { __mode = "k" })

local function resume(co, ...)
	local ok, result = coroutine.resume(co, ...)
	if coroutine.status(co) ~= "dead" then
		return
	end
	local handlers = tasks[co]
	tasks[co] = nil
	if ok then
		if handlers and handlers.done then
			handlers.done(result)
		end
		return
	end
	-- A table is an answer the handler meant to give (`error({ code = "not-found" })`); anything
	-- else is a fault, and the log has where it was.
	if type(result) ~= "table" then
		native.report(debug.traceback(co, tostring(result)))
	end
	if handlers and handlers.fail then
		handlers.fail(result)
	end
end

-- Runs `fn(...)` as a task. Called by the runner, for everything it starts.
local function task(fn, handlers, ...)
	local co = coroutine.create(fn)
	tasks[co] = handlers
	resume(co, ...)
	return co
end

-- Suspends the task that calls this until the function it is given (`done`) is called, from
-- wherever; returns what `done` was called with. `register` is called at once.
local function suspend(register)
	if not coroutine.isyieldable() then
		error("this waits for something, so it runs in the app, in a call of the view or in a callback", 3)
	end
	local co = coroutine.running()
	local waiting, used, early = false, false, nil
	register(function(...)
		if used then
			return
		end
		used = true
		if waiting then
			resume(co, ...)
		else
			early = table.pack(...)
		end
	end)
	if early then
		return table.unpack(early, 1, early.n)
	end
	waiting = true
	return coroutine.yield()
end

native.set_task(task)

-- ---- words --------------------------------------------------------------------------------

local function say(...)
	local count = select("#", ...)
	local parts = table.create and table.create(count) or {}
	for index = 1, count do
		parts[index] = tostring((select(index, ...)))
	end
	native.print(table.concat(parts, "\t"))
end

_G.print = say
_G.warn = function(...)
	say("warning:", ...)
end

-- ---- modules ------------------------------------------------------------------------------

local loaded = {}

-- `require("pencere")` is this module; any other name is a script of the app: "lib/util" or
-- "lib.util" is lib/util.lua (or lib/util/init.lua).
local function require_module(name)
	if name == "pencere" then
		return module
	end
	if type(name) ~= "string" then
		error("require takes the name of a module", 2)
	end
	-- A name the one who runs the app reserved (the runner of an app's tests has one) is found
	-- before the app's own files, which cannot take it.
	local reserved = native.modules[name]
	if reserved ~= nil then
		return reserved
	end
	local path = string.gsub(name, "%.", "/")
	if loaded[path] ~= nil then
		return loaded[path]
	end
	local found = path .. ".lua"
	local source = native.app_read(found)
	if source == nil then
		found = path .. "/init.lua"
		source = native.app_read(found)
	end
	if source == nil then
		error("no module " .. name .. " in the app (looked for " .. path .. ".lua and " .. path .. "/init.lua)", 2)
	end
	loaded[path] = false
	local chunk = native.load(source, found)
	local value = chunk(name)
	if value == nil then
		value = true
	end
	loaded[path] = value
	return value
end

_G.require = require_module

-- ---- the module ---------------------------------------------------------------------------

-- An error raised in Rust arrives with the library's "runtime error: " in front of the words;
-- the functions of the module are wrapped so that the app reads the words, and the place it called
-- from.
local function guarded(fn)
	return function(...)
		local results = table.pack(pcall(fn, ...))
		if not results[1] then
			local err = results[2]
			if type(err) ~= "table" then
				-- The library adds its own traceback after the words, and the app has its own.
				err = string.gsub(string.gsub(tostring(err), "\nstack traceback:.*$", ""), "^runtime error: ", "")
			end
			error(err, 2)
		end
		return table.unpack(results, 2, results.n)
	end
end

local function guard(functions)
	local wrapped = {}
	for name, value in pairs(functions) do
		wrapped[name] = type(value) == "function" and guarded(value) or value
	end
	return wrapped
end

module.fs = guard(native.fs)
-- fs.watch(directory, function(changes) end[, { recursive = true }]) calls the function with the
-- list of files that changed under the directory, `{ path =, kind =, dir = }` each (kind is
-- "created", "modified", "removed", "renamed" or "other"), and gives a handle with `:stop()`.
-- The system tells; nothing is polled.
module.fs.watch = guarded(native.watch)
module.path = guard(native.path)
module.env = guard(native.env)
module.json = guard(native.json)
module.hash = guard(native.hash)
module.os = guard(native.os)
module.io = guard(native.io)

-- io.read_stdin(on_data[, on_end]) reads standard input as it arrives, without holding the app:
-- `on_data(bytes)` is given each piece (up to 64 KiB, exactly as written: no line is cut, no end is
-- taken off) in order, and `on_end()` once, when the input ends. It gives a reading with `:stop()`
-- (no more is read, `on_end` is not called, and what writes the input is told it has no reader),
-- `:active()` and `:wait()`, which waits until the input has ended or was stopped while the app's
-- other work goes on. One reading at a time.
do
	local start = guarded(native.read_stdin)
	module.io.read_stdin = function(on_data, on_end)
		if type(on_data) ~= "function" then
			error("io.read_stdin takes a function that is given each piece of the input", 2)
		end
		local over, waiting = false, {}
		local function ended()
			over = true
			local list = waiting
			waiting = {}
			for _, done in ipairs(list) do
				done()
			end
		end
		local reading = start(on_data, function()
			if on_end then
				on_end()
			end
			ended()
		end)
		return {
			stop = function()
				local was = reading:stop()
				ended()
				return was
			end,
			active = function()
				return reading:active()
			end,
			wait = function()
				if over then
					return
				end
				suspend(function(done)
					waiting[#waiting + 1] = done
				end)
			end,
		}
	end
end
module.text = guard(native.text)
module.regex = guard(native.regex)
module.args = native.args
module.name = native.name

-- app.read(path) reads a file the app came with, by its path in the packed app ("assets/icon.png"
-- is src/assets/icon.png): from the directory the app is run from, inside the executable when it
-- was built. fs.read(path) reads the machine's.
module.app = {
	read = function(path)
		local source = native.app_read(path)
		if source == nil then
			error("app.read: " .. tostring(path) .. ": not in the app", 2)
		end
		return source
	end,
	exists = native.app_has,
	list = guarded(native.app_list),
}

module.time = {
	now = native.now,
	monotonic = native.monotonic,
	after = guarded(native.after),
	every = guarded(native.every),
	-- Waits, and the app goes on with other things meanwhile.
	sleep = function(seconds)
		suspend(function(done)
			native.after(seconds, done)
		end)
	end,
}

local spawn = guarded(native.spawn)

module.process = {
	spawn = spawn,
	-- run{ argv = {...} } (or run{"docker", "ps"}) waits for the program to end, and gives what it
	-- said: { code =, signal =, ok =, timed_out =, overflowed =, stdout =, stderr = } as text;
	-- `timeout` (seconds) ends it if it takes longer. Lines are also given to
	-- `on_stdout` and `on_stderr` while it runs, if the options have them. `raw = true` is for
	-- output that is not text (names ended by NUL): `stdout` and `stderr` are then the bytes as the
	-- program wrote them, no line ends taken out or put in. `max_output` (bytes, both streams
	-- together, line ends included) ends the program that says more, and `overflowed` says it did;
	-- what it said up to the limit is in `stdout` and `stderr`, and nothing after it is kept. The
	-- program and everything it started (a shell's pipeline) are ended together.
	run = function(options)
		if type(options) ~= "table" then
			error("process.run takes a table: { argv = {...} } or the argv itself", 2)
		end
		local spec = {}
		for key, value in pairs(options) do
			spec[key] = value
		end
		if spec.argv == nil then
			spec.argv = options
		end
		local out, err = {}, {}
		local timer, timed_out = nil, false
		local child, overflowed = nil, false
		local function collect(into, forward)
			return function(line)
				into[#into + 1] = line
				if forward then
					forward(line)
				end
			end
		end
		spec.on_stdout = collect(out, options.on_stdout)
		spec.on_stderr = collect(err, options.on_stderr)
		local code, signal = suspend(function(done)
			spec.on_exit = function(code, signal, over)
				overflowed = over == true
				if options.on_exit then
					options.on_exit(code, signal)
				end
				done(code, signal)
			end
			child = spawn(spec)
			-- `timeout` (seconds) ends it if it takes longer.
			if options.timeout then
				timer = native.after(options.timeout, function()
					timed_out = true
					child:kill("KILL")
				end)
			end
		end)
		if timer then
			timer:cancel()
		end
		local joint = options.raw and "" or "\n"
		return {
			code = code,
			signal = signal,
			timed_out = timed_out,
			overflowed = overflowed,
			ok = code == 0 and not overflowed,
			stdout = table.concat(out, joint),
			stderr = table.concat(err, joint),
		}
	end,
}

-- serve{ page =, web =, views =, assets =, access =, title =, program =, icon =, icons =, calls =,
-- topics =, fallback = } shows the view and waits until it is over. Told no `page`, `web` or `views`
-- it shows what the app has in view/ (page.lua, index.html). What it gives says how:
--   { shown = true, ended = "user" | "view" | "program" | "error" | "signal" | "terminal",
--     signal =, code =, message =, status = }
--   { shown = false, reason = "no-tty" | "no-host" | "no-common-version" | "no-common-view" |
--     "refused" | "failed" | "timeout" | "interrupted", code =, message =, status = }
-- A terminal that has no view to show is not an error: `fallback(outcome)` runs, once the terminal is
-- the shell's again, and is where the app shows its text.
function module.serve(options)
	if type(options) ~= "table" then
		error("serve takes a table: { calls = {...}, topics = {...} }", 2)
	end
	local start = guarded(native.serve)
	local outcome, failure = suspend(function(done)
		start(options, done)
	end)
	if failure then
		error("serve: " .. tostring(failure), 2)
	end
	-- What the person must read, now that the terminal is theirs again.
	local failed = outcome.ended == "error" or outcome.reason == "failed" or outcome.reason == "refused"
	if failed and outcome.message then
		native.eprint(string.format("%s: the view ended: %s%s", module.name, outcome.code and (outcome.code .. ": ") or "", outcome.message))
	end
	if not outcome.shown and outcome.reason ~= "interrupted" and options.fallback then
		options.fallback(outcome)
	end
	return outcome
end

-- close_view() ends the view the app is showing from the app's side, as the app's end does: the terminal
-- takes the view down and gives the pane back, and the `serve` that showed it returns (`ended` is
-- "program", or "signal" from a terminal that cannot be asked to). The app goes on, with the pane its
-- own again: an app that has chosen something for the pipeline it is in writes it after this. Whether
-- there was a view to close. Ask for it after the answer of the call that decided it has gone
-- (`time.after(0, ...)`), or the page's own answer is lost with the view.
module.close_view = guarded(native.close_view)

-- handoff{ "zsh", "-l" } (or { argv =, cwd =, env = }) gives the terminal to a program, and takes it
-- back when it ends: the view is closed and the terminal given back, the program runs with it in the
-- foreground, as a shell runs a command, and then the same `serve` shows its view again. Gives
-- `{ code =, signal = }`. Needs a terminal that can close a view for the program; with no view up the
-- program runs all the same. As for close_view, ask for it after the answer of the call that decided
-- it has gone (`time.after(0, ...)`), or the page's own answer is lost with the view.
module.handoff = guarded(native.handoff)

-- ---- files beside the view's bundle ------------------------------------------------------------

-- Waits for a file being moved with the view, and the app goes on with other things meanwhile; a
-- failure is raised as `{ code =, message = }`, which a call of the view answers the view with.
local function moved(start)
	local result, failure = suspend(start)
	if failure then
		error(failure, 0)
	end
	return result
end

-- files: what a view of `lua` 2.1 (a terminal that has files beside the bundle) moves with its app.
-- Each waits, as process.run does, and so runs in the app, in a call of the view or in a callback,
-- once the view is up: before the terminal took the view's open they fail with `unsupported`, as
-- they do on a terminal that opens only `lua` 2.0. A failure is `{ code =, message = }`: the
-- terminal's code (`limit`, `gone`, `order`, …), `ended` when the view ended first, `io` when a file
-- of this machine could not be read or written.
--
--   files.attach(path) and files.attach_data(bytes) give the view a file to draw, read once, and
--   return the name it draws it by ("sha256:..."), once the terminal holds it; a file the view has
--   already, or the terminal still holds, is not sent again. A view shown again needs them again.
--   files.detach(name) lets one go.
--   files.receive(handle, to, { progress = function(bytes), path =, size = }) receives the file the
--   person gave the view (the `handle` its page passed on) into `to`, and returns its size. It is
--   written beside `to` as ".<name>.pencere-part" and given its name at the end; `progress`
--   returning false gives it up, and a part that is not finished is deleted. Make `to` of
--   files.safe_name(name): the name the person gave, with nothing a path could be made of. Where the
--   terminal handed the page the file's `path` (a program on the person's machine) and the page
--   passes it on with the `size`, the file is copied from there and no byte of it crosses the
--   terminal; a path that cannot be read, or a file that is no longer that size, is received by
--   the handle.
--   files.send(handle, from, { progress = function(bytes) }) writes the file `from` to the place
--   the person chose in the Save panel, and returns its size once the terminal has saved it; its
--   `progress` is told and cannot stop it (a save the view leaves unfinished is thrown away).
module.files = {
	attach = function(path)
		return moved(function(done)
			native.file_attach("path", path, done)
		end)
	end,
	attach_data = function(bytes)
		return moved(function(done)
			native.file_attach("data", bytes, done)
		end)
	end,
	detach = guarded(native.file_detach),
	receive = function(handle, to, options)
		return moved(function(done)
			native.file_receive(
				handle,
				to,
				options and options.progress,
				options and options.path,
				options and options.size,
				done
			)
		end)
	end,
	send = function(handle, from, options)
		return moved(function(done)
			native.file_send(handle, from, options and options.progress, done)
		end)
	end,
	safe_name = guarded(native.file_safe_name),
}

-- Ends the app now, with this status (0 unless told).
function module.exit(code)
	native.exit(code or 0)
	-- Nothing after this runs: the loop has been told, and this task waits for ever.
	suspend(function() end)
end

-- ---- the start ----------------------------------------------------------------------------

-- Runs the app's script.
return function(main)
	task(main, {
		done = function()
			native.finish()
		end,
		fail = function(err)
			if type(err) == "table" then
				err = tostring(err.code) .. (err.message and (": " .. tostring(err.message)) or "")
			end
			native.fail(tostring(err))
		end,
	})
end
