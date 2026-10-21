# @pencere/cli

Run an MCP server written in any language as a pencere program, so its MCP App view opens
in the terminal pane you ran it in.

```sh
npx @pencere/cli run -- python server.py
npx @pencere/cli dev --view dist/view -- node dist/server.js
```

[pencere](https://github.com/bateri/pencere) shows an MCP App view in the terminal pane a
program runs in, on your machine or on a server over plain ssh. The program is an MCP server,
and `@pencere/sdk` is how a TypeScript one asks the terminal for a view. `pencere run` is for
every other server: Python, Go, Rust, a shell script, anything that speaks MCP over stdio. It
runs the server as an MCP client would, talks pencere's protocol with the terminal, and carries
the messages between the two. The server is the one you would give Claude Desktop or VS Code,
unchanged. `pencere dev` is the same while you work on the view or the server: when their files
change, the view is shown again.

## Use

```
pencere run [options] -- <command> [args...]
```

Everything after `--` is the server's command, taken as it is.

| Option            | What it does                                                                                                                                                                                                   |
| ----------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `--tool <name>`   | The tool whose view to show. Without it, `pencere run` starts the server once to ask (`initialize` and `tools/list`) and takes the one tool whose `_meta.ui.resourceUri` names a view.                         |
| `--args <json>`   | The arguments the tool is called with, a JSON object.                                                                                                                                                          |
| `--view <dir>`    | Show the view from the files in `<dir>` (it needs an `index.html`). They cross the terminal's stream once and the terminal keeps them by content. Without it the terminal reads the server's `ui://` resource. |
| `-h`, `--help`    | Usage.                                                                                                                                                                                                         |
| `-v`, `--version` | The version.                                                                                                                                                                                                   |

The same options and exit status are `pencere dev`'s, which adds `--watch` (see below).

If the server has no tool with a view, or several, `pencere run` says which tools there are
and, for several, asks you to choose one with `--tool`. A tool names a view by a `ui://`
resource, as the terminal reads it, and the tool list is read a page at a time as far as the
terminal reads it (16 pages). What the server wrote is shown as text.

A terminal that does not host pencere is told so: `pencere run` says that the command is an MCP
server, which an MCP client can take as it is, and ends. A server is not a text interface, so it
is not attached to the terminal, and with `--tool` it is not started for it at all (without
`--tool` it was started once, to ask which tool has a view).

### Exit status

| Status                  | Meaning                                                                                                                                                                                                   |
| ----------------------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `0`                     | The view ran and closed (you closed it, the view did, or the program did).                                                                                                                                |
| `1`                     | It could not run: the server did not start, did not answer as an MCP server, or ended while its view was up; the terminal refused or ended the view, or stopped answering while the view was shown again. |
| `2`                     | A mistake in the command line, or a `--view` directory that is not a view, or several tools with a view and no `--tool`.                                                                                  |
| `3`                     | This terminal does not show pencere views, or the input or output of `pencere run` is not a terminal.                                                                                                     |
| ⌃C, `SIGTERM`, a hangup | The view and the server are ended first, then `pencere run` ends by the same signal, so the shell reads 130, 143 or 129 (the terminal went away, as when ssh does).                                       |

When the server fails, what it wrote to stderr is shown on the screen you get back. While a
view is up the screen is the view's, so the end of it is kept (the last 16 KiB) and printed
after the view has closed, as text.

## Reloading with `pencere dev`

```
pencere dev [options] -- <command> [args...]
```

```sh
pencere dev --view dist/view -- node dist/server.js     # a view that is built into dist/view
pencere dev --watch server.py -- python server.py        # a server whose own files are the work
```

`pencere dev` is `pencere run`, and it watches files. When they change, the view in the pane
closes, the server is started again, and the view opens again, in the same pane and without
leaving the alternate screen. What the view held in its own memory is gone, as it is when the
terminal reconnects: its state is rebuilt from the server's tool results, as at the first open.

| Option           | What it watches                                                                                                                                         |
| ---------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `--view <dir>`   | The view's files. A change that leaves the files as they were (a save that wrote the same bytes) shows nothing; the terminal is sent only what changed. |
| `--watch <path>` | A file or directory the server reads, as often as needed. A change to what it holds (size or modification time) starts the server and the view again.   |

One of the two is needed. Names that begin with a dot, `node_modules` and `__pycache__` are not
watched under a directory: a server that writes into a directory it is watched through would end
every run with a change.

- **A burst is one reopening.** A build writes many files; the view is reopened once they have
  been quiet for a moment, and a change that comes while it is reopening is shown after it.
- **A half-built view is not shown.** A build that has emptied its output directory and not yet
  written `index.html` leaves something that is not a view. The change is looked at again at the
  next one, which is the build writing more.
- **The terminal does the swap.** The view is closed and opened by the terminal's own messages
  (`dismiss`; specification §10.8), which is why this works when the server runs over plain ssh
  and the terminal is on the other side: the files cross the stream, not a port. A terminal that
  does not list `dismiss` in its `welcome` cannot reopen a view: `pencere dev` runs the view as
  `pencere run` would, and says after the view has closed that the changes were not shown.
- **What it tells you is told after.** While the view is up the screen is the view's, so notes
  (a view that could not be read at the last change, a watch that failed) are printed on the
  screen you get back, as the server's stderr is.
- **A server that ends is waited out.** If the server exits while its view is up or opening, as
  after a syntax error is saved, the view goes with it and the terminal is yours again: `pencere dev`
  says that the server ended, shows the end of what it wrote to stderr, and waits. The next change
  to the files it watches starts the server and the view again, and nothing has to be run twice.
  A change that leaves the files as they were is not another try. ⌃C ends `pencere dev` as ever.
  `pencere run`, which watches nothing, ends with the server.

## How the server is run

- **As an MCP client runs it.** The command is started with your whole environment and working
  directory, with pipes for stdin and stdout; stdout carries one JSON-RPC message to a line.
  Each view is a new MCP session and so a new process: a stdio server expects one `initialize`
  for the life of its process, and a view that the terminal starts again (after a reconnection)
  or that the program reopens gets a server of its own.
- **In a process group of its own.** ⌃C is the terminal's signal to the foreground group, which
  is `pencere run`'s. The server must not take it: `pencere run` ends it, after telling the
  terminal the program is done. It closes the server's stdin first, which every stdio server
  takes as the end, then sends SIGTERM and SIGKILL to the whole group, which catches what a
  wrapper (`npx`, `uv run`, `sh -c`) started. A server that leaves at the end of its stdin, or
  falls, leaves what it started behind in the group, and that gets the same two signals.
- **Nothing else is added.** What the terminal asks (the question, the token, the bundle's
  files, ⌃C and `bye`, starting the view again) is done by `@pencere/sdk`'s `serve`, which this
  package runs; `pencere run` does not parse the server's messages.

Node 22 or later is needed, on a system with `stty` (macOS, Linux): the SDK sets the terminal's
modes with it, so Windows has no host as far as it is concerned.

## Versions

This release speaks version 1 of the pencere wire (a draft), through `@pencere/sdk`. The package
follows semantic versioning; [CHANGELOG.md](CHANGELOG.md) says what each release changed.

## License

Apache-2.0 — see [LICENSE](LICENSE).
