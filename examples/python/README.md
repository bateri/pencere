# tasks, in Python

A tool with a view, written in Python with the official [MCP Python SDK](https://pypi.org/project/mcp/),
to show that a pencere tool needs no pencere library: any MCP server that speaks stdio can show its view
in the terminal pane, and `pencere run` is all it takes. It lists a few tasks and lets you tick them
off; nothing here needs more than Python.

- `server.py` is an ordinary MCP server: `show_tasks` is the tool with the view (its
  `_meta.ui.resourceUri` names `ui://tasks/view`), `toggle_task` is a tool only the view calls, and the
  view is a resource. Nothing in it knows about pencere. The same server, given to Claude Desktop or
  VS Code, shows the same view there.
- `view.html` is the view: one document with its stylesheet and script in it, written by hand against
  the MCP Apps messages, with no build step and no library. The server reads it when it starts.

## Run it

You need Python 3.10 or later and Node 22 or later. The one dependency is the MCP SDK, and it belongs to
this example, not to the repository:

```sh
cd examples/python
python3 -m venv .venv
. .venv/bin/activate
pip install -r requirements.txt
```

Then, in a terminal that hosts pencere views:

```sh
npx @pencere/cli run -- python server.py
```

`pencere run` starts the server as an MCP client would, finds the one tool that names a view, asks the
terminal to open it, and carries the messages between them. `↑` `↓` choose a task, `⏎` ticks it, ⌃C ends
the program and the terminal comes back. Over plain ssh on a server it works the same: the view opens in
the pane you are connected from.

To work on it, `pencere dev` shows the view again each time the files change:

```sh
npx @pencere/cli dev --watch server.py --watch view.html -- python server.py
```

`--watch .` does the same for everything in the directory (names that begin with a dot, such as `.venv`,
and `__pycache__` are not watched).

In a terminal that does not host pencere, `pencere run` says what the command is and ends: a server is
not a text interface.

## Try it without a terminal that hosts pencere

From the root of the repository, after `make setup` and `npx tsc -b` once, and with the example's
environment active:

```sh
cd examples/python
cargo run -p pencere-fake-host -- --call 'toggle_task={"id":"view"}' --then interrupt \
  -- node ../../cli/dist/main.js run -- python server.py
```

The [fake host](../../host/fake/README.md) plays the terminal and a scripted view, and writes what
happens, in order. `make check-network` runs this, and the same for the two templates `create-pencere`
writes, as a check.
