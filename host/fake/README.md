# pencere-fake-host

A stand-in terminal for the authors of programs and for tests. It runs a program on
a pty and plays the host of the views that program opens, with the
[`pencere-host` crate](../rust) holding every rule of the conversation: who may open a
view, what is refused, which files of a bundle are missing, what the page is told,
what ends it. The page is a script, not a web view, so a program can be tried, and the
libraries tested, without a terminal that hosts pencere. It opens the three kinds of view the
protocol has: a page (`web`) and a component or a Lua script that a terminal draws itself (`wasm`
and `lua`, at 1.0); it draws neither, and plays all three the same way.

```sh
cargo run -p pencere-fake-host -- [options] -- <program> [args...]
```

What happens is written to stderr, one line each, in the order it happens:

```
host: sees the tty: canonical=false echo=false
host: welcome id=1 re=1 version=1 with a token features=bundle,dismiss views=web@1.0
host: answers CSI c
screen: alternate screen on
program: open id=2 view=web version=1.0 title="Containers" bundle of 3 files
host: need id=2 re=2 missing=3
...
view: title="Containers" program="containers"
view: <- context mode=dark size=640x384 reduceMotion=false access=none
view: -> call id=1 list
program: msg id=6 response=1 result
view: <- response id=1 result={"containers":[{"id":"web","image":"nginx:1.27", ... (216 bytes)
host: the person presses ^C
program: bye id=8 reason=signal
view: destroyed
screen: alternate screen off
host: sees the tty after the screen: canonical=true echo=true
```

`program:` is what the program wrote and how it ended, `host:` what the host decided
and wrote into the program's input (and the question it asks the person when a page
asks to reach an origin or a capability, which names the program), `view:` what the
scripted page received and sent (and the program's words for the view and for itself
that the view starts with, which a terminal shows above it), `screen:` the alternate
screen. The modes of the tty are noted when the program asks (`sees the tty`) and again when it
writes something after it gave the screen back, which is where a program that gives the tty back
as it found it shows it has. Tokens never appear.

The page is told its `context` first, as a page that has loaded is, and then sends
what it was given on the command line: the topics it subscribes to (`--subscribe`),
the links it asks the host to open (`--link`, which the stand-in person refuses) and,
once each topic has pushed, the calls it makes (`--call`), one after the other. It logs
the `response`, `push` and `progress` messages the program answers with, and cancels
(`--cancel-after`) when it is told to.

Run it from a terminal and the program's keys are yours: ⌃C reaches the program as it
does in any terminal. Otherwise the script decides how the run ends (`--then`): when
the page has what it waited for, ^C is typed into the pane, the program is killed with
SIGKILL, the terminal goes away as when an ssh connection drops (the program is hung up
and its tty fails), the person closes the view, the page asks to be closed, the host starts
the view again once, as after a reconnection, or nothing happens. A program that shows
several views, as one that reloads its view does, takes the action at the Nth view
(`--on-view`). `--no-host` plays a terminal that does not host pencere, `--allow-network`
answers yes to the question a page's `access` raises, `--store` keeps verified files
between runs, and `--help` lists the rest.

It is not a terminal emulator: it draws nothing, tracks only the alternate screen and
answers only `CSI c`, and it keeps the pane's size as it was when the program started.
The end-to-end tests in `tests/e2e.rs` run the small programs of `tests/programs`,
written against the TypeScript SDK, and the example written against the Rust library
(`examples/containers-rust`), against it.
