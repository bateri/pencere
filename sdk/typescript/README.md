# @pencere/sdk

Show a view in the terminal pane your program runs in.

A program running in a terminal pane — on your machine, or on a server reached over plain
ssh — can show a view of its own in that pane, the way a full-screen program takes the screen and
gives it back when it ends. The view is a web page, or a WebAssembly component or a Lua script the
terminal draws itself; whichever it is, it and your program talk in six small messages, carried in
the terminal's own byte stream. This package is the program's side of that, as the
[specification](https://github.com/bateri/pencere/blob/main/spec/draft/pencere.md) defines it.

```ts
import { bundleFromDirectory, serve } from "@pencere/sdk";

const result = await serve({
  views: [{ kind: "web", bundle: await bundleFromDirectory(new URL("./view/", import.meta.url)) }],
  program: "containers",
  title: "Containers",
  calls: { list: () => ({ containers: listContainers() }) },
  topics: {
    stats: (_params, subscription) => {
      const timer = setInterval(() => subscription.push(readStats()), 1000);
      return () => clearInterval(timer);
    },
  },
  fallback: () => console.log(table()),
});
```

`serve` does what the situation asks:

- **A terminal that hosts pencere.** The terminal's modes are set, the host is asked which kinds of
  view it opens, the alternate screen is taken, and the view is opened with the token the host
  gave. The files of the bundle the host does not already hold are sent, your handlers answer
  the view, and when the view closes the terminal is given back and `serve` resolves with
  `{ kind: "closed", by }`. ⌃C ends the program as it always does: `bye` is sent, the screen and
  the modes are restored, and the signal goes on.
- **A terminal that does not, or no terminal at all.** `serve` resolves with
  `{ kind: "no-view", reason }` after the `fallback`, if there is one, has run: the program offers
  its own text interface. No frame of the protocol is sent when stdin or stdout is not a terminal.

## Views

`views` lists the kinds of view the program can show, in the order it prefers them. The host opens
the first kind it can, at the first version of that kind's interface it speaks:

- `web` is a page: HTML and JavaScript, with the files in a bundle (`bundleFromDirectory`, or
  `bundleFromFiles`), sent once and then served from the terminal's store by content. A page that
  needs to reach a domain, or the camera, microphone, location or clipboard, says so in its
  `access`; the person is asked, once, and the page is told what they allowed.
- `wasm` and `lua` are views the terminal draws itself with the pencere kit. They say which
  `versions` of the kit's interface they are written against, and `bundle` may be a function of
  the version that was agreed, for a view that has a build for each.

`program`, `title` and `icons` are what the terminal writes above the view and draws beside it. A
terminal draws an icon only from the bytes it is given and never fetches one, so give it as a PNG,
JPEG or WebP in a `data:` URI, small and not animated; an `https:` icon is left out in the pane.
Make it square: a terminal draws it in its own frame, at its own size. On a clear background the
icon stands on the terminal's own face, as its marks do; filled to its edges, the icon's ground
becomes the face. A version for a light background and one for a dark one, each with its `theme`,
let the terminal take the one that suits its theme.

## Calls and topics

The view asks and the program answers. `calls` are the methods the view may call, each returning a
result (or throwing a `ViewError` with a short `code` the view can act on and, if you like, words
for the person); a method that is not there is answered `not-found`. `topics` are the
subscriptions the view may make: a handler pushes values to the view with `subscription.push` until
the view cancels it or ends, and returns a function that stops whatever feeds it. A call that takes
a while reports how far it has got with `call.progress`, and stops when `call.signal` is aborted:
the view gave it up. The view side of all this is a few lines of JavaScript posting JSON objects to
its window's parent (specification §9.4); the [`containers`
example](https://github.com/bateri/pencere/tree/main/examples/containers) has both halves.

A program that wants to close its view and go on — to show its own interface for a while, or, while
a program is developed, to show the view again with the files that changed — gives `serve` a
`ViewControl` and calls it while `serve` runs. A host that lists `dismiss` in its `welcome` takes
it; with another the view is left as it is and the call says `unsupported`.

```ts
const control = new ViewControl();
const served = serve({ views: [{ kind: "web", bundle }], calls, control });
// when the view's files changed:
await control.reopen({ views: [{ kind: "web", bundle: await bundleFromDirectory(directory) }] });
// or to leave the view and show the program's own interface:
await control.close(); // `serve` then resolves with `{ kind: "closed", by: "program" }`
```

The terminal's modes are set with `stty`, not with `process.stdin.setRawMode` or Deno's `setRaw`: raw
mode also turns off ISIG, and ⌃C would arrive as a byte instead of a SIGINT. A system with no `stty`,
such as Windows, has no host as far as this library is concerned.

## Runtimes

The same code runs on Node (20 and later), Deno (2) and Bun, with nothing to configure. The terminal's
modes, the signals and the files are each reached the way the runtime allows; the rule is the same
on all three (specification §4.9): the tty is not canonical and does not echo, ISIG stays on so that
⌃C is a SIGINT, and every mode is given back on every way out.

- **Node** needs nothing.
- **Deno** needs permission to read the view's files and to run programs (`stty`, and the signal
  the program sends itself): `deno run --allow-read --allow-run main.js`. Install the package with
  `npm:@pencere/sdk`. A program that is not allowed to run programs ends with the status a shell
  reports for a signal (130, 143) instead of being ended by the signal. A `filter` asks for the
  terminal it opens itself, which Deno lets a program write to only with `--allow-all`.
