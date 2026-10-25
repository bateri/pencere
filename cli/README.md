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
```

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

|                                                                                                   |                                                                                                                                                                                                                                                                                                                                                                                                                                                              |
| ------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `serve{ page, web, views, assets, access, title, program, icon, icons, calls, topics, fallback }` | shows the view and waits until it is over; returns how it ended (`shown`, `ended` or `reason`, `status`). `calls` answer with a value or `error({ code =, message = })`; they get `call:progress(done, total, note)` and `call.cancelled`. `topics` get `sub:push(value)`, `sub:on_end(fn)`, `sub.cancelled`. What the view is: see below                                                                                                                    |
| `process.spawn{ argv, cwd, env, input, raw, max_output, on_stdout, on_stderr, on_exit }`          | starts a program (no shell reads `argv`) in a group of its own, so that what is sent to it reaches everything it started; its output comes a line at a time; `max_output` (bytes, line ends included) ends a program that says more, and `on_exit(code, signal, overflowed)` says it did; `child:kill(signal)`, `child.pid`                                                                                                                                  |
| `process.run{ argv, timeout, raw, max_output }`                                                   | waits for it and gives `{ code, signal, ok, timed_out, overflowed, stdout, stderr }`; the app's other work goes on meanwhile. `timeout` and `max_output` end the program and whatever it started (a shell's pipeline too). `raw` is for output that is not text (a list ended by NUL): it comes exactly as written                                                                                                                                           |
| `handoff{ argv }`                                                                                 | gives the terminal to a program and takes it back when it ends; see below                                                                                                                                                                                                                                                                                                                                                                                    |
| `fs`                                                                                              | `read`, `write`, `append`, `create` (only where nothing is: never truncates, never writes through a link), `rename`, `list` (with `link`), `stat`, `exists`, `writable`, `mkdir`, `remove`, `cwd`, and `watch(dir, fn)`                                                                                                                                                                                                                                      |
| `json`                                                                                            | `encode`, `decode`, `object()` (an empty table that is sent as `{}`), `null` (sent as `null`)                                                                                                                                                                                                                                                                                                                                                                |
| `hash.sha256(data)`                                                                               | 64 hex digits                                                                                                                                                                                                                                                                                                                                                                                                                                                |
| `text`                                                                                            | `compare(a, b[, tie])` (the order names are listed in: `file9` before `file10`, `a` beside `A`), `units`, `index`, `wire_size`, `wire_fit` (a text counted in UTF-16 units, as a page counts it; `wire_size` is the size the protocol counts a message by, the text's UTF-8 before any escape; they read the bytes they are given, a run of bytes that is not UTF-8 is one character, and each asks for no more of the text than it needs), `valid`, `lossy` |
| `regex.new(pattern, { ignore_case, whole_word })`                                                 | a pattern a person typed: `:find(text[, init])` gives `first, last` as `string.find` does (a negative `init` counts from the end); groups, `\|`, repeats, `\d \w \s \b`, `\p{L}`, `(?i)`; no back-references and no look-ahead or look-behind (the pattern is refused with the words for it), so no pattern can hold the app; `whole_word` means no letter, digit or `_` beside the match; `regex.escape(text)`; a pattern that is not one raises            |
| `time`                                                                                            | `now`, `monotonic`, `after`, `every` (timers with `:cancel()`), `sleep`                                                                                                                                                                                                                                                                                                                                                                                      |
| `os`, `env`, `args`, `path`, `app`                                                                | `os.home()`, `os.tmpdir()`, `os.hostname()`, `os.username()`, `os.platform`, `os.arch`; the environment (read); the arguments; path functions; the files the app came with                                                                                                                                                                                                                                                                                   |
| `io.write(text)`, `io.ewrite(text)`, `io.read_line(prompt)`                                       | the terminal's own text, for the interface an app shows where there is no view (`print` goes to the log when there is one)                                                                                                                                                                                                                                                                                                                                   |
| `exit(code)`                                                                                      | ends the app                                                                                                                                                                                                                                                                                                                                                                                                                                                 |

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

## Giving the terminal to a program

`pencere.handoff { "zsh", "-l" }` closes the view, gives the terminal to the program as a shell
gives it to a command (the screen the person had, the program in the foreground), and when the
program ends takes the terminal back and shows the view again: the same `serve`, from the files as
they are then. It gives `{ code, signal }`. It needs a terminal that can close a view at the program's
request; an editor that opens the person's shell in its pane is the use. The loop waits while the
program runs.

Everything an app runs is a coroutine on one thread: a call of the view, a timer, a process's
output. A function that waits (`process.run`, `time.sleep`, `serve`) suspends only its own, so a
slow `docker stop` does not freeze the stats that are coming in. Other `.lua` files
of `src/program/` are modules (`require("api.files")` is `src/program/api/files.lua`), and the
files of `assets/` go to the page of the kit with it.

`print` and the errors of a built app go to the terminal, which is under the view while it is up:
run it with `--log FILE` to read them in a file. `pencere dev` does that by itself (`.pencere/log`).

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
  and `--runner FILE` uses one you have.
- **Other languages need nothing of this.** The protocol is bytes; the TypeScript SDK
  (`@pencere/sdk`, `deno compile`) and the Rust library (`pencere`) are the same program in other
  words, and `pencere new --lang ts|rust` starts one.

The specification is [here](https://github.com/bateri/pencere/blob/main/spec/draft/pencere.md).
Apache-2.0.
