# @pencere/cli

Run an MCP server written in any language as a pencere program, so its MCP App view opens
in the terminal pane you ran it in.

```sh
npx @pencere/cli run -- python server.py
```

[pencere](https://github.com/bateri/pencere) shows an MCP App view in the terminal pane a
program runs in, on your machine or on a server over plain ssh. The program is an MCP server,
and `@pencere/sdk` is how a TypeScript one asks the terminal for a view. `pencere run` is for
every other server: Python, Go, Rust, a shell script, anything that speaks MCP over stdio. It
runs the server as an MCP client would, talks pencere's protocol with the terminal, and carries
the messages between the two. The server is the one you would give Claude Desktop or VS Code,
unchanged.

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

If the server has no tool with a view, or several, `pencere run` says which tools there are
and, for several, asks you to choose one with `--tool`.

A terminal that does not host pencere is told so: `pencere run` says that the command is an MCP
server, which an MCP client can take as it is, and ends. A server is not a text interface, so it
is not attached to the terminal, and with `--tool` it is not started for it at all (without
`--tool` it was started once, to ask which tool has a view).

### Exit status

| Status        | Meaning                                                                                                                                              |
| ------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------- |
| `0`           | The view ran and closed (you closed it, the view did, or the program did).                                                                           |
| `1`           | It could not run: the server did not start, did not answer as an MCP server, or ended while its view was up; the terminal refused or ended the view. |
| `2`           | A mistake in the command line, or a `--view` directory that is not a view, or several tools with a view and no `--tool`.                             |
| `3`           | This terminal does not show pencere views, or the input or output of `pencere run` is not a terminal.                                                |
| ⌃C, `SIGTERM` | The view and the server are ended first, then `pencere run` ends by the same signal, so the shell reads 130 or 143.                                  |

When the server fails, what it wrote to stderr is shown on the screen you get back. While a
view is up the screen is the view's, so the end of it is kept (the last 16 KiB) and printed
after the view has closed, as text.

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
  wrapper (`npx`, `uv run`, `sh -c`) started.
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