- **Bun** needs nothing. A `filter` does not work on Bun 1.0, the version this was tried on: its
  stream for a tty does not deliver what the terminal writes to `/dev/tty`, so a filter finds no host.

`deno compile` makes one executable of the program, its view and the Deno runtime. A view the
program reads from a directory at run time is put in with `--include`; see the
[`containers` example](https://github.com/bateri/pencere/tree/main/examples/containers) for the
task, which keeps the executable down to what the program needs.

```sh
deno compile --allow-read --allow-run --no-prompt --include view --output containers main.js
```

The program leaves the terminal as it found it when it ends in any way it can run code in:

- **⌃C and SIGTERM** send `bye` and wait for the answer, reading the input and throwing it away
  (`byeTimeout`, 500 ms by default), then give the screen and the modes back and let the signal go
  on. A signal before the host has answered the `hello` waits, in the same way, for the terminal's
  answer to `CSI c`, so that a late `welcome` does not reach the shell as typed text.
- **`process.exit`** (the `exit` event) cannot wait for the event loop. The program says `bye`, and
  reads the tty itself, with no loop, until the answer to it or `byeTimeout`.
- **⌃Z** closes the view (with `dismiss`, or `bye` for a host that does not list it), gives the
  screen and the modes back, and stops; when it is continued it takes the modes again and asks
  for the view anew, as the first time. A runtime that cannot catch SIGTSTP leaves ⌃Z to the
  system.

## The page's side

A `web` view reaches the program through the bridge of §9.4, which `@pencere/sdk/page` is, as
functions: a browser ES module with no dependency that does nothing until it is used.

```js
import { call, close, onContext, openLink, subscribe } from "@pencere/sdk/page";

onContext((context) => document.documentElement.setAttribute("data-theme", context.mode));

const { containers } = await call("list");

const abort = new AbortController();
await call(
  "pull",
  { image: "nginx" },
  {
    onProgress: ({ done, total, note }) => draw(done, total, note),
    signal: abort.signal, // aborting sends `cancel`, and the promise rejects
  },
);

const stop = subscribe(
  "stats",
  { every: 1000 },
  {
    onPush: (value) => draw(value),
    onError: (error) => console.warn(`the readings stopped: ${error.message}`),
    onEnd: () => hideSpinner(), // whoever ended it: the program, or `stop()`
  },
);
stop(); // or stop.cancel()
```

- `call(method, params?, { onProgress?, signal? })` resolves with the result, or rejects with a
  `CallError` (`code`, and `message`, which is the code when the program gave no words). A page
  that nothing embeds has no host: its calls are rejected with the code `no-host`.
- `subscribe(topic, params, handlers)` takes a function in place of `handlers` when only the
  values matter. The program ends a subscription with an error, which `onError` hears; no push
  follows. The page ends it with the function that is returned (also called `cancel`).
- `onContext(listener)` calls the listener with the terminal's context now, when the page has it,
  and on each change: the mode, the roles of the theme, the font, the size, the locale and the
  time zone, `reduceMotion` and what the person allowed (§9.5). `context()` is the last one.
- `close()` asks the host to close the view; the program is told, and decides. `openLink(url)`
  asks it to open an `http:` or `https:` link in the person's browser.
- `createBridge(channel)` is the same on a channel of your own, for a host that embeds the page in
  some other way, or for a test.

A page has no `node_modules`, so the module goes in the bundle beside the page, and an import map
in the page's HTML says where the name is. `bundleFromDirectory` takes the file with its `files`
option:

```ts
const bridge = readFileSync(new URL(import.meta.resolve("@pencere/sdk/page")));
const bundle = await bundleFromDirectory(new URL("./view/", import.meta.url), {
  files: [{ path: "pencere-page.js", data: bridge }],
});
```

```html
<script type="importmap">
  { "imports": { "@pencere/sdk/page": "./pencere-page.js" } }
</script>
<script type="module" src="app.js"></script>
```

A page that is built with a bundler imports the module like any other, and needs none of this.

## The router

`serve` hands the view's `call`, `subscribe` and `cancel` to a router, which finds the handler by
name, runs it, and writes the `response`, `push` and `progress` back, with the errors turned into
the view's words (`ViewError` as its `code` and message, anything else as `internal`).
`createRouter` is that router with no terminal: the handlers of an app can be tested, or driven by a
development harness, through it.

```ts
import { createRouter } from "@pencere/sdk";

const sent: unknown[] = [];
const router = createRouter({ calls, topics }, (payload) => {
  sent.push(JSON.parse(payload)); // the program's side of what the view hears
  return true;
});
router.handle({ call: { id: 1, method: "list" } });
// ... later, `sent` holds { response: { id: 1, result: { ... } } }
router.end(); // the view is over: calls are cancelled, subscriptions stop
```

## Versions

This release speaks version 1 of the pencere wire, which is still a draft: until the specification
publishes a dated version, a `0.x` release of this package may change the wire along with it. The
package follows semantic versioning; [CHANGELOG.md](CHANGELOG.md) says what each release changed.

## License

Apache-2.0 — see [LICENSE](LICENSE).
