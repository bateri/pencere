# create-pencere

Start a pencere tool: an MCP server whose view opens in the terminal pane it runs in, with a view
written in React or Vue and built by Vite.

```sh
npm create pencere@latest my-tool
cd my-tool
npm install
npm run dev
```

[pencere](https://github.com/bateri/pencere) shows an MCP App view in the terminal pane a program
runs in, on your machine or on a server over plain ssh. This writes the program: a server whose
tool has a view, the view, and what builds both, so that the first thing you do is change the
view and see it change in the pane.

## Use

```
create-pencere [directory] [options]
```

| Option                    | What it does                                          |
| ------------------------- | ----------------------------------------------------- |
| `-t`, `--template <name>` | The view's framework: `react` (the default) or `vue`. |
| `-h`, `--help`            | Usage.                                                |
| `-v`, `--version`         | The version.                                          |

With no directory or no template it asks, when it can be answered: from a script it needs the
directory, and the template is `react`. `npm create` passes options after `--`:
`npm create pencere@latest my-tool -- --template vue`.

The package is named after the directory (`My Tool` becomes `my-tool`), and the name is also the
host of the view's `ui://` address, so it keeps to lowercase letters, digits, `.`, `_` and `-`.
A directory that exists and holds anything but a `.git` is left as it is: nothing is written into
it, and nothing is ever replaced.

## What it writes

The same server and build for both templates, and a view in the framework you chose:

- `src/server/`: an MCP server with `@pencere/sdk`. Its tool `show_counter` has a view, and
  `add_one` is a tool only the view calls. Started in a terminal that shows pencere views, it opens
  the view in the pane. Started by an MCP client over stdio, it is an ordinary MCP server, so the
  same view shows in Claude Desktop or VS Code.
- `src/view/`: the view, which talks to the host with `@modelcontextprotocol/ext-apps` and takes
  the host's theme (the `--color-*` and `--font-mono` variables MCP Apps defines).
- `vite.config.ts`: the view is built twice. `dist/view/` is many files, which a pencere terminal
  receives once and keeps by content; `dist/single/index.html` is one document, which every other
  MCP Apps host reads.
- `npm run dev`: builds, shows the view in the pane with `pencere dev`, and builds again when a
  file under `src/` changes, so the view is shown again with what you saved. `npm run build` and
  `npm start` are the finished tool.

The project's dependencies (Vite, React or Vue, the MCP SDK, `@pencere/sdk`, and `@pencere/cli` for
`pencere dev`) are the project's own: this package has none, and it only writes files.

Node 22 or later is needed, as for `@pencere/sdk`, which sets the terminal's modes with `stty`.

## Versions

This release writes projects for version 1 of the pencere wire (a draft), through `@pencere/sdk`.
The package follows semantic versioning; [CHANGELOG.md](CHANGELOG.md) says what each release changed.

## License

Apache-2.0 — see [LICENSE](LICENSE).
