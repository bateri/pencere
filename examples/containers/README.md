# containers

A tool with a view, to try pencere with. It lists some made-up containers and
lets you stop and start them; nothing here needs Docker.

- `src/tool.ts` is an ordinary MCP server: `list_containers` (the tool with the
  view), `stop_container`, `start_container`, and the view's document at
  `ui://containers/view`. Nothing in it knows about pencere.
- `src/main.ts` is the only place that asks a terminal to show the view, with
  `serve` from `@pencere/sdk`.
- `view/` is the view: `index.html`, `style.css` and `app.js`, written by hand
  against the MCP Apps messages, with no build step and no library. A host that
  takes bundles gets the three files, each sent once and then kept by content; a
  host that does not (any MCP Apps host that knows nothing of pencere) gets one
  document with the stylesheet and the script put into it.

Started the three ways a tool can be:

- **In a terminal that hosts pencere:** the view opens in the pane. ⌃C ends the
  program and the terminal comes back.
- **In a terminal that does not:** it prints the containers and exits.
- **By an MCP client, over stdio:** it is an MCP server, and the client can list
  its tools and read the view.

## Dependencies

This directory is not an npm workspace, so that adding an example does not change
the repository's lockfile: inside the repository `@pencere/sdk` and the MCP packages
resolve from the root install (`make setup`), which has them because the SDK's own
development dependencies are the same. Copied out of the repository it is an ordinary
package: `npm install` reads `package.json`.

## Try it without a terminal that hosts pencere

From the root of the repository, after `make setup` once:

```sh
npx tsc -b
cargo run -p pencere-fake-host -- --call 'stop_container={"id":"cache"}' --then interrupt \
  -- node examples/containers/dist/main.js
```

The [fake host](../../host/fake/README.md) plays the terminal and a scripted view and
writes what happens, in order. Leave out `--then interrupt` to end the program with ⌃C
yourself, and add `--store some-directory` to see the second run find the files already
kept.
