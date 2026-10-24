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

The terminal's modes are set with `stty`, not with `process.stdin.setRawMode`: raw mode also turns
off ISIG, and ⌃C would arrive as a byte instead of a SIGINT. A system with no `stty`, such as
Windows, has no host as far as this library is concerned.

## Versions

This release speaks version 1 of the pencere wire, which is still a draft: until the specification
publishes a dated version, a `0.x` release of this package may change the wire along with it. The
package follows semantic versioning; [CHANGELOG.md](CHANGELOG.md) says what each release changed.

## License

Apache-2.0 — see [LICENSE](LICENSE).
