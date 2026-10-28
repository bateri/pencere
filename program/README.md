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

A program laid out as `pencere new --lang rust` lays it out keeps its page in `src/view/` and its
pictures in `src/assets/`, and does not list them: its build script carries both into the binary
(`pencere::build::app()` in `build.rs`), and `Options::app(pencere::app!())` offers what they are,
by the rules of a Lua app: the page of the kit (`view/page.lua`, with the scripts it `require`s and
the pictures) if there is one, then the web page (`view/index.html`) if there is one. Under
`pencere dev` the program is told where the project is (`PENCERE_DEV_APP`) and reads them from
there instead, and the view is shown again, without ending the program, whenever one changes.

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
not defined. Those that must not overtake one another go in a lane:
`Options::call_in("edits", "edit", handler)`. The calls of one lane run one at a time, in the order
the view sent them, each once the one before it has been answered, while other lanes and the
calls of `Options::call` go on; a call the view cancels before its turn does not run, and a
handler that panics fails its own call and the lane goes on. A lane lasts as long as `serve`, so a
call of a view shown again waits for a handler of the view before it that is still running. (The
example takes the pieces of a payload the view sends in a lane, and the last one, which asks how
much there was, comes after the others.)

A program with a loop of its own — an interpreter, an event loop — takes the requests in the order
the view made them and answers each when it can: `Options::call_deferred(method, handler)` and
`Options::topic_deferred(topic, handler)` give the handler each call or subscription on the thread
that reads the view, in order, with an `Answer` to send from any thread later. The handler hands it
to the loop and returns at once; none waits for another's answer, and the loop sees them in the
order they were made.

```rust
use pencere::{Answer, Call, Options};
use serde_json::{Value, json};
use std::sync::mpsc::channel;
use std::sync::Mutex;

let (to_loop, from_view) = channel::<(Value, Answer)>();
let to_loop = Mutex::new(to_loop);
let options = Options::new().call_deferred("edit", move |params: Value, _call: Call, answer: Answer| {
    let _ = to_loop.lock().unwrap().send((params, answer));
});
// The program's loop, elsewhere: each edit in the order the view sent it.
std::thread::spawn(move || {
    for (params, answer) in from_view {
        answer.send(Ok(json!({ "version": params["version"] })));
    }
});
# drop(options);
```

Anything a handler returns is JSON: the result of a call is `Serialize`, an error is a
`ViewError` with a short `code` the view can act on and words for the person, and a handler that
panics is an `internal` error of its call and nothing else.

## Closing the view and showing another

A program that goes on after its view, or whose author reloads the view as the files change, makes
a `ViewControl`, gives it to `Options::control` and calls it from another thread while `serve`
runs. `reopen(None)` closes the view and shows it again; `reopen(Some(views))` shows other files
or other kinds (the terminal keeps what it verified, so only what changed crosses the stream);
`close()` closes the view and makes `serve` return `EndedBy::Program`, with the terminal given back
first. Both need a terminal that lists `dismiss` in its `welcome`; with another they answer
`Unsupported` and the view is untouched.

## Lending the terminal to a command

A program that runs a shell or an editor in its pane lends it the terminal:
`control.hand_off(command)` closes the view, gives the terminal back as at the end (the screen,
the modes), runs the command as the terminal's foreground job, in a process group of its own, as a
shell runs one, and when it ends takes the terminal back and shows the view again, `serve` going
on with the same handlers. `hand_off` returns a `HandOff`; its `wait` gives how the command ended
(`HandOffResult::Returned`) once the view is back, or why the view did not come back
(`HandOffResult::NoView`, and `serve` ends with it). What was on its way to the view goes out
before it closes, and a handler that asks for a hand-off is waited for, so that its own answer
does too:

```rust,no_run
use std::process::Command;
use std::sync::{Arc, Mutex};

use pencere::{Options, ViewControl};
use serde_json::json;

let control = ViewControl::new();
let lender = control.clone();
// How the shell ended, for the view that comes back to ask.
let last = Arc::new(Mutex::new(None));
let kept = Arc::clone(&last);
let options = Options::new()
    .control(control)
    .call("shell", move |_params, _call| {
        let kept = Arc::clone(&kept);
        // Kept before the view is shown again; the view closes once this answer is out.
        let _ = lender.hand_off_then(Command::new("bash"), move |ended| {
            *kept.lock().unwrap() = ended.as_ref().ok().and_then(|status| status.code());
        });
        Ok(json!("starting"))
    })
    .call("last", move |_params, _call| Ok(json!({ "code": *last.lock().unwrap() })));
```

`hand_off_then` gives its function how the command ended (or why it could not be started) once it
has, on the library's thread, **before the view is asked for again**: what it keeps is there for
the first call of the view that comes back, which need wait for nothing. It should be brief (keep
a value, send it on a channel); a panic in it is caught. `hand_off` is the same with nothing to
call, and the `HandOff` of either says how it went: its `wait` returns as the terminal takes the
open of the view shown again, or why the view did not come back (`HandOffResult::NoView`).

A signal that asks the program to end while the command has the terminal is acted on once the
command has ended (a hangup is passed on to it, as a shell passes its own on); `serve` then ends as
for that signal. With no view up — before `serve`, after it, in its fallback — `hand_off` runs the
command in the foreground all the same.

## Without a terminal

`Conversation::new(options)` serves the same handlers with no terminal and no pty, in the
program's own process: `send` gives them a message of the view (the JSON text of a `call`, a
`subscribe` or a `cancel`), and `receive(within)` or `try_receive()` takes what the program says
back. Everything between is `serve`'s, by the same code: each call on a thread of its own or of its
lane, the subscriptions and their cleanups, the limit of a message, a panic as the `internal` error
of its call, and the pace of the reader, which a handler that pushes faster than it is read waits
for. It is how a program's tests try its handlers by the rules they run by, and how a host that
runs the page in the same process talks to the program.

```rust
use pencere::{Conversation, Options, Said};
use serde_json::json;
use std::time::Duration;

let mut conversation = Conversation::new(
    Options::new().call_in("edits", "edit", |params, _call| Ok(json!({ "version": params["version"] }))),
);
conversation.send(r#"{"call":{"id":1,"method":"edit","params":{"version":3}}}"#);
match conversation.receive(Duration::from_secs(5)) {
    Some(Said::Message(text)) => assert_eq!(text, r#"{"response":{"id":1,"result":{"version":3}}}"#),
    other => panic!("{other:?}"),
}
```

A conversation is a terminal that can close a view and answers at once: what the program asks of
its `ViewControl` is what it says, `Said::Closed`, `Said::Reopened` and `Said::HandedOff`. No
command runs; the `Lent` of a hand-off says how it ended (`lent.returned(status)`), and until then
there is no view to hear.

To test the handlers with the page they serve, `pencere-drive`'s host with no screen runs the page
in the kit and serves it the handlers by a conversation (`headless::InProcess`, with the crate's
feature `program`), with no terminal and no pty between them.

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

The protocol is [the specification](https://github.com/bateri/pencere/blob/main/spec/draft/pencere.md),
still a draft; the wire is the crate `pencere-wire`. [CHANGELOG.md](CHANGELOG.md) says what each
release changed.

## License

Apache-2.0 — see [LICENSE](LICENSE).
