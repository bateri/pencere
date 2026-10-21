# {{title}}

A [pencere](https://github.com/bateri/pencere) tool: an MCP server whose view opens in the
terminal pane it runs in. The view is written with {{framework}} and built with Vite, and it is an
[MCP App](https://github.com/modelcontextprotocol/ext-apps), so the same server shows the same
view in any MCP Apps host.

## Run it

```sh
npm install
npm run dev
```

`npm run dev` builds the view and the server, shows the view in this pane, and shows it again each
time you save a file under `src/`. It needs a terminal that shows pencere views; in any other it
says so. What the builds write after the first is in `dev.log`, because while the view is up the
screen is the view's.

For the finished tool, `npm run build` and then `npm start`. Run over plain ssh on a server, it
shows its view in the pane you are connected from, with nothing else to set up.

## Elsewhere

The server is an ordinary MCP server on stdio. Give an MCP Apps host such as Claude Desktop or VS
Code the command `node /path/to/{{name}}/dist/server/main.js`, after `npm run build`: the host reads
the view from one self-contained document, `dist/single/index.html`.

`pencere run` starts any MCP server as a pencere program, whatever it is written in. Here it
is one more way to start the same server:

```sh
npx @pencere/cli run -- node dist/server/main.js
```

## What is where

| File                   | What it is                                                                                                                                                                       |
| ---------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `src/server/tool.ts`   | The MCP server: the tools, and the view as a resource.                                                                                                                           |
| `src/server/main.ts`   | Starts it, and asks the terminal to show the view.                                                                                                                               |
| `src/view/`            | The view, in {{framework}}.                                                                                                                                                      |
| `index.html`           | The view's page.                                                                                                                                                                 |
| `vite.config.ts`       | The view's build, made twice: `dist/view/` is many files, which a pencere terminal keeps by content, and `dist/single/index.html` is one document, which every other host reads. |
| `tsconfig.server.json` | The server's build into `dist/server/`.                                                                                                                                          |
| `scripts/dev.mjs`      | What `npm run dev` does.                                                                                                                                                         |

## The view

It talks to the host with `@modelcontextprotocol/ext-apps`: the result of `show_counter` arrives as
the first tool result, the button calls `add_one` through the host, and the colours and the font are
the host's, which sends them as the `--color-*` and `--font-mono` variables MCP Apps defines. The
view lists `pencere/pane` among its capabilities. That tells a pencere terminal it was written for
the pane: it fills the pane, and every key is the view's.
