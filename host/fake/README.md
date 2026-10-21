# pencere-fake-host

A stand-in terminal for the authors of tools and for tests. It runs a program on
a pty and plays the host of the views that program opens, with the
[`pencere` crate](../rust) holding every rule of the conversation: who may open a
view, what is refused, which files of a bundle are missing, what the view is told,
what ends it. The view is a script, not a web view, so a tool can be tried, and the
SDK tested, without a terminal that hosts pencere.

```sh
cargo run -p pencere-fake-host -- [options] -- <program> [args...]
```

What happens is written to stderr, one line each, in the order it happens:

```
host: sees the tty: canonical=false echo=false
host: welcome id=1 re=1 version=1 with a token
host: answers CSI c
screen: alternate screen on
program: open id=2 tool=list_containers bundle of 3 files
host: need id=2 re=2 missing=3
...
view: -> tools/call stop_container {"id":"cache"}
host: the person presses ^C
program: bye id=10 reason=signal
view: destroyed
screen: alternate screen off
```

`program:` is what the program wrote and how it ended, `host:` what the host decided
and wrote into the program's input (and the question it asks the person when a view declares
domains, which names the tool), `view:` what the scripted view received and sent
(and the tool's title and the program's name the view starts with, which a terminal
shows above it), `screen:` the alternate screen. Tokens never appear.

Run it from a terminal and the program's keys are yours: ⌃C reaches the program as it
does in any terminal. Otherwise the script decides how the run ends (`--then`): the view
makes the calls it was given (`--call`), and then ^C is typed into the pane, the program
is killed with SIGKILL, the terminal goes away as when an ssh connection drops (the program is hung up
and its tty fails), the person closes the view, the host starts the view again once, as after a
reconnection, or nothing happens. A program that shows several views, as `pencere dev` does,
takes the action at the Nth view (`--on-view`). `--no-host` plays
a terminal that does not host pencere, `--store` keeps verified files between runs, and
`--help` lists the rest.

It is not a terminal emulator: it draws nothing, tracks only the alternate screen and
answers only `CSI c`, and it keeps the pane's size as it was when the program started.
The end-to-end tests in `tests/e2e.rs` run the example tool against it.
