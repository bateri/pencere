# pencere

Show a view in the pane of the terminal your program runs in.

[pencere](https://github.com/bateri/pencere) is a protocol for programs that run in a terminal to
ask it for a real interface — a web page, or a WebAssembly component or a Lua script that the
terminal draws itself — that takes the pane the way `vim` takes it and gives it back when it is
done. This crate is the program's side: it asks the terminal what it can show, opens the view,
sends the files the terminal lacks, answers the calls of the view and feeds its subscriptions, and
leaves the terminal as it found it. There is no async runtime in it.

```sh
cargo add pencere serde_json
```

```rust,no_run
use pencere::{Bundle, Options, View, ViewError};
use serde_json::json;

fn main() -> Result<(), pencere::Error> {
    // The files of the view: a directory, or a component or a script the program carries.
    let page = Bundle::from_dir("view", "index.html")?;

    let outcome = pencere::serve(
        Options::new()
            .view(View::web(page))
            .program("containers")
            .title("Containers")
            // What the view may ask the program to do ...
            .call("stop", |params, _call| match params["id"].as_str() {
                Some(id) => Ok(json!({ "stopped": id })),
                None => Err(ViewError::with_message("bad-params", "which container?")),
            })
            // ... and what it may listen to.
            .topic("stats", |_params, subscription| {
                let feed = subscription.clone();
                std::thread::spawn(move || {
                    while feed.push(json!({ "cpu": 3 })) && !feed.sleep(std::time::Duration::from_secs(1)) {}
                });
                Ok(())
            })
            // The program's own interface, for a terminal that shows no views.
            .fallback(|_why| println!("web 8080->80, running")),
    )?;
    std::process::exit(outcome.shell_status().into())
}
```

Run in a terminal that hosts pencere, the view opens in the pane; run anywhere else — a pipe, a
terminal that does not know the protocol — and `serve` returns after the fallback has run. ⌃C
ends the view and the shell is where it was.

To try a program without a terminal that hosts pencere, run it under the stand-in terminal of this
repository, which plays the host and writes the conversation to stderr
(`cargo run -p pencere-fake-host -- --subscribe stats --then interrupt -- ./your-program`).

## What the library does so that your program does not

- **The handshake**: a `hello` and the fence `CSI c`; an answer within two seconds or no host; a
  late answer to the fence read and thrown away, so that it does not reach the shell as typed
  text.
- **The modes of the tty**: non-canonical, echo off, signals on, the input read without ever
  waiting. Given back on every path out, a panic and `std::process::exit` included.
- **The signals**: ⌃C, `SIGTERM`, `SIGHUP` and ⌃\ end the view with a `bye`, wait up to half a
  second for the terminal's `closed`, throw away what is left unread and give the terminal back.
  ⌃Z gives the terminal back, stops the program, and takes it again when it is continued. `serve`
  then returns, [`EndedBy::Signal`] saying which; what the program does next is its own, and
  `Outcome::shell_status` is the status a shell reports for it (130 for ⌃C).
- **The writes**: one thread writes, a few bytes at a time while the tty has room, so a terminal
  that is not reading cannot keep the program from reading or from leaving.
- **The alternate screen**: entered before the view is opened, left on the way out.

## Views

A program offers the kinds of view it can show, in the order it prefers them, each with the
versions of its interface it is written against: `View::web(bundle)` for a page, and
`View::new(Kind::Wasm, &["1.0"], bundle)` or `Kind::Lua` for a view the terminal draws itself. The
first kind the terminal will open, at the first of the versions the terminal speaks, is the view.
`Bundle::single("view.wasm", include_bytes!(…))` is the bundle of a component or a script the
program carries; `Bundle::from_dir` and `Bundle::new` build a page's. A view with a bundle for each
version is `View::per_version`.

## Calls and subscriptions

A handler runs on a thread of its own, so a call that takes a while neither keeps ⌃C from ending
the program nor holds the other calls up. It looks at `Call::is_cancelled` (or sleeps with
`Call::sleep`, which returns when the view cancels) and reports how far it has got with
`Call::progress`. A subscription's handler starts whatever feeds it and returns; the feed pushes
with `Subscription::push` from any thread, learns that the view stopped listening from
`is_cancelled` or `sleep`, and can register what stops it with `on_end`. `push` waits while the
terminal is catching up, which is what keeps a feed that is faster than the terminal from filling
the memory.

Calls run at the same time, so which of two calls that arrive together is answered first is
not defined. A program that needs them in order, or needs the last of several to know about the
others, says so itself (the example sums a payload the view sends in pieces, and its last piece
waits for the rest).

Anything a handler returns is JSON: the result of a call is `Serialize`, an error is a
`ViewError` with a short `code` the view can act on and words for the person, and a handler that
panics is an `internal` error of its call and nothing else.

## When there is no view

`serve` returns `Outcome::NoView` — after the fallback has run, and with the terminal as it was
found — when the input or the output is not a terminal, when nothing answers, when the terminal
shares no version of the protocol or no kind of view with the program, or when it refuses or ends
the open before the view runs. A program in a pipeline, whose standard streams carry its data,
shows its view on the controlling terminal instead: `Options::terminal(Terminal::controlling()?)`,
chosen by a flag of its own and never guessed from the standard streams.

## Things to know

- The terminal is the view's while it is up: the program must not print.
- A process has one terminal: a second `serve` while the first runs is `Error::Busy`.
- The signal handlers are installed for the time of `serve`; handlers of calls and subscriptions
  run on threads that do not take signals.
- A handler that does not look at `is_cancelled` is not stopped when the view ends; it keeps
  its thread until it returns, and the result is dropped.
- Not in this version: closing the view and showing another while the program runs (`dismiss`).

The protocol is [the specification](https://github.com/bateri/pencere/blob/main/spec/draft/pencere.md),
still a draft; the wire is the crate `pencere-wire`. [CHANGELOG.md](CHANGELOG.md) says what each
release changed.

## License

Apache-2.0 — see [LICENSE](LICENSE).
