# pencere

The command that makes apps for [pencere](https://github.com/bateri/pencere): programs that show a
view in the pane of the terminal they run in. An app is a Lua program and a page, and `pencere
build` makes one executable of it that runs anywhere its kind of machine does.

## Install

```sh
curl -fsSL https://raw.githubusercontent.com/bateri/pencere/main/cli/install.sh | sh
```

It puts the newest release of the command in `~/.local/bin` (no `sudo`) after checking the file
against the `SHA256SUMS` of that release, and tells you if the directory is not on your `PATH`.
`PENCERE_INSTALL=<dir>` installs elsewhere and `PENCERE_VERSION=<x.y.z>` a release other than the
newest. The files are those of the releases `cli-v<version>` of the repository
([releases](https://github.com/bateri/pencere/releases)), one for each of macOS and Linux on arm64
and x86_64, named `pencere-<version>-<target>`.

With Rust, `cargo install --git https://github.com/bateri/pencere pencere-cli --locked` builds it
from the source (the program is called `pencere`).

`pencere upgrade` replaces the command with the newest release later (`--check` only looks), checked
the same way; one that Cargo installed is replaced by Cargo (`cargo install … --force`).

A file downloaded with a browser is quarantined by macOS, which will not run it:
`xattr -d com.apple.quarantine <file>` lifts that. A file `curl` fetches, which is what the line
above does, is not.

## Use

```sh
pencere new hello          # a project: src/program/, src/view/, src/assets/
cd hello
pencere dev                # run it in this pane; a change to a file under src/ runs it again
pencere build              # dist/hello, one file
pencere build --target x86_64-unknown-linux-musl     # the same app for a Linux machine
pencere test               # its tests: the page under a host with no screen (see Testing an app)
```

The same two commands work on a program written in Rust (`pencere new hello --lang rust`, a Cargo
package): `pencere dev` builds it with Cargo and runs it in the pane, shows the view again without
ending the program when the page or a picture changes, and builds and runs it again when anything
else does; `pencere build` builds it with `cargo build --release` and copies it to `dist/`. The page
is the same page of the kit, in `src/view/`, carried into the program by its build script.

## Where things go

```
hello/
  src/
    program/    the program: app.lua, and the modules it requires
    view/       the page: page.lua (a page of the kit) or index.html and its scripts (a web page)
    assets/     the pictures and icons the app ships
  README.md  .gitignore
  dist/         what is built; nothing generated is under src/
```

Every path an app writes is a path of the packed app, which is the shape the three directories
make once the app is built (and the shape `pencere dev` reads them in):

| in the project           | in the app           | the app names it                                                        |
| ------------------------ | -------------------- | ----------------------------------------------------------------------- |
| `src/program/app.lua`    | `app.lua`            | the script that starts the app                                          |
| `src/program/api/x.lua`  | `api/x.lua`          | `require("api.x")`                                                      |
| `src/view/page.lua`      | `view/page.lua`      | nothing: `serve` shows what `view/` has                                 |
| `src/view/screens/x.lua` | `view/screens/x.lua` | `require("./screens/x")`, in the page                                   |
| `src/view/index.html`    | `view/index.html`    | nothing: the same                                                       |
| `src/assets/icon.png`    | `assets/icon.png`    | `icon = "assets/icon.png"`, `b.image("assets/icon.png")`, `app.read(…)` |

A program does not name its view: `serve{}` shows what `view/` has, a page of the kit if there is a
`page.lua` and a web page if there is an `index.html` (both: the page of the kit to a terminal that
draws views itself, the web page to the others). `view/` and `assets/` are the page's and the
pictures' at the top of the app, so a directory of `src/program/` cannot be called either.

A page of the kit can be split into files. `page.lua` is the script; it may `require` the other
`.lua` and `.luau` files of `src/view/`, by Luau's rules for requiring by string, with paths
relative to the file that asks: `require("./screens/logs")` is `src/view/screens/logs.lua` (or
`logs.luau`, or `logs/init.luau`), and `require("../parts/table")` goes up a directory. The view
reaches nothing outside its own files: a path out of `view/`, an absolute path or a file that is not
a script is an error in the page, with the path in it. Each file runs once. (That is the page's
`require`; the program's own, `require("api.files")`, loads the modules of `src/program/`.)

A page that a tool builds (a React page, say) is packed from where it is built, and not from
`src/view/`: `pencere build --web dist/view`. The name of the app, which is the executable's and
the one the app's own messages carry, is that of the directory unless `--name` says another.

`pencere build <dir>` takes a directory that is not laid out so, the simple form: an `app.lua`
with the modules, a `page.lua` or a `web/` directory, and `assets/`, as they lie in the directory,
and `serve` finds them there. It is what to write for an app of a script and a page.

## An app

```lua
-- src/program/app.lua: the program
local pencere = require("pencere")

pencere.serve {                            -- shows src/view/: page.lua, a page of the kit
  calls = {                                -- what the view asks for, once
    hello = function(params) return { message = "Hello, " .. (params.name or "world") end,
  },
  topics = {                               -- what the view listens to as it changes
    clock = function(_, sub)
      local timer = pencere.time.every(1, function() sub:push({ now = pencere.time.now() }) end)
      sub:on_end(function() timer:cancel() end)
    end,
  },
  fallback = function() print("This terminal shows no views.") end,
}
```

`require("pencere")` is all an app has:

|                                                                                                           |                                                                                                                                                                                                                                                                                                                                                                                                                                                                                             |
| --------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `serve{ page, web, views, assets, access, title, program, icon, icons, filter, calls, topics, fallback }` | shows the view and waits until it is over; returns how it ended (`shown`, `ended` or `reason`, `status`). `calls` answer with a value or `error({ code =, message = })`; they get `call:progress(done, total, note)` and `call.cancelled`. `topics` get `sub:push(value)`, `sub:on_end(fn)`, `sub.cancelled`. `filter = true` is for a program in a pipeline: the view is shown on the controlling terminal and not on stdin and stdout, which stay the data's. What the view is: see below |
| `process.spawn{ argv, cwd, env, input, raw, session, max_output, on_stdout, on_stderr, on_exit }`         | starts a program (no shell reads `argv`) in a group of its own (with `session = true`, in a session of its own, which has no controlling terminal: it cannot open `/dev/tty`), so that what is sent to it reaches everything it started; its output comes a line at a time; `max_output` (bytes, line ends included) ends a program that says more, and `on_exit(code, signal, overflowed)` says it did; `child:kill(signal)`, `child.pid`                                                  |
| `process.run{ argv, timeout, raw, max_output }`                                                           | waits for it and gives `{ code, signal, ok, timed_out, overflowed, stdout, stderr }`; the app's other work goes on meanwhile. `timeout` and `max_output` end the program and whatever it started (a shell's pipeline too). `raw` is for output that is not text (a list ended by NUL): it comes exactly as written                                                                                                                                                                          |
| `handoff{ argv }`                                                                                         | gives the terminal to a program and takes it back when it ends; see below                                                                                                                                                                                                                                                                                                                                                                                                                   |
| `close_view()`                                                                                            | ends the view the app is showing, from the app's side, as the app's end does; `serve` then returns (`ended` is `"program"`). See below                                                                                                                                                                                                                                                                                                                                                      |
| `fs`                                                                                                      | `read`, `write`, `append`, `create` (only where nothing is: never truncates, never writes through a link), `rename`, `list` (with `link`), `stat`, `exists`, `writable`, `mkdir`, `remove`, `cwd`, and `watch(dir, fn)`                                                                                                                                                                                                                                                                     |
| `json`                                                                                                    | `encode`, `decode`, `object()` (an empty table that is sent as `{}`), `null` (sent as `null`)                                                                                                                                                                                                                                                                                                                                                                                               |
| `hash.sha256(data)`                                                                                       | 64 hex digits                                                                                                                                                                                                                                                                                                                                                                                                                                                                               |
| `text`                                                                                                    | `compare(a, b[, tie])` (the order names are listed in: `file9` before `file10`, `a` beside `A`), `units`, `index`, `wire_size`, `wire_fit` (a text counted in UTF-16 units, as a page counts it; `wire_size` is the size the protocol counts a message by, the text's UTF-8 before any escape; they read the bytes they are given, a run of bytes that is not UTF-8 is one character, and each asks for no more of the text than it needs), `valid`, `lossy`                                |
| `regex.new(pattern, { ignore_case, whole_word })`                                                         | a pattern a person typed: `:find(text[, init])` gives `first, last` as `string.find` does (a negative `init` counts from the end); groups, `\|`, repeats, `\d \w \s \b`, `\p{L}`, `(?i)`; no back-references and no look-ahead or look-behind (the pattern is refused with the words for it), so no pattern can hold the app; `whole_word` means no letter, digit or `_` beside the match; `regex.escape(text)`; a pattern that is not one raises                                           |
| `time`                                                                                                    | `now`, `monotonic`, `after`, `every` (timers with `:cancel()`), `sleep`                                                                                                                                                                                                                                                                                                                                                                                                                     |
| `os`, `env`, `args`, `path`, `app`                                                                        | `os.home()`, `os.tmpdir()`, `os.hostname()`, `os.username()`, `os.platform`, `os.arch`; the environment (read); the arguments; path functions; the files the app came with                                                                                                                                                                                                                                                                                                                  |
| `io.write(text)`, `io.ewrite(text)`, `io.read_line(prompt)`                                               | the terminal's own text, for the interface an app shows where there is no view (`print` goes to the log when there is one)                                                                                                                                                                                                                                                                                                                                                                  |
| `io.read_stdin(on_data[, on_end])`, `io.is_terminal(stream)`                                              | standard input as it arrives (see below), and whether `"stdin"`, `"stdout"` or `"stderr"` is a terminal                                                                                                                                                                                                                                                                                                                                                                                     |
| `exit(code)`                                                                                              | ends the app                                                                                                                                                                                                                                                                                                                                                                                                                                                                                |

## The view: a page of the kit, a web page, or both

```lua
pencere.serve { calls = ... }                              -- what src/view/ has: page.lua, index.html, or both
pencere.serve { page = "view/page.lua", calls = ... }      -- only this script of the kit's page API
pencere.serve { web = "view", calls = ... }                -- only this directory of HTML, JavaScript and CSS (index.html is the entry)
pencere.serve {                                            -- either: the terminal takes the first it can show
  views = {
    { kind = "lua" },                                      -- view/page.lua
    { kind = "web", access = { connect = { "https://api.example.com" } } },  -- view/
  },
  calls = ...,
}
```

`page`, `web` and a view's `dir` are paths of the packed app (the table above); a view's `page` and
`dir` default to the `view/page.lua` and `view/` the app has.

A terminal that draws views itself shows the page of the kit in its own font and theme; one that has
a web view shows the web page; one that has neither runs `fallback`. A web page may reach the
network only where `access` says (`connect`, `resources`, `frames`, `base` as lists of origins, and
`permissions` such as `"camera"`), and the person is asked first. `pencere new <name> --page web`
makes a Lua program with a web page: `src/program/app.lua`, `src/view/index.html`,
`src/view/app.js` and `src/view/pencere-page.js`, which is the SDK's bridge between a page and its
program, one file with no dependencies (it is the SDK's, taken when the command is built, not
written by hand). There is no npm and no build step. `pencere dev` shows the page again when a file
under `src/view/` changes, and `pencere build` packs the directory into the executable. A page
that is built by a tool is run and packed from its build directory: `pencere dev --web dist/view`,
`pencere build --web dist/view`.

## Files that change

`fs.watch(directory, function(changes) … end)` calls the function with the files that changed under
the directory (and below it; `{ recursive = false }` for the directory alone): `{ path, kind, dir }`
each, `kind` being `created`, `modified`, `removed`, `renamed` or `other`. The system tells (FSEvents
on a Mac, inotify on Linux); nothing is polled, and an app whose files do not change is not woken.
`watch:stop()` stops it.

## Files with the view

A page of the kit can show files and take them, and its bytes never pass through it (the protocol,
Sections 8.8 to 8.10; a terminal that opens `lua` 2.1). The page gets handles; the program, with
`pencere.files`, moves the bytes:

- **Showing a file.** `files.attach(path)` (or `files.attach_data(bytes)`) reads it once, gives it to
  the view and returns the name the page draws it by: `pencere.image(name)` on the page. A file the
  view has, or the terminal still holds, is not sent again; `files.detach(name)` lets it go. A view
  shown again needs its files again.
- **Taking a file.** The person drops a file on the page, pastes one, or chooses it in the Open
  panel the page asks for (a reply's `pick = true` to a press, a key, a menu pick or a command); the
  page is told `{ type = "files", source =, files = { { handle =, name =, size =, type = }, ... } }`,
  draws an image by `"file:" .. handle`, and passes the handle to its program, which calls
  `files.receive(handle, path, { progress = function(bytes) end })`. The file is written beside
  `path` as `.<name>.pencere-part` and renamed at the end; a `progress` that returns `false` gives it
  up, and a part that is not finished is deleted, ⌃C and a closed view included. The name is the
  person's: `files.safe_name(name)` keeps its last part, with nothing a path could be made of.
- **Giving a file.** The page asks for the Save panel (`save = { name =, type = }`), is told `saving`
  with the handle of the place the person chose, and passes it on; `files.send(handle, path)` writes
  the file there.

Each waits without holding the loop, and runs once the view is up (in a call of the view, or after
it). A failure is raised as `{ code =, message = }` (the terminal's `limit`, `gone`, `order`, …;
`unsupported` on a terminal that opens only `lua` 2.0), which a call of the view answers the view
with. `examples/files-lua` does all three; a test plays the person with `give_files`,
`drag_files_over`, `panel`, `choose_files` and `choose_place`, and the program's files with
`test.open{ attach = { bytes } }`.

## A filter in a pipeline

A program that sits in the middle of a pipeline (`… | tool | …`) has the data on stdin and writes it
on stdout, and shows its view on the controlling terminal: `serve{ filter = true }`. A program must
decide this itself, with a flag or a mode of its own, and not guess it from whether its streams are
terminals (a pipeline and a program started by another program look alike from inside).

```lua
local pencere = require("pencere")
local chunks, done = {}, false
pencere.io.read_stdin(function(bytes)           -- as it arrives: pieces of up to 64 KiB, exactly as written
  chunks[#chunks + 1] = bytes                   -- the view is up and answering while the data comes
end, function() done = true end)                -- the input has ended
local keep
pencere.serve { filter = true, calls = {        -- the view on the controlling terminal
  rows = function() return { size = #chunks, done = done } end,
  choose = function()                           -- the person has chosen
    keep = true
    pencere.time.after(0, pencere.close_view)   -- after this call's answer has gone
  end,
} }
if keep then pencere.io.write(table.concat(chunks)) end   -- the pipeline's data goes on only now
pencere.exit(keep and 0 or 1)
```

`io.read_stdin` reads one piece in a turn of the loop, so the app's calls, timers and processes are not
held back by a slow writer or starved by a fast one. The function is called for each piece in order and
should not wait. `reading:wait()` waits for the end of the input, for a program that shows no view;
`reading:stop()` ends the reading early and tells the writer the pipe has no reader, as `head` does.
`pencere.close_view()` ends the view when the program has what it came for: the terminal takes it
down and gives the pane back, the `serve` that showed it returns, and the program writes its result
to stdout with the pane its own again. Ask for it after the answer of the call that decided it has
gone (`time.after(0, pencere.close_view)`), or the page loses its answer with the view.
`io.write` writes to stdout exactly what it is given and waits until it is taken, so a full pipe slows
the program down; a program whose reader has gone is told by the error of `io.write`. The runner reads `--log FILE` and
`--` before the app's own arguments; everything else on the command line is the app's.

## Giving the terminal to a program

`pencere.handoff { "zsh", "-l" }` closes the view, gives the terminal to the program as a shell
gives it to a command (the screen the person had, the program in the foreground), and when the
program ends takes the terminal back and shows the view again: the same `serve`, from the files as
they are then. It gives `{ code, signal }`. It needs a terminal that can close a view at the program's
request; an editor that opens the person's shell in its pane is the use. The loop waits while the
program runs.

Everything an app runs is a coroutine on one thread: a call of the view, a timer, a process's
output. A function that waits (`process.run`, `time.sleep`, `serve`) suspends only its own, so a
slow `docker stop` does not freeze the stats that are coming in. The calls and the subscriptions of
the view start in the order the view made them, and none waits for another's answer: an `edit`
the view sent after another is begun after it. Other `.lua` files
of `src/program/` are modules (`require("api.files")` is `src/program/api/files.lua`), and the
files of `assets/` go to the page of the kit with it.

`print` and the errors of a built app go to the terminal, which is under the view while it is up:
run it with `--log FILE` to read them in a file. `pencere dev` does that by itself (`.pencere/log`).

## Testing an app

```sh
pencere test                      # every test/**/*_test.lua of the app
pencere test test/page_test.lua:12 --grep clock --reporter tap --junit report.xml
```

A test is Lua that opens the page of the app under a host with no screen — the page engine, its
editors and the kit's interpreter, laid out and scrolled as a terminal does, with a clock the test
moves — and does to it what a person does, looking at what it shows. It needs no browser, no
terminal and no Node. `pencere test` runs the app's tests with `pencere-test`, a program of its own
(it links the kit, which the command copied into every app does not): the one beside this command, or
this version's release file (fetched once, checked against `SHA256SUMS`, kept), or the one
`PENCERE_TEST` names. `pencere new` makes a project with its first test.

```lua
local test = require("pencere.test")
local expect = test.expect

test("the button shows what the program answers", function()
	local page = test.open({
		calls = { hello = function(params) return { message = "Hello, " .. params.name .. "!" } end },
	})
	expect(page):to_say("Press the button.")
	page:get_by_role("button", { name = "Greet" }):click()
	expect(page):to_say("Hello, pencere!")
	expect(page):to_have_called("hello", { name = "pencere" })
end)
```

- **The program of the page** is a stand-in made of the calls the test answers (`calls = { m = fn }`;
  a call with no handler waits for `page:answer(m, result)`, and `page:push(topic, value)` speaks to
  what the page listens to), or the app's own: a Lua program runs in the test's process, in its
  own environment (`env`, `cwd`, `args`; the test's process is never touched), a Cargo package is
  built once and run on a pty, and any other (`command = { "deno", "run", "-A", "src/program/main.ts" }`)
  is run on a pty as a terminal runs it. A page with a program of its own is looked at again until it
  shows what is expected (`within`, 5 seconds); with a stand-in, what is expected is there or not.
- **Finding things** as a person does, not by the page's own names: `get_by_text`, `get_by_role`
  (button, field, editor, link, checkbox, heading …), `get_by_field`, and `get_by_id` for what the
  page names. A locator looks again when it is used, wants exactly one match, and says what is on the
  page when there is none; `:nth(i)`, `:first()`, `:last()` and `locator:get_by_…` narrow it.
- **Doing**: `click`, `double_click`, `right_click`, `hover`, `drag_to`, `wheel`, `fill`, and `press("cmd+f")`,
  `type`, `paste`, `compose` for the keyboard; `advance`, `step`, `idle` for the clock, which
  only the test moves (the page's; the program's is the machine's).
- **Looking**: `expect(page):to_say`, `to_have_called`, `to_have_subscribed`, `to_have_link`,
  `to_have_focus`; `expect(locator):to_exist`, `to_have_count`, `to_be_checked`, `to_have_value` …;
  `expect(value):to_equal` (with the difference), `to_contain`, `to_match`, `to_throw`. `.never` turns
  any of them round.
- **Structure**: `test.describe`, `before_each`, `after_each`, `test.skip`, `test.only` (refused when
  `CI` is set), `{ timeout = }`, `test.temp_dir()`, and a test file's helpers in `test/`, found before
  the program's own modules.
- **A failure** says where, what the page said, and keeps the page: a picture
  (`.pencere/test/<file>/<test>/page-1.png`, each character a box, the same on every machine), its
  words, its messages and what the program printed. `GITHUB_ACTIONS` makes it an annotation; `--junit`
  writes a report the CI systems read, with the pictures attached.
- **A test that never ends** is cut off at its timeout (30 seconds, `--timeout`), and a file whose
  process dies costs that file; the files run on `--jobs` workers.

An app of the simple form (its files packed as they lie) does not pack `test/`, so a program module
of that form cannot live in a directory of that name.

## What it is, and is not

- **An app is a program.** It runs with the rights of whoever starts it, as any program does:
  it reads and writes files, starts programs, and sees the environment. Nothing in it is
  sandboxed; the _view_ is (it reaches nothing but its program). There is no network client in the
  standard library — a program that wants the network runs `curl`, and the person can see which.
- **One file.** `pencere build` copies the runner and writes the app into it as a section of the
  executable, so the file is the interpreter, the script, the page and the pictures. A Mac's file
  is signed ad hoc, in the process; a Linux one is static.
- **Another target without its toolchain.** `--target` fetches the same version's runner of that
  target from the release (`curl`, checked against the release's `SHA256SUMS`, kept in the cache),
  and `--runner FILE` uses one you have. A pencere that is not released, a commit of it, has no
  runners to fetch: `make runner TARGET=<target>` in a checkout of it builds one into
  `target/runners/` (Docker for Linux, a Mac for macOS).
- **Other languages need nothing of this to run.** The protocol is bytes; the TypeScript SDK
  (`@pencere/sdk`, `deno compile`) and the Rust library (`pencere`) are the same program in other
  words, and `pencere new --lang ts|rust` starts one. For a Rust program `dev` and `build` are a
  convenience over Cargo, which does the compiling: the runner is not in it.

The specification is [here](https://github.com/bateri/pencere/blob/main/spec/draft/pencere.md).
Apache-2.0.
