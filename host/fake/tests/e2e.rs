//! End to end, on a real pty: small programs written against the TypeScript SDK
//! (`tests/programs`, which import it from its build, made by `npx tsc -b`, which the
//! gate runs before these), and the example written against the Rust library
//! (`examples/containers-rust`, built by cargo), against the stand-in host, which holds the
//! `pencere-host` crate's rules and a scripted page.
//!
//! The example (`examples/containers`) runs on each runtime the SDK supports: Node, Deno and
//! Bun, and as the single executable `deno compile` makes of it. A runtime that is not installed
//! has its tests print `SKIPPED` and pass (`PENCERE_DENO` and `PENCERE_BUN` name an executable
//! that is not on the `PATH`; `cargo test -- --nocapture` shows the lines).
//!
//! What is read here is the record the host writes to stderr, the bytes the program wrote
//! to its tty, and how the processes ended. Nothing goes to a network, and every file
//! written is under the target directory.

// A test fails by panicking, and the helpers here are the test's.
#![allow(clippy::expect_used, clippy::panic)]

use std::fs;
use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};
use std::sync::OnceLock;
use std::sync::mpsc::channel;
use std::time::Duration;

const FAKE_HOST: &str = env!("CARGO_BIN_EXE_pencere-fake-host");
const TMP: &str = env!("CARGO_TARGET_TMPDIR");

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn programs() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/programs")
}

/// A program of `tests/programs`, after a check that the SDK it imports has been built.
fn program(name: &str) -> String {
    let sdk = root().join("sdk/typescript/dist/index.js");
    assert!(
        sdk.is_file(),
        "{} is missing: build the TypeScript half first with `npx tsc -b` (make check does)",
        sdk.display()
    );
    programs().join(name).to_string_lossy().into_owned()
}

/// A directory of the test's own under the target directory, empty.
fn fresh(name: &str) -> PathBuf {
    let dir = Path::new(TMP).join(format!("e2e-{name}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("a directory under the target directory");
    dir
}

/// Whether `event` has the parts of `pattern`, split at `*`, one after the other.
fn matches(event: &str, pattern: &str) -> bool {
    let mut rest = event;
    for part in pattern.split('*') {
        match rest.find(part) {
            Some(at) => rest = rest.get(at + part.len()..).unwrap_or(""),
            None => return false,
        }
    }
    true
}

struct Run {
    /// The record, one line per event.
    events: Vec<String>,
    /// What the program wrote to its tty.
    output: Vec<u8>,
    status: ExitStatus,
}

impl Run {
    fn text(&self) -> String {
        self.events.join("\n")
    }

    /// Each of `wanted` occurs in an event, in this order. A `*` in a pattern
    /// stands for whatever the event has there (the ids of messages, which no
    /// test is about).
    fn in_order(&self, wanted: &[&str]) {
        let mut at = 0;
        for want in wanted {
            let found = self
                .events
                .get(at..)
                .and_then(|rest| rest.iter().position(|event| matches(event, want)));
            match found {
                Some(index) => at += index + 1,
                None => panic!(
                    "no event with {want:?} after event {at}; the record:\n{}",
                    self.text()
                ),
            }
        }
    }

    fn never(&self, unwanted: &str) {
        assert!(
            !self.events.iter().any(|event| event.contains(unwanted)),
            "an event with {unwanted:?} where there should be none; the record:\n{}",
            self.text()
        );
    }

    fn count(&self, text: &str) -> usize {
        self.events.iter().filter(|e| e.contains(text)).count()
    }

    fn output_has(&self, text: &str) -> Option<usize> {
        String::from_utf8_lossy(&self.output).find(text)
    }

    /// What the program said on the screen it was given back: where it tells how the
    /// view ended, and what it did when it was done.
    fn said_after_the_screen(&self, line: &str) {
        let off = self
            .output_has("\x1b[?1049l")
            .expect("it gave the screen back");
        let at = self
            .output_has(line)
            .unwrap_or_else(|| panic!("no {line:?}:\n{}", self.text()));
        assert!(off < at, "{line:?} came before the screen was given back");
    }
}

fn run(options: &[&str], program: &[&str]) -> Run {
    let output = Command::new(FAKE_HOST)
        .args(["--limit", "30", "--passthrough"])
        .args(options)
        .arg("--")
        .args(program)
        .stdin(Stdio::null())
        .output()
        .expect("the fake host starts");
    let events = String::from_utf8_lossy(&output.stderr)
        .lines()
        .map(str::to_owned)
        .collect();
    Run {
        events,
        output: output.stdout,
        status: output.status,
    }
}

/// The program named, run in the pane of the stand-in host.
fn run_program(name: &str, options: &[&str]) -> Run {
    run(options, &["node", &program(name)])
}

fn run_containers(options: &[&str]) -> Run {
    run_program("containers.mjs", options)
}

/// A JavaScript runtime the SDK runs on.
#[derive(Clone, Copy, Debug)]
enum Runtime {
    Node,
    Deno,
    Bun,
}

/// A runtime that is installed, and where.
struct Installed {
    runtime: Runtime,
    executable: PathBuf,
}

impl Runtime {
    fn name(self) -> &'static str {
        match self {
            Self::Node => "node",
            Self::Deno => "deno",
            Self::Bun => "bun",
        }
    }

    /// Where the runtime is: what `PENCERE_DENO` or `PENCERE_BUN` names, else the `PATH`, and for
    /// Bun the place its installer puts it. Node is the one that runs the gate's own tools.
    fn find(self) -> Option<Installed> {
        let var = format!("PENCERE_{}", self.name().to_uppercase());
        let named = std::env::var_os(var).filter(|value| !value.is_empty());
        let on_path = || {
            std::env::split_paths(&std::env::var_os("PATH")?)
                .map(|dir| dir.join(self.name()))
                .find(|candidate| candidate.is_file())
        };
        let installed = || {
            let home = PathBuf::from(std::env::var_os("HOME")?);
            Some(home.join(".bun/bin/bun")).filter(|path| path.is_file())
        };
        let executable = match (named, self) {
            (Some(named), _) => Some(PathBuf::from(named)),
            (None, Self::Bun) => on_path().or_else(installed),
            (None, _) => on_path(),
        }?;
        Some(Installed {
            runtime: self,
            executable,
        })
    }
}

impl Installed {
    /// The command that runs the script at `script`. Deno is given the permissions the SDK needs
    /// and no more: to read the files of the view, and to run `stty` and to signal itself.
    fn command(&self, script: &Path) -> Vec<String> {
        self.command_with(script, false)
    }

    /// The command for a filter. Deno asks for all permissions before it lets a program open
    /// `/dev/tty` to write, which a filter does for the terminal it asks on.
    fn filter_command(&self, script: &Path) -> Vec<String> {
        self.command_with(script, true)
    }

    fn command_with(&self, script: &Path, filter: bool) -> Vec<String> {
        let executable = self.executable.to_string_lossy().into_owned();
        let script = script.to_string_lossy().into_owned();
        match (self.runtime, filter) {
            (Runtime::Deno, false) => vec![
                executable,
                "run".into(),
                "--allow-read".into(),
                "--allow-run".into(),
                script,
            ],
            (Runtime::Deno, true) => vec![executable, "run".into(), "--allow-all".into(), script],
            (Runtime::Node | Runtime::Bun, _) => vec![executable, script],
        }
    }
}

/// Runs `test` with the runtime if it is installed, and says `SKIPPED` if it is not.
fn on(runtime: Runtime, test: fn(&Installed)) {
    match runtime.find() {
        Some(installed) => test(&installed),
        None => println!("SKIPPED: {} is not installed", runtime.name()),
    }
}

/// `$test` on each runtime, as three tests (`$test::on_node`, `::on_deno` and `::on_bun`), so that a
/// failure says which runtime it was.
macro_rules! on_each_runtime {
    ($test:ident) => {
        mod $test {
            #[test]
            fn on_node() {
                super::on(super::Runtime::Node, super::$test);
            }
            #[test]
            fn on_deno() {
                super::on(super::Runtime::Deno, super::$test);
            }
            #[test]
            fn on_bun() {
                super::on(super::Runtime::Bun, super::$test);
            }
        }
    };
}

/// The built containers example (`examples/containers`, made by `npx tsc -b`).
fn example_main() -> PathBuf {
    let main = root().join("examples/containers/dist/main.js");
    assert!(
        main.is_file(),
        "{} is missing: build the TypeScript half first with `npx tsc -b` (make check does)",
        main.display()
    );
    main
}

/// The containers example, run on `on` in the pane.
fn run_example(on: &Installed, options: &[&str]) -> Run {
    let command = on.command(&example_main());
    let command: Vec<&str> = command.iter().map(String::as_str).collect();
    run(options, &command)
}

fn view_file_size(name: &str) -> u64 {
    fs::metadata(programs().join("view").join(name))
        .expect("a file of the view")
        .len()
}

// ---- a program with a view ------------------------------------------------------

#[test]
fn a_program_goes_from_the_capability_query_to_the_interrupt() {
    let store = fresh("first");
    let store = store.to_string_lossy();
    let run = run_containers(&[
        "--store",
        &store,
        "--call",
        "list",
        "--call",
        r#"stop={"id":"cache"}"#,
        "--then",
        "interrupt",
    ]);
    assert!(run.status.success(), "{}", run.text());
    run.in_order(&[
        "program: hello id=1",
        // The host looked at the tty before it answered (§6.2): the SDK had made
        // it non-canonical with echo off.
        "host: sees the tty: canonical=false echo=false",
        "host: welcome id=1 re=1 version=1 with a token features=dismiss views=web@1.0",
        // The answer to the question comes before the reply to `CSI c` (§5.2).
        "host: answers CSI c",
        "screen: alternate screen on",
        r#"program: open id=2 view=web version=1.0 title="Containers" bundle of 3 files"#,
        "host: need id=2 re=2 missing=3",
        "program: file",
        "program: file",
        "program: file",
        "view: started from the bundle: entry index.html, 3 files",
        "view: kind=web version=1.0",
        // What the strip above the view says: the program's words for the view and for
        // itself, and its icon, a PNG in a `data:` URI.
        r#"view: title="Containers" program="containers""#,
        "view: icons png 64x64",
        "view: policy default-src 'none'",
        // The page has loaded: the host told it where it is before it said anything.
        "view: <- context mode=dark size=640x384 reduceMotion=false access=none",
        "view: -> call id=1 list",
        "host: msg id=* call=1 list",
        "program: msg id=* response=1 result",
        "view: <- response id=1 result=",
        r#"view: -> call id=2 stop {"id":"cache"}"#,
        "host: msg id=* call=2 stop",
        r#"view: <- response id=2 result={"stopped":"cache"}"#,
        "host: the person presses ^C",
        "program: bye id=* reason=signal",
        "view: destroyed",
        // The host's last word for the view, and the one frame it writes for a program
        // that has said it ended (§10.3).
        "host: closed id=* re=* reason=program",
        "screen: alternate screen off",
        "program: was killed by signal 2",
    ]);
    assert_eq!(run.count("program: file"), 3, "{}", run.text());

    // The files the view would fetch are the files on disk, whole.
    for (name, media_type) in [
        ("index.html", "text/html"),
        ("app.js", "text/javascript"),
        ("style.css", "text/css"),
    ] {
        let wanted = format!(
            "view: fetched {name} {media_type} {} bytes",
            view_file_size(name)
        );
        assert!(
            run.events.contains(&wanted),
            "no {wanted:?}:\n{}",
            run.text()
        );
    }
}

#[test]
fn a_call_that_fails_and_a_name_nobody_serves_are_answered_with_an_error() {
    let run = run_containers(&[
        "--subscribe",
        "no-such-topic",
        "--call",
        r#"stop={"id":"nobody"}"#,
        "--call",
        "no-such-method",
        "--then",
        "interrupt",
    ]);
    assert!(run.status.success(), "{}", run.text());
    run.in_order(&[
        "view: -> subscribe id=1 no-such-topic",
        // A topic that is not served is answered at once, and does not hold the script.
        "view: <- response id=1 error code=not-found",
        r#"view: -> call id=2 stop {"id":"nobody"}"#,
        r#"view: <- response id=2 error code=no-such-container message="There is no container \"nobody\"."#,
        "view: -> call id=3 no-such-method",
        "program: msg id=* response=3 error",
        "view: <- response id=3 error code=not-found",
        "host: the person presses ^C",
        "program: bye id=* reason=signal",
    ]);
}

#[test]
fn a_call_that_reports_progress_is_followed_to_its_answer() {
    let run = run_containers(&[
        "--call",
        r#"slow={"steps":3,"ms":30}"#,
        "--then",
        "interrupt",
    ]);
    assert!(run.status.success(), "{}", run.text());
    run.in_order(&[
        "view: -> call id=1 slow",
        "host: msg id=* call=1 slow",
        "program: msg id=* progress=1",
        r#"view: <- progress id=1 done=1 total=3 note="step 1 of 3""#,
        "view: <- progress id=1 done=2 total=3",
        "view: <- progress id=1 done=3 total=3",
        r#"view: <- response id=1 result={"steps":3}"#,
        "host: the person presses ^C",
        "program: bye id=* reason=signal",
    ]);
    run.never("-> cancel");
}

#[test]
fn a_call_the_page_cancels_is_stopped_while_the_view_runs() {
    // The page gives the call up at its first report and goes on with the next call, which
    // asks the program how many it has stopped: the answer comes while the view is up.
    let run = run_containers(&[
        "--call",
        r#"slow={"steps":5,"ms":300}"#,
        "--call",
        "counts",
        "--cancel-after",
        "1",
        "--then",
        "interrupt",
    ]);
    assert!(run.status.success(), "{}", run.text());
    run.in_order(&[
        "view: -> call id=1 slow",
        "view: <- progress id=1 done=1 total=5",
        "view: -> cancel id=1",
        "view: -> call id=2 counts",
        "host: msg id=* cancel=1",
        "host: msg id=* call=2 counts",
        r#"view: <- response id=2 result={"cancelledCalls":1,"endedTopics":0}"#,
        "host: the person presses ^C",
        "program: bye id=* reason=signal",
    ]);
    // It stopped where it was; nothing more of it reached the page, and it did not finish.
    run.never("view: <- progress id=1 done=2");
    run.never("view: <- response id=1");
    assert!(
        run.output_has("slow: cancelled after 1 of 5").is_some(),
        "{}",
        run.text()
    );
    assert!(run.output_has("slow: finished").is_none());
}

#[test]
fn a_topic_pushes_until_the_page_cancels_it() {
    let run = run_containers(&[
        "--subscribe",
        r#"ticks={"every":10}"#,
        "--call",
        "counts",
        "--cancel-after",
        "2",
        "--then",
        "interrupt",
    ]);
    assert!(run.status.success(), "{}", run.text());
    run.in_order(&[
        r#"view: -> subscribe id=1 ticks {"every":10}"#,
        "host: msg id=* subscribe=1 ticks",
        "program: msg id=* push=1",
        r#"view: <- push id=1 value={"n":1}"#,
        r#"view: <- push id=1 value={"n":2}"#,
        "view: -> cancel id=1",
        // The calls wait for the topic, so the answer says what the cancel did.
        "view: -> call id=2 counts",
        "host: msg id=* cancel=1",
        r#"view: <- response id=2 result={"cancelledCalls":0,"endedTopics":1}"#,
        "host: the person presses ^C",
        "program: bye id=* reason=signal",
    ]);
    assert!(
        run.output_has("ticks: stopped after").is_some(),
        "the program did not clean up:\n{}",
        run.text()
    );
}

#[test]
fn a_topic_the_page_does_not_cancel_stops_when_the_view_is_over() {
    let run = run_containers(&["--subscribe", "ticks", "--then", "interrupt"]);
    assert!(run.status.success(), "{}", run.text());
    run.in_order(&[
        "view: -> subscribe id=1 ticks",
        r#"view: <- push id=1 value={"n":1}"#,
        // The script was done at the first push, and the program was interrupted.
        "host: the person presses ^C",
        "program: bye id=* reason=signal",
        "view: destroyed",
    ]);
    run.never("-> cancel");
    assert!(
        run.output_has("ticks: stopped after").is_some(),
        "the program did not clean up:\n{}",
        run.text()
    );
}

#[test]
fn a_message_far_larger_than_the_ttys_input_queue_gets_through() {
    // The tty takes about a kilobyte of input at a time (specification Appendix B):
    // the host writes the message's frames as the tty takes them, none inside
    // another, and the program's answer comes back in frames of its own.
    let text = "a".repeat(60_000);
    let call = format!(r#"echo={{"text":"{text}"}}"#);
    let run = run_containers(&["--call", &call, "--then", "interrupt"]);
    assert!(run.status.success(), "{}", run.text());
    run.in_order(&[
        "view: -> call id=1 echo",
        "host: msg id=* call=1 echo",
        "program: msg id=* response=1 result",
        r#"view: <- response id=1 result={"text":"aaaa"#,
        "program: bye id=* reason=signal",
    ]);
    // The record cuts what it shows and says how long it was: the whole message went each way.
    assert_eq!(run.count("(60011 bytes)"), 2, "{}", run.text());
    run.never("dropped a frame");
}

#[test]
fn what_the_program_writes_to_the_screen_among_its_frames_gets_through() {
    let run = run_containers(&[
        "--call",
        r#"say={"text":"from the program","lines":20}"#,
        "--then",
        "close",
    ]);
    assert!(run.status.success(), "{}", run.text());
    run.in_order(&[
        "view: -> call id=1 say",
        "view: <- progress id=1 done=1 total=20",
        "view: <- progress id=1 done=20 total=20",
        r#"view: <- response id=1 result={"written":20}"#,
        "host: the person closes the view",
    ]);
    assert_eq!(run.count("view: <- progress id=1"), 20, "{}", run.text());
    for line in [1, 2, 20] {
        // The tty ends a line with a return of its own, so only the first is asked for.
        let text = format!("say: from the program {line}\r");
        assert!(run.output_has(&text).is_some(), "no {text:?}");
    }
    run.never("dropped a frame");
}

// ---- how a view ends -------------------------------------------------------------

#[test]
fn a_second_run_finds_the_bundle_in_the_store_and_the_person_can_close_the_view() {
    let store = fresh("second");
    let store = store.to_string_lossy();
    let first = run_containers(&["--store", &store, "--then", "close"]);
    first.in_order(&["host: need id=2 re=2 missing=3", "program: file"]);

    let second = run_containers(&["--store", &store, "--then", "close"]);
    assert!(second.status.success(), "{}", second.text());
    second.in_order(&[
        "host: need id=2 re=2 missing=0",
        "view: started from the bundle: entry index.html, 3 files",
        "view: fetched app.js",
        "view: <- context",
        "host: the person closes the view",
        "view: destroyed",
        "host: closed id=* reason=user",
        "program: exited with status 0",
    ]);
    // No file crossed the stream the second time.
    second.never("program: file");

    // The program took the alternate screen before it opened the view and gave
    // it back before it said anything on the screen it returned to.
    let on = second
        .output_has("\x1b[?1049h")
        .expect("it took the alternate screen");
    let open = second.output_has(";open;").expect("it opened the view");
    let off = second
        .output_has("\x1b[?1049l")
        .expect("it gave the screen back");
    let farewell = second
        .output_has(r#"serve: {"kind":"closed","by":"user""#)
        .expect("it said how the view ended");
    assert!(
        on < open && open < off && off < farewell,
        "{on} {open} {off} {farewell}"
    );
}

#[test]
fn only_the_files_the_store_lacks_cross_the_stream() {
    let dir = fresh("partial");
    let store = dir.to_string_lossy();
    let first = run_containers(&["--store", &store, "--then", "close"]);
    assert!(first.status.success(), "{}", first.text());

    // The store loses one file: the first one the program sent.
    let sent = first
        .events
        .iter()
        .find(|event| event.starts_with("program: file id="))
        .and_then(|event| event.split_whitespace().last())
        .expect("the first run sent files");
    let lost: Vec<PathBuf> = fs::read_dir(&dir)
        .expect("the store")
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .is_some_and(|name| name.to_string_lossy().starts_with(sent))
        })
        .collect();
    assert_eq!(lost.len(), 1, "{lost:?}");
    fs::remove_file(&lost[0]).expect("a file of the store");

    let second = run_containers(&["--store", &store, "--then", "close"]);
    assert!(second.status.success(), "{}", second.text());
    second.in_order(&[
        "host: need id=2 re=2 missing=1",
        &format!("program: file id=3 {sent}"),
        "view: started from the bundle: entry index.html, 3 files",
    ]);
    assert_eq!(second.count("program: file"), 1, "{}", second.text());
    // The three files are served, the lost one from what was just sent.
    assert_eq!(second.count("view: fetched"), 3, "{}", second.text());
}

#[test]
fn a_page_that_asks_to_be_closed_is_closed_and_the_program_is_told_so() {
    let run = run_containers(&["--call", "list", "--then", "page-close"]);
    assert!(run.status.success(), "{}", run.text());
    run.in_order(&[
        "view: <- response id=1 result=",
        "view: -> close",
        "view: destroyed",
        // The page asked, the program is told and decides: this one ends its wait.
        "host: closed id=* reason=view",
        "program: exited with status 0",
    ]);
    run.never("program: bye");
    run.said_after_the_screen(r#"serve: {"kind":"closed","by":"view""#);
}

#[test]
fn a_program_that_is_killed_leaves_no_view_behind() {
    let run = run_containers(&["--call", "list", "--then", "kill"]);
    run.in_order(&[
        "view: <- response id=1 result=",
        "host: kills the program with SIGKILL",
        "program: was killed by signal 9",
        // Nobody said goodbye: the host saw the stream end.
        "view: destroyed",
    ]);
    run.never("program: bye");
    assert!(run.status.success(), "{}", run.text());
}

#[test]
fn a_terminal_that_goes_away_takes_the_program_and_its_view_with_it() {
    // ssh going away: the pane's pty is closed, the program's session is hung up and every write
    // it makes to its tty fails. Nothing of the program is left and nothing of its view.
    let run = run_containers(&["--call", "list", "--then", "hangup"]);
    assert!(run.status.success(), "{}", run.text());
    run.in_order(&[
        "view: <- response id=1 result=",
        "host: hangs up the terminal",
        // Ended by the signal it was sent, as any program is.
        "program: was killed by signal 1",
        "view: destroyed",
    ]);
    run.never("program: bye");
}

#[test]
fn a_program_closes_its_view_shows_another_and_another_and_closes_that_one_and_goes_on() {
    let run = run_program("reopen.mjs", &["--call", "show"]);
    assert!(run.status.success(), "{}", run.text());
    run.in_order(&[
        "host: welcome id=1 re=1 version=1 with a token features=dismiss views=web@1.0",
        "program: open id=2 view=web version=1.0 bundle of 2 files",
        "host: need id=2 re=2 missing=2",
        "program: file",
        "program: file",
        "view: started from the bundle: entry index.html, 2 files",
        // The program has no title: the terminal names the view by the program.
        r#"view: title=none program="reopen""#,
        "view: -> call id=1 show",
        "host: msg id=* call=1 show",
        // The program closes the view that runs; the host tells the program once the
        // view is gone.
        "program: dismiss id=",
        "host: the program asks to close its view",
        "view: destroyed",
        "host: closed id=* re=* reason=program",
        // A new token and a new open, as for a first view: the file that changed
        // crosses the stream again, the one that did not does not.
        "program: hello id=",
        "host: welcome id=* re=* version=1 with a token",
        "program: open id=* view=web version=1.0 bundle of 2 files",
        "host: need id=* re=* missing=1",
        "program: file",
        "view: started from the bundle: entry index.html, 2 files",
        "view: -> call id=1 show",
        "program: dismiss id=",
        "view: destroyed",
        "host: closed id=* re=* reason=program",
        // Shown again as it was: the host holds all of it.
        "program: hello id=",
        "host: welcome id=* re=* version=1 with a token",
        "program: open id=* view=web version=1.0 bundle of 2 files",
        "host: need id=* re=* missing=0",
        "view: started from the bundle: entry index.html, 2 files",
        "view: -> call id=1 show",
        "program: dismiss id=",
        "view: destroyed",
        "host: closed id=* re=* reason=program",
        "program: exited with status 0",
    ]);
    // Three views, and only the files that were not held crossed the stream.
    assert_eq!(run.count("program: open"), 3, "{}", run.text());
    assert_eq!(run.count("program: file"), 3, "{}", run.text());
    // The program asked for no second screen.
    assert_eq!(run.count("program: hello"), 3, "{}", run.text());
    assert_eq!(run.count("program: dismiss"), 3, "{}", run.text());
    assert_eq!(run.count("answers CSI c"), 1, "{}", run.text());
    run.never("program: bye");
    run.never("dropped a frame");
    // It said how it went, on the screen it was given back.
    for line in [
        r#"control: {"kind":"reopened"}"#,
        r#"control: {"kind":"closed"}"#,
        r#"serve: {"kind":"closed","by":"program""#,
    ] {
        run.said_after_the_screen(line);
    }
    assert_eq!(
        String::from_utf8_lossy(&run.output)
            .matches("\x1b[?1049h")
            .count(),
        1
    );
}

#[test]
fn a_program_that_has_its_view_shown_again_after_it_answered_is_waited_for() {
    // The first view's script ends with the program closing its view, so `--on-view` makes
    // the interrupt wait for the view that comes after it.
    let run = run_containers(&["--call", "reload", "--then", "interrupt", "--on-view", "2"]);
    assert!(run.status.success(), "{}", run.text());
    run.in_order(&[
        "view: -> call id=1 reload",
        r#"view: <- response id=1 result={"reloads":1}"#,
        "view: has nothing more to do",
        "program: dismiss id=",
        "view: destroyed",
        "host: closed id=* re=* reason=program",
        "program: hello id=",
        r#"program: open id=* view=web version=1.0 title="Containers" bundle of 3 files"#,
        // The terminal held the files: nothing crossed the stream again.
        "host: need id=* re=* missing=0",
        "view: -> call id=1 reload",
        r#"view: <- response id=1 result={"reloads":2}"#,
        "host: the person presses ^C",
        "program: bye id=* reason=signal",
        "program: was killed by signal 2",
    ]);
    assert_eq!(run.count("program: open"), 2, "{}", run.text());
    assert_eq!(run.count("program: file"), 3, "{}", run.text());
    assert_eq!(run.count("answers CSI c"), 1, "{}", run.text());
}

#[test]
fn the_host_that_starts_the_view_again_gets_it_asked_for_again() {
    let run = run_containers(&["--call", "list", "--then", "restart"]);
    assert!(run.status.success(), "{}", run.text());
    run.in_order(&[
        r#"program: open id=2 view=web version=1.0 title="Containers" bundle of 3 files"#,
        "view: -> call id=1 list",
        "view: <- response id=1 result=",
        "host: starts the view again, as after a reconnection",
        "view: destroyed",
        // No `hello` asked: the host offers again, with every version it speaks, and the
        // program that has a view open opens it again.
        "host: welcome id=* version=1 with a token (restart) features=dismiss views=web@1.0",
        r#"program: open id=* view=web version=1.0 title="Containers" bundle of 3 files"#,
        "host: need id=* re=* missing=0",
        "view: -> call id=1 list",
        "view: <- response id=1 result=",
        "host: the person presses ^C",
        "program: bye id=* reason=signal",
        "program: was killed by signal 2",
    ]);
    assert_eq!(run.count("program: open"), 2, "{}", run.text());
    assert_eq!(run.count("view: -> call id=1 list"), 2, "{}", run.text());
}

// ---- what a page asks to reach ---------------------------------------------------

#[test]
fn the_question_to_the_person_names_the_program_and_refusing_it_leaves_the_page_with_nothing() {
    // The page of this program asks for an origin and a capability, so the person is asked.
    // The stand-in person refuses, which is what it does unless it is told otherwise.
    let run = run_program("access.mjs", &["--call", "show", "--then", "interrupt"]);
    assert!(run.status.success(), "{}", run.text());
    run.in_order(&[
        r#"program: open id=2 view=web version=1.0 title="Show it" bundle of 1 files with access"#,
        "host: need id=2 re=2 missing=1",
        "program: file",
        // The words the program has for itself and for its view; the question names no one else.
        r#"host: asked the person about origins=1 permissions=1 of program="Domain Desk" title="Show it": refused"#,
        "view: started from the bundle: entry index.html, 1 files",
        r#"view: title="Show it" program="Domain Desk""#,
        // Nothing was allowed: the policy lets the page reach only its own bundle, and the page
        // is told so.
        "view: policy default-src 'none'*connect-src 'self'; img-src",
        "view: <- context mode=dark size=640x384 reduceMotion=false access=none",
        "view: -> call id=1 show",
        r#"view: <- response id=1 result={"shown":true}"#,
        "host: the person presses ^C",
        "program: bye id=* reason=signal",
    ]);
    run.never("api.example.com");
    run.never("clipboardWrite");
}

#[test]
fn what_the_person_allows_is_in_the_policy_and_in_what_the_page_is_told() {
    let run = run_program(
        "access.mjs",
        &["--allow-network", "--call", "show", "--then", "interrupt"],
    );
    assert!(run.status.success(), "{}", run.text());
    run.in_order(&[
        r#"host: asked the person about origins=1 permissions=1 of program="Domain Desk" title="Show it": allowed"#,
        "view: started from the bundle",
        "view: policy default-src 'none'*connect-src 'self' https://api.example.com",
        // What was allowed is what the page is told, in the shape the program asked in.
        r#"view: <- context mode=dark size=640x384 reduceMotion=false access={"connect":["https://api.example.com"],"permissions":["clipboardWrite"]}"#,
        "view: -> call id=1 show",
        "host: the person presses ^C",
        "program: bye id=* reason=signal",
    ]);
}

#[test]
fn the_page_asks_to_open_a_link_and_the_host_refuses_and_tells_it_nothing() {
    let run = run_containers(&[
        "--link",
        "https://example.com/docs",
        "--link",
        "file:///etc/passwd",
        "--call",
        "list",
        "--then",
        "interrupt",
    ]);
    assert!(run.status.success(), "{}", run.text());
    run.in_order(&[
        "view: -> link https://example.com/docs",
        "view: -> link file:///etc/passwd",
        "view: -> call id=1 list",
        // The person is asked about the one the host may open; the stand-in refuses.
        "host: refuses to open https://example.com/docs for the view",
        "view: <- response id=1 result=",
        "host: the person presses ^C",
    ]);
    // An address that is not `http` or `https` is dropped before the person is asked, and the
    // page is answered neither way: a link is not a call.
    run.never("refuses to open file:");
    assert_eq!(run.count("refuses to open"), 1, "{}", run.text());
    assert_eq!(run.count("view: <- response"), 1, "{}", run.text());
}

// ---- a terminal that does not host pencere ---------------------------------------

#[test]
fn a_terminal_that_does_not_host_pencere_gets_the_text_interface() {
    let run = run_containers(&["--no-host"]);
    assert!(run.status.success(), "{}", run.text());
    run.in_order(&[
        "program: hello id=1",
        "host: does not host pencere: no answer",
        "host: answers CSI c",
        "program: exited with status 0",
    ]);
    run.never("welcome");
    run.never("program: open");
    assert!(
        run.output_has("nginx:1.27").is_some(),
        "the fallback did not print the table"
    );
    assert!(run.output_has("cache").is_some());
    assert!(
        run.output_has(r#"serve: {"kind":"no-view","reason":"no-host"}"#)
            .is_some()
    );
    // It never took the screen: there was nothing to put on it.
    assert!(run.output_has("\x1b[?1049h").is_none());
}

// ---- what a program cannot make the host do --------------------------------------

/// A program that writes frames and cannot read what the host answers, as a
/// file shown with `cat` or a program in the background is.
fn forger(name: &str, with_raw_tty: bool) -> Run {
    let dir = fresh(name);
    let script = dir.join("forger.sh");
    let stty = if with_raw_tty {
        "stty -icanon -echo\n"
    } else {
        ""
    };
    fs::write(
        &script,
        format!(
            r#"{stty}printf '\033_pencere;hello;id=1;{{"versions":[1],"views":[{{"kind":"web","versions":["1.0"]}}]}}\033\\'
printf '\033[c'
sleep 0.3
printf '\033[?1049h'
printf '\033_pencere;open;id=2;{{"token":"AAAAAAAAAAAAAAAAAAAAAA","view":"web","version":"1.0","title":"x\\u001b[2J"}}\033\\'
sleep 0.3
"#
        ),
    )
    .expect("a script under the target directory");
    run(&[], &["sh", &script.to_string_lossy()])
}

#[test]
fn a_program_that_cannot_read_the_answer_cannot_open_a_view() {
    // The tty is as a program waiting for the answer leaves it, so the host
    // answers: but the program never reads it, and a guessed token opens nothing.
    let run = forger("forger-raw", true);
    run.in_order(&[
        "program: hello id=1",
        "host: sees the tty: canonical=false echo=false",
        "host: welcome id=1 re=1 version=1 with a token",
        // The view's title has an escape in it; the record writes it out and does
        // not put it on the person's terminal.
        r#"program: open id=2 view=web version=1.0 title="x\u{1b}[2J" without a bundle"#,
        "reason=refused code=token",
    ]);
    assert!(!run.text().contains('\u{1b}'), "{:?}", run.text());
    run.never("view: started");
    run.never("need");
}

#[test]
fn a_program_that_leaves_the_tty_as_the_shell_set_it_is_not_answered() {
    // `cat file`: the tty is canonical with echo on, so what the host wrote would
    // be typed text for whatever reads next. It writes nothing: not the `welcome`
    // a hello would get, and not the refusal an `open` with no token would get.
    let run = forger("forger-canonical", false);
    run.in_order(&[
        "program: hello id=1",
        "host: sees the tty: canonical=true echo=true",
        "program: open id=2",
    ]);
    run.never("welcome");
    run.never("closed");
    run.never("view: started");
}

#[test]
fn the_answer_to_hello_comes_before_the_reply_to_csi_c() {
    // What the program finds in its input, byte for byte, in the order it is
    // there (specification §5.2): a program that sees the reply first has to
    // conclude that no host answered.
    let dir = fresh("order");
    let script = dir.join("reader.sh");
    fs::write(
        &script,
        r#"stty -icanon -echo min 0 time 5
printf '\033_pencere;hello;id=1;{"versions":[1],"views":[{"kind":"web","versions":["1.0"]}]}\033\\'
printf '\033[c'
dd bs=512 count=1 2>/dev/null | od -An -tx1 | tr -d ' \n'
"#,
    )
    .expect("a script under the target directory");
    let run = run(&[], &["sh", &script.to_string_lossy()]);
    let seen = String::from_utf8_lossy(&run.output).into_owned();
    // `;welcome` and the reply `ESC [ ? 6 2 ; 4 c`, as hex.
    let welcome_at = seen.find("3b77656c636f6d65");
    let reply_at = seen.find("1b5b3f36323b3463");
    assert!(
        welcome_at.is_some(),
        "no welcome in what the program read: {seen:?}"
    );
    assert!(
        reply_at.is_some(),
        "no reply in what the program read: {seen:?}"
    );
    assert!(welcome_at < reply_at, "the reply came first: {seen:?}");
}

/// Runs the host with its output discarded, for programs that write a lot, and
/// checks it gave up on the program at the time limit and did not wait for it.
fn cut_short(program: &[&str]) {
    let started = std::time::Instant::now();
    let output = Command::new(FAKE_HOST)
        .args(["--limit", "1", "--"])
        .args(program)
        .stdin(Stdio::null())
        .output()
        .expect("the fake host starts");
    assert_eq!(output.status.code(), Some(124));
    let record = String::from_utf8_lossy(&output.stderr);
    assert!(record.contains("the time limit is up"), "{record}");
    assert!(record.contains("was killed by signal 9"), "{record}");
    assert!(
        started.elapsed().as_secs() < 10,
        "it waited for the program: {record}"
    );
}

#[test]
fn a_program_that_never_ends_is_cut_short() {
    cut_short(&["sleep", "30"]);
}

#[test]
fn a_program_that_never_stops_writing_is_cut_short_too() {
    // Reading its output must not keep the host from watching the clock.
    cut_short(&[
        "sh",
        "-c",
        "while true; do echo xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx; done",
    ]);
}

#[test]
fn a_program_that_closes_its_tty_and_goes_on_is_cut_short_as_well() {
    // The pane reports its end, the program is still there: the host must not
    // wait for it without a limit.
    cut_short(&["sh", "-c", "exec >/dev/null 2>&1 </dev/null; sleep 30"]);
}

// ---- a filter in a pipeline -------------------------------------------------------

/// `text` in single quotes, for `sh -c`.
fn quoted(text: &str) -> String {
    format!("'{}'", text.replace('\'', r"'\''"))
}

/// `filter.mjs` in the middle of a pipeline: three rows in from `printf`, its stdout to
/// `out`, and its status after it.
fn pipeline(dir: &Path, on: &Installed, options: &[&str]) -> (Run, String, String) {
    let out = dir.join("out.txt");
    let status = dir.join("status");
    let command: Vec<String> = on
        .filter_command(Path::new(&program("filter.mjs")))
        .iter()
        .map(|word| quoted(word))
        .collect();
    let script = format!(
        r#"printf 'web\napi\ndb\n' | {} > {}; echo $? > {}"#,
        command.join(" "),
        quoted(&out.to_string_lossy()),
        quoted(&status.to_string_lossy()),
    );
    let run = run(options, &["sh", "-c", &script]);
    let out = fs::read_to_string(&out).expect("the pipeline wrote its output");
    let status = fs::read_to_string(&status).expect("the pipeline wrote its status");
    (run, out, status.trim().to_owned())
}

// A filter asks on the terminal it opens itself (`/dev/tty`). Deno and Node read it; Bun's stream
// for a tty (1.0.0, the one tried) does not deliver what the host writes there, so a filter on Bun
// finds no host: these two are not run on it.
macro_rules! on_node_and_deno {
    ($test:ident) => {
        mod $test {
            #[test]
            fn on_node() {
                super::on(super::Runtime::Node, super::$test);
            }
            #[test]
            fn on_deno() {
                super::on(super::Runtime::Deno, super::$test);
            }
        }
    };
}

fn a_filter_asks_for_its_view_on_the_terminal_and_writes_what_was_chosen_to_the_pipe(
    on: &Installed,
) {
    let dir = fresh(&format!("filter-chosen-{}", on.runtime.name()));
    let (run, out, status) = pipeline(
        &dir,
        on,
        &["--call", "rows", "--call", r#"choose={"rows":[2,0]}"#],
    );
    assert!(run.status.success(), "{}", run.text());
    run.in_order(&[
        "program: hello id=1",
        "host: sees the tty: canonical=false echo=false",
        "screen: alternate screen on",
        r#"program: open id=2 view=web version=1.0 title="Choose rows" bundle of 1 files"#,
        "view: -> call id=1 rows",
        "view: -> call id=2 choose",
        // The program answers the call before it closes the view, so the view hears
        // that the rows were taken.
        r#"view: <- response id=2 result={"chosen":2}"#,
        "program: dismiss",
        "host: closed id=* reason=program",
        "screen: alternate screen off",
    ]);
    // The terminal had the protocol; the pipe has the data and nothing else.
    assert_eq!(out, "db\nweb\n");
    assert_eq!(status, "0");
}
on_node_and_deno!(
    a_filter_asks_for_its_view_on_the_terminal_and_writes_what_was_chosen_to_the_pipe
);

fn a_filter_whose_view_closes_with_nothing_chosen_writes_nothing_and_fails(on: &Installed) {
    let dir = fresh(&format!("filter-nothing-{}", on.runtime.name()));
    let (run, out, status) = pipeline(&dir, on, &["--call", "rows", "--then", "close"]);
    assert!(run.status.success(), "{}", run.text());
    run.in_order(&[
        "screen: alternate screen on",
        "host: the person closes the view",
        "host: closed id=* reason=user",
        "screen: alternate screen off",
    ]);
    // `rows=$(… | filter) && mail …` stops here.
    assert_eq!(out, "");
    assert_eq!(status, "1");
}
on_node_and_deno!(a_filter_whose_view_closes_with_nothing_chosen_writes_nothing_and_fails);

// ---- the example, on each runtime and as one executable ------------------------------------

/// The size of a file the page would fetch: the files of the view, and the module that is the
/// page's bridge, which the SDK builds.
fn example_file_size(name: &str) -> u64 {
    let path = match name {
        "pencere-page.js" => root().join("sdk/typescript/dist/page/index.js"),
        _ => root().join("examples/containers/view").join(name),
    };
    fs::metadata(path).expect("a file of the view").len()
}

/// What the example does in the pane of a host that hosts views: whichever way it was started.
fn check_the_example_opens_answers_a_call_and_pushes_to_its_page(run: &Run) {
    assert!(run.status.success(), "{}", run.text());
    run.in_order(&[
        "host: welcome id=1 re=1 version=1 with a token features=dismiss views=web@1.0",
        r#"program: open id=2 view=web version=1.0 title="Containers" bundle of 4 files"#,
        "host: need id=2 re=2 missing=4",
        "view: started from the bundle: entry index.html, 4 files",
        // What the strip above the view says, and the icon.
        r#"view: title="Containers" program="containers""#,
        "view: icons png 64x64",
        "view: <- context mode=dark size=640x384",
        // The program pushes the use of the running containers as soon as it is asked
        // and the page hears it.
        r#"view: -> subscribe id=1 stats {"every":20}"#,
        "host: msg id=* subscribe=1 stats",
        "program: msg id=* push=1",
        r#"view: <- push id=1 value={"stats":[{"cpu":4,"id":"web","memory":38}"#,
        // The calls wait for the first push, and each is answered with the list.
        "view: -> call id=2 list",
        "host: msg id=* call=2 list",
        "program: msg id=* response=2 result",
        r#"view: <- response id=2 result={"containers":[{"id":"web","image":"nginx:1.27","name":"web","ports":"8080→80","state":"running"}"#,
        r#"view: -> call id=3 stop {"id":"web"}"#,
        "host: msg id=* call=3 stop",
        r#"view: <- response id=3 result={"containers":[{"id":"web","image":"nginx:1.27","name":"web","ports":"8080→80","state":"stopped"}"#,
        // ⌃C reached the program as SIGINT, so the tty kept ISIG on (§4.9), and it ends as the
        // signal would have ended it.
        "host: the person presses ^C",
        "program: bye id=* reason=signal",
        "view: destroyed",
        "host: closed id=* re=* reason=program",
        "screen: alternate screen off",
        "program: was killed by signal 2",
    ]);
    // The host looked at the tty before it answered the question: not canonical, no echo.
    run.in_order(&[
        "program: hello id=1",
        "host: sees the tty: canonical=false echo=false",
    ]);

    // The files the page would fetch are the files on disk, whole.
    for (name, media_type) in [
        ("index.html", "text/html"),
        ("app.js", "text/javascript"),
        ("pencere-page.js", "text/javascript"),
        ("style.css", "text/css"),
    ] {
        let size = example_file_size(name);
        let wanted = format!("view: fetched {name} {media_type} {size} bytes");
        assert!(
            run.events.contains(&wanted),
            "no {wanted:?}:\n{}",
            run.text()
        );
    }
}

const EXAMPLE_SESSION: [&str; 8] = [
    "--subscribe",
    r#"stats={"every":20}"#,
    "--call",
    "list",
    "--call",
    r#"stop={"id":"web"}"#,
    "--then",
    "interrupt",
];

fn the_containers_example_opens_answers_a_call_and_pushes_to_its_page(on: &Installed) {
    let run = run_example(on, &EXAMPLE_SESSION);
    check_the_example_opens_answers_a_call_and_pushes_to_its_page(&run);
}
on_each_runtime!(the_containers_example_opens_answers_a_call_and_pushes_to_its_page);

/// What the example printed where no view could be shown.
fn check_the_example_prints_its_containers(run: &Run) {
    assert!(run.status.success(), "{}", run.text());
    run.never("program: open");
    for line in ["● web", "● api", "○ cache", "8080→80"] {
        assert!(
            run.output_has(line).is_some(),
            "the fallback did not print {line:?}:\n{}",
            run.text()
        );
    }
}

// ---- the example written with the Rust library --------------------------------------------
//
// The same program in another language, held to the same scenarios: the handshake, the files the
// terminal lacks, a call, a subscription, ⌃C, a terminal that is not a host, and the state the
// tty is left in. The view is the Lua one, which every build carries: the stand-in opens the
// three kinds of view the same way.

/// The Rust example (`examples/containers-rust`), built by cargo next to the stand-in host. It is
/// built here once, so that these tests run on the example as it is now whichever way they were
/// started (`cargo test --workspace` has built it already, and this is then a look that finds
/// nothing to do).
fn rust_example() -> PathBuf {
    static BUILT: OnceLock<PathBuf> = OnceLock::new();
    BUILT
        .get_or_init(|| {
            let profile = Path::new(FAKE_HOST).parent().expect("a directory");
            let target = profile.parent().expect("the target directory");
            let mut build = Command::new(env!("CARGO"));
            build
                .args(["build", "--quiet", "-p", "containers-rust", "--target-dir"])
                .arg(target);
            if profile.file_name().is_some_and(|name| name == "release") {
                build.arg("--release");
            }
            let status = build.status().expect("cargo runs");
            assert!(status.success(), "the example does not build");
            let binary = profile.join("containers-rust");
            assert!(binary.is_file(), "{} is missing", binary.display());
            binary
        })
        .clone()
}

fn run_rust_example(options: &[&str], arguments: &[&str]) -> Run {
    let binary = rust_example();
    let mut program = vec![binary.to_string_lossy().into_owned()];
    program.extend(arguments.iter().map(|argument| (*argument).to_owned()));
    let program: Vec<&str> = program.iter().map(String::as_str).collect();
    run(options, &program)
}

#[test]
fn the_rust_example_opens_answers_a_call_and_pushes_to_its_page() {
    let run = run_rust_example(
        &[
            "--subscribe",
            "stats",
            "--call",
            r#"upload={"seq":1,"data":"abc","last":true}"#,
            "--call",
            "nobody",
            "--then",
            "interrupt",
        ],
        &["--lua"],
    );
    assert!(run.status.success(), "{}", run.text());
    run.in_order(&[
        "program: hello id=1",
        // The host looked at the tty before it answered (§6.2): the library had made it
        // non-canonical with echo off.
        "host: sees the tty: canonical=false echo=false",
        "host: welcome id=1 re=1 version=1 with a token features=dismiss views=lua@1.0",
        // The answer to the question comes before the reply to `CSI c` (§5.2).
        "host: answers CSI c",
        "screen: alternate screen on",
        r#"program: open id=2 view=lua version=1.0 title="Containers" bundle of 1 files"#,
        "host: need id=2 re=2 missing=1",
        "program: file",
        "view: started from the bundle: entry containers.lua, 1 files",
        "view: kind=lua version=1.0",
        // What the strip above the view says: the program's words for the view and for itself.
        r#"view: title="Containers" program="containers-rust""#,
        // The program pushes the use of the running containers as soon as it is asked.
        "view: -> subscribe id=1 stats",
        "host: msg id=* subscribe=1 stats",
        "program: msg id=* push=1",
        r#"view: <- push id=1 value={"readings":[{"cpu":"#,
        r#"view: -> call id=2 upload {"data":"abc","last":true,"seq":1}"#,
        "host: msg id=* call=2 upload",
        "program: msg id=* response=2 result",
        r#"view: <- response id=2 result={"bytes":3}"#,
        // A name nobody serves is answered at once, and does not hold the script.
        "view: -> call id=3 nobody",
        "view: <- response id=3 error code=not-found",
        "host: the person presses ^C",
        "program: bye id=* reason=signal",
        "view: destroyed",
        // The host's last word for the view, and the one frame it writes for a program that has
        // said it ended (§10.3).
        "host: closed id=* re=* reason=program",
        "screen: alternate screen off",
        // What the program wrote after it gave the screen back was written after it gave back the
        // tty's modes: the shell finds the tty as it left it (§4.9).
        "host: sees the tty after the screen: canonical=true echo=true",
        "program: exited with status 130",
    ]);
    assert_eq!(run.count("program: file"), 1, "{}", run.text());
    // The files the view would fetch are the file on disk, whole.
    let size = fs::metadata(root().join("kit/luau/examples/containers.lua"))
        .expect("the script of the example")
        .len();
    let wanted = format!("view: fetched containers.lua text/x-lua {size} bytes");
    assert!(
        run.events.contains(&wanted),
        "no {wanted:?}:\n{}",
        run.text()
    );
    // What the program says once the terminal is the shell's again.
    run.said_after_the_screen("containers-rust: the view sent 3 bytes");
    assert!(
        !run.text().contains("host: cannot read the tty's modes"),
        "{}",
        run.text()
    );
}

#[test]
fn the_rust_example_has_a_flood_of_readings_cancelled_and_ends_at_the_interrupt() {
    // The program pushes as fast as the terminal takes them; the page cancels after three.
    let run = run_rust_example(
        &[
            "--subscribe",
            "stats",
            "--cancel-after",
            "3",
            "--then",
            "interrupt",
        ],
        &["--lua", "--flood"],
    );
    assert!(run.status.success(), "{}", run.text());
    run.in_order(&[
        "view: -> subscribe id=1 stats",
        "view: <- push id=1",
        "view: <- push id=1",
        "view: <- push id=1",
        "view: -> cancel id=1",
        "host: the person presses ^C",
        "program: bye id=* reason=signal",
        "host: closed id=* re=* reason=program",
        "screen: alternate screen off",
        "host: sees the tty after the screen: canonical=true echo=true",
        "program: exited with status 130",
    ]);
}

#[test]
fn the_rust_example_is_asked_for_its_view_again_by_a_host_that_starts_it_again() {
    let run = run_rust_example(&["--subscribe", "stats", "--then", "restart"], &["--lua"]);
    assert!(run.status.success(), "{}", run.text());
    run.in_order(&[
        "host: welcome id=1 re=1 version=1 with a token features=dismiss views=lua@1.0",
        "program: open id=2 view=lua",
        "view: -> subscribe id=1 stats",
        "host: starts the view again, as after a reconnection",
        "host: welcome id=* version=1 with a token (restart)",
        // The files are in the host's store already: the program opens again and sends none.
        "program: open id=* view=lua",
        "view: -> subscribe id=1 stats",
        "host: the person presses ^C",
        "program: bye id=* reason=signal",
        "host: closed id=* re=* reason=program",
        "program: exited with status 130",
    ]);
    assert_eq!(run.count("program: open"), 2, "{}", run.text());
}

#[test]
fn the_rust_example_ends_when_the_terminal_goes_away() {
    let run = run_rust_example(&["--subscribe", "stats", "--then", "hangup"], &["--lua"]);
    assert!(run.status.success(), "{}", run.text());
    run.in_order(&[
        "view: -> subscribe id=1 stats",
        "host: hangs up the terminal",
    ]);
    run.never("the time limit is up");
    assert!(
        run.events.iter().any(|event| event.starts_with("program: ")
            && (event.contains("exited with status") || event.contains("killed by signal"))),
        "{}",
        run.text()
    );
}

#[test]
fn the_rust_example_prints_its_containers_where_no_view_can_be_shown() {
    let run = run_rust_example(&["--no-host"], &["--lua"]);
    assert!(run.status.success(), "{}", run.text());
    run.in_order(&[
        "program: hello id=1",
        "host: does not host pencere: no answer",
        "host: answers CSI c",
        "program: exited with status 0",
    ]);
    run.never("welcome");
    run.never("program: open");
    for line in ["NAME", "IMAGE", "nginx", "redis"] {
        assert!(
            run.output_has(line).is_some(),
            "the fallback did not print {line:?}:\n{}",
            run.text()
        );
    }
    // It never took the screen: there was nothing to put on it.
    assert!(run.output_has("\x1b[?1049h").is_none());
}

#[test]
fn the_rust_example_prints_its_containers_where_it_has_no_terminal_at_all() {
    // Not a pane: its output is a pipe, and a pipe is no place to ask for a view.
    let output = Command::new(rust_example())
        .arg("--lua")
        .stdin(Stdio::null())
        .output()
        .expect("the example starts");
    assert!(output.status.success());
    let text = String::from_utf8_lossy(&output.stdout);
    assert!(text.contains("NAME") && text.contains("IMAGE"), "{text}");
    assert!(!text.contains('\x1b'), "{text:?}");
}

fn the_containers_example_prints_its_containers_where_no_view_can_be_shown(on: &Installed) {
    check_the_example_prints_its_containers(&run_example(on, &["--no-host"]));
}
on_each_runtime!(the_containers_example_prints_its_containers_where_no_view_can_be_shown);

/// What the tty's flags are in the text `stty -a` printed after the last time the screen was
/// given back: the words of it, where a flag that is off has a `-` before it.
fn flags_after_the_screen(run: &Run) -> Vec<String> {
    let text = String::from_utf8_lossy(&run.output).into_owned();
    let off = text
        .rfind("\x1b[?1049l")
        .expect("the program gave the screen back");
    text.get(off..)
        .unwrap_or("")
        .split(|c: char| c.is_whitespace() || c == ';')
        .map(str::to_owned)
        .collect()
}

fn the_tty_is_as_it_was_when_the_person_closes_the_view_and_the_program_ends_on_its_own(
    on: &Installed,
) {
    // The program is followed by `stty -a`, in the same pane: it prints the modes it was left with.
    let command: Vec<String> = on
        .command(&example_main())
        .iter()
        .map(|word| quoted(word))
        .collect();
    let script = format!("{}; stty -a", command.join(" "));
    let run = run(
        &["--call", "list", "--then", "close"],
        &["sh", "-c", &script],
    );
    assert!(run.status.success(), "{}", run.text());
    run.in_order(&[
        "screen: alternate screen on",
        "host: the person closes the view",
        "host: closed id=* reason=user",
        "screen: alternate screen off",
    ]);
    // It ended without being killed: nothing waits for a byte of input, and nothing was left running.
    run.never("the time limit is up");
    let flags = flags_after_the_screen(&run);
    for on in ["icanon", "echo", "isig"] {
        assert!(
            flags.iter().any(|flag| flag == on),
            "{on} is not on after the program ended: {flags:?}"
        );
    }
    for off in ["-icanon", "-echo", "-isig"] {
        assert!(
            !flags.iter().any(|flag| flag == off),
            "{off} after the program ended: {flags:?}"
        );
    }
}
on_each_runtime!(
    the_tty_is_as_it_was_when_the_person_closes_the_view_and_the_program_ends_on_its_own
);

/// `exit.mjs` or `suspend.mjs`, on the runtime.
fn run_test_program(on: &Installed, name: &str, options: &[&str], arguments: &[&str]) -> Run {
    let mut command = on.command(Path::new(&program(name)));
    command.extend(arguments.iter().map(|argument| (*argument).to_owned()));
    let command: Vec<&str> = command.iter().map(String::as_str).collect();
    run(options, &command)
}

fn a_process_that_exits_with_its_view_up_waits_for_the_answer_to_its_bye_and_leaves_the_screen_after_it(
    on: &Installed,
) {
    let run = run_test_program(on, "exit.mjs", &["--call", "quit"], &[]);
    assert!(run.status.success(), "{}", run.text());
    run.in_order(&[
        "screen: alternate screen on",
        "view: -> call id=1 quit",
        "program: bye id=* reason=exit",
        // The program read the host's last word, without an event loop to wait with: it left
        // the screen only after, and it is the shell that gets the tty with nothing in its queue.
        "host: closed id=* re=* reason=program",
        "screen: alternate screen off",
        "program: exited with status 0",
    ]);
    // After the host had time to answer, the tty held nothing for whatever reads it next.
    assert!(
        run.output_has("LEFT:\"\"").is_some(),
        "the tty was left with the host's answer in it:\n{}",
        String::from_utf8_lossy(&run.output)
    );
}
on_each_runtime!(
    a_process_that_exits_with_its_view_up_waits_for_the_answer_to_its_bye_and_leaves_the_screen_after_it
);

fn a_program_stopped_by_ctrl_z_gives_the_terminal_back_and_shows_its_view_again_when_it_is_continued(
    on: &Installed,
) {
    let dir = fresh(&format!("suspend-{}", on.runtime.name()));
    let modes = dir.join("modes");
    let run = run_test_program(
        on,
        "suspend.mjs",
        &["--call", "suspend"],
        &[&modes.to_string_lossy()],
    );
    assert!(run.status.success(), "{}", run.text());
    run.in_order(&[
        "program: hello id=1",
        "host: sees the tty: canonical=false echo=false",
        "screen: alternate screen on",
        "view: -> call id=1 suspend",
        // ⌃Z: the view is closed with the program's word, and the answer is waited for; the screen
        // is given back, the program is stopped, and nothing more is heard of it until it is continued.
        "program: dismiss id=*",
        "host: closed id=* re=* reason=program",
        "screen: alternate screen off",
        // Continued: it takes its modes and asks for a view, as the first time.
        "program: hello id=*",
        "host: sees the tty: canonical=false echo=false",
        "screen: alternate screen on",
        r#"program: open id=* view=web version=1.0 title="Suspend""#,
        "view: started from the bundle",
        // The page is shown again and calls `suspend` once more, which ends the program with SIGINT.
        "view: -> call id=1 suspend",
        "program: bye id=* reason=signal",
        "screen: alternate screen off",
        "program: was killed by signal 2",
    ]);
    // What the shell saw while the program was stopped: the tty as the shell set it (§4.9).
    let seen = fs::read_to_string(&modes).expect("the helper wrote the modes of the tty");
    let flags: Vec<&str> = seen
        .split(|c: char| c.is_whitespace() || c == ';')
        .collect();
    for on in ["icanon", "echo", "isig"] {
        assert!(
            flags.contains(&on),
            "{on} is not on while it is stopped: {seen}"
        );
    }
    for off in ["-icanon", "-echo", "-isig"] {
        assert!(!flags.contains(&off), "{off} while it is stopped: {seen}");
    }
    // It was one stop and one continue: the program asked twice, and opened twice.
    assert_eq!(run.count("program: hello"), 2, "{}", run.text());
}
on_each_runtime!(
    a_program_stopped_by_ctrl_z_gives_the_terminal_back_and_shows_its_view_again_when_it_is_continued
);

/// The example compiled by `deno compile` with the task of `examples/containers/deno.jsonc`, into
/// the target directory: where it is, or why there is none (`SKIPPED`). The compile is made once.
fn compiled_example() -> Result<&'static Path, String> {
    static COMPILED: OnceLock<Result<PathBuf, String>> = OnceLock::new();
    COMPILED
        .get_or_init(compile_example)
        .as_ref()
        .map(PathBuf::as_path)
        .map_err(Clone::clone)
}

fn compile_example() -> Result<PathBuf, String> {
    let deno = Runtime::Deno
        .find()
        .ok_or("deno is not installed")?
        .executable;
    example_main();
    // `deno compile` downloads the runtime it builds on the first time it is used for a version of
    // Deno; the gate does not reach the network, so it only compiles when that is already done.
    let version = Command::new(&deno)
        .arg("--version")
        .output()
        .map_err(|e| format!("deno does not run: {e}"))?;
    let version = String::from_utf8_lossy(&version.stdout).into_owned();
    let first = version.lines().next().unwrap_or("");
    let number = first.split_whitespace().nth(1).unwrap_or("");
    let target = first
        .trim_end_matches(')')
        .rsplit(", ")
        .next()
        .unwrap_or("");
    let info = Command::new(&deno)
        .args(["info", "--json"])
        .output()
        .map_err(|e| format!("deno does not run: {e}"))?;
    let info: serde_json::Value = serde_json::from_slice(&info.stdout).unwrap_or_default();
    let dir = info.get("denoDir").and_then(|d| d.as_str()).unwrap_or("");
    let runtime = Path::new(dir)
        .join("dl/release")
        .join(format!("v{number}"))
        .join(format!("denort-{target}.zip"));
    if !runtime.is_file() {
        return Err(format!(
            "the runtime deno compile builds on is not downloaded ({} is missing): \
             run `deno task compile` in examples/containers once",
            runtime.display()
        ));
    }

    // The task of the example, with its output in the target directory.
    let example = root().join("examples/containers");
    let config = fs::read_to_string(example.join("deno.jsonc")).expect("the example's deno.jsonc");
    let task = config
        .lines()
        .find_map(|line| line.trim().strip_prefix(r#""compile": ""#))
        .and_then(|rest| rest.trim_end_matches(',').strip_suffix('"'))
        .expect("a compile task in deno.jsonc");
    assert!(
        task.contains(" --output bin/containers "),
        "the task writes to bin/containers: {task}"
    );
    let out = fresh("compiled").join("containers");
    let task = task.replace(
        "--output bin/containers",
        &format!("--output {}", quoted(&out.to_string_lossy())),
    );
    let path = format!(
        "{}:{}",
        deno.parent().unwrap_or(Path::new("")).display(),
        std::env::var("PATH").unwrap_or_default()
    );
    let made = Command::new("sh")
        .args(["-c", &task])
        .current_dir(&example)
        .env("PATH", path)
        .env("DENO_NO_UPDATE_CHECK", "1")
        .output()
        .map_err(|e| format!("deno compile does not run: {e}"))?;
    if !made.status.success() || !out.is_file() {
        return Err(format!(
            "deno compile failed:\n{}",
            String::from_utf8_lossy(&made.stderr)
        ));
    }
    Ok(out)
}

/// `test` with the compiled example, or `SKIPPED` and the reason when it cannot be made. A compile
/// that fails where deno and its runtime are there is a failure.
fn with_the_compiled_example(test: fn(&Path)) {
    match compiled_example() {
        Ok(binary) => test(binary),
        Err(reason) if reason.starts_with("deno compile failed") => panic!("{reason}"),
        Err(reason) => println!("SKIPPED: the compiled example: {reason}"),
    }
}

#[test]
fn the_compiled_example_opens_answers_a_call_and_pushes_to_its_page() {
    with_the_compiled_example(|binary| {
        let run = run(&EXAMPLE_SESSION, &[&binary.to_string_lossy()]);
        check_the_example_opens_answers_a_call_and_pushes_to_its_page(&run);
    });
}

#[test]
fn the_compiled_example_prints_its_containers_where_no_view_can_be_shown() {
    with_the_compiled_example(|binary| {
        let run = run(&["--no-host"], &[&binary.to_string_lossy()]);
        check_the_example_prints_its_containers(&run);
    });
}

// ---- an app written in Lua, made one file by `pencere build` --------------------------------
//
// The command (`pencere-cli`, built by cargo next to the stand-in host) makes the executable of an
// app: the example, and a project `pencere new` makes. The executable is then run in the pane like
// any program, and held to the same scenarios as the Rust example's.

/// The `pencere` command, built by cargo next to the stand-in host (once, as the Rust example is).
fn pencere_command() -> PathBuf {
    static BUILT: OnceLock<PathBuf> = OnceLock::new();
    BUILT
        .get_or_init(|| {
            let profile = Path::new(FAKE_HOST).parent().expect("a directory");
            let target = profile.parent().expect("the target directory");
            let mut build = Command::new(env!("CARGO"));
            build
                .args(["build", "--quiet", "-p", "pencere-cli", "--target-dir"])
                .arg(target);
            if profile.file_name().is_some_and(|name| name == "release") {
                build.arg("--release");
            }
            let status = build.status().expect("cargo runs");
            assert!(status.success(), "the command does not build");
            let binary = profile.join("pencere");
            assert!(binary.is_file(), "{} is missing", binary.display());
            binary
        })
        .clone()
}

/// Runs `pencere` with `arguments` in `directory`; its status and what it said.
fn pencere(directory: &Path, arguments: &[&str]) -> (ExitStatus, String) {
    let output = Command::new(pencere_command())
        .args(arguments)
        .current_dir(directory)
        .stdin(Stdio::null())
        .output()
        .expect("pencere runs");
    let said = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    (output.status, said)
}

/// The Lua example, built into one file (once).
fn lua_example() -> PathBuf {
    static BUILT: OnceLock<PathBuf> = OnceLock::new();
    BUILT
        .get_or_init(|| {
            let out = fresh("lua-example").join("containers-lua");
            let example = root().join("examples/containers-lua");
            let (status, said) = pencere(&example, &["build", ".", "-o", &out.to_string_lossy()]);
            assert!(status.success(), "pencere build: {said}");
            out
        })
        .clone()
}

/// Whether macOS accepts the executable's signature: a signed file that grew is refused.
fn assert_signed(executable: &Path) {
    if cfg!(target_os = "macos") {
        let output = Command::new("codesign")
            .arg("--verify")
            .arg(executable)
            .output()
            .expect("codesign runs");
        assert!(
            output.status.success(),
            "codesign --verify {}: {}",
            executable.display(),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
fn the_built_lua_app_opens_its_view_answers_calls_and_feeds_its_page_until_the_interrupt() {
    let app = lua_example();
    assert_signed(&app);
    let app = app.to_string_lossy().into_owned();
    let run = run(
        &[
            "--subscribe",
            "stats",
            "--call",
            "list",
            "--call",
            r#"toggle={"id":"web"}"#,
            "--call",
            r#"toggle={"id":"nobody"}"#,
            "--call",
            "unheard",
            "--then",
            "interrupt",
        ],
        &[&app],
    );
    assert!(run.status.success(), "{}", run.text());
    run.in_order(&[
        "program: hello id=1",
        "host: sees the tty: canonical=false echo=false",
        "host: welcome id=1 re=1 version=1 with a token features=dismiss views=lua@1.0",
        "host: answers CSI c",
        "screen: alternate screen on",
        r#"program: open id=2 view=lua version=1.0 title="Containers" bundle of 5 files"#,
        "host: need id=2 re=2 missing=5",
        "program: file",
        "view: started from the bundle: entry page.lua, 5 files",
        "view: fetched assets/box.svg image/svg+xml",
        "view: kind=lua version=1.0",
        r#"view: title="Containers" program="containers-lua""#,
        "view: icons png 64x64",
        "view: -> subscribe id=1 stats",
        "host: msg id=* subscribe=1 stats",
        "program: msg id=* push=1",
        r#"view: <- push id=1 value={"readings":[{"cpu":"#,
        "view: -> call id=2 list",
        r#"view: <- response id=2 result={"containers":[{"id":"web","image":"nginx:1.27""#,
        r#"view: -> call id=3 toggle {"id":"web"}"#,
        r#"view: <- response id=3 result={"id":"web","state":"exited"}"#,
        r#"view: -> call id=4 toggle {"id":"nobody"}"#,
        "view: <- response id=4 error code=not-found",
        "view: -> call id=5 unheard",
        "view: <- response id=5 error code=not-found",
        "host: the person presses ^C",
        "program: bye id=* reason=signal",
        "view: destroyed",
        "host: closed id=* re=* reason=program",
        "screen: alternate screen off",
        "host: sees the tty after the screen: canonical=true echo=true",
        "program: exited with status 130",
    ]);
    run.said_after_the_screen("containers-lua: the view ended (signal)");
    assert_eq!(run.count("program: file"), 5, "{}", run.text());
    assert!(
        !run.text().contains("host: cannot read the tty's modes"),
        "{}",
        run.text()
    );
}

#[test]
fn the_built_lua_app_prints_its_text_where_no_terminal_hosts_pencere() {
    let app = lua_example().to_string_lossy().into_owned();
    let run = run(&["--no-host"], &[&app]);
    assert!(run.status.success(), "{}", run.text());
    run.in_order(&["host: does not host pencere: no answer"]);
    // The fallback ran: the list, as text.
    assert!(
        run.output_has("NAME         IMAGE").is_some(),
        "{}",
        run.text()
    );
    assert!(run.output_has("nginx:1.27").is_some(), "{}", run.text());
}

#[test]
fn the_built_lua_app_sends_print_and_errors_to_the_file_it_is_told() {
    let log = fresh("lua-log").join("app.log");
    let app = lua_example().to_string_lossy().into_owned();
    let run = run(
        &["--then", "interrupt"],
        &[&app, "--log", &log.to_string_lossy()],
    );
    assert!(run.status.success(), "{}", run.text());
    // `--no-host` is the fallback test's; here the view is shown, and the text is not printed.
    assert!(
        run.output_has("NAME         IMAGE").is_none(),
        "the list was printed over the view"
    );
}

#[test]
fn an_app_made_by_new_and_built_is_a_program_that_shows_a_view() {
    let directory = fresh("new-lua");
    let (status, said) = pencere(&directory, &["new", "greeter", "--lang", "lua"]);
    assert!(status.success(), "pencere new: {said}");
    let project = directory.join("greeter");
    assert!(project.join("app.lua").is_file());
    let (status, said) = pencere(&project, &["build"]);
    assert!(status.success(), "pencere build: {said}");
    let app = project.join("greeter");
    assert_signed(&app);
    let run = run(
        &[
            "--subscribe",
            "clock",
            "--call",
            r#"hello={"name":"you"}"#,
            "--then",
            "interrupt",
        ],
        &[&app.to_string_lossy()],
    );
    assert!(run.status.success(), "{}", run.text());
    run.in_order(&[
        r#"program: open id=2 view=lua version=1.0 title="greeter" bundle of 2 files"#,
        "view: -> subscribe id=1 clock",
        "program: msg id=* push=1",
        r#"view: <- push id=1 value={"now":"#,
        r#"view: -> call id=2 hello {"name":"you"}"#,
        r#"view: <- response id=2 result={"message":"Hello, you!"}"#,
        "host: the person presses ^C",
        "program: exited with status 130",
    ]);
}

#[test]
fn a_page_that_does_not_compile_is_refused_when_the_app_is_built() {
    let directory = fresh("broken-lua");
    let (status, said) = pencere(&directory, &["new", "broken", "--lang", "lua"]);
    assert!(status.success(), "{said}");
    let project = directory.join("broken");
    fs::write(project.join("lib.lua"), "local = nothing").expect("a broken script");
    let (status, said) = pencere(&project, &["build"]);
    assert!(!status.success());
    assert!(said.contains("lib.lua"), "{said}");
    assert!(!project.join("broken").exists(), "nothing was made");
}

/// A change to the files the program is run from.
type Edit = Box<dyn FnOnce() + Send>;

/// Runs the program in the pane of the stand-in host, and, as the record gets a line with each
/// marker in turn, makes the edit that goes with it (a change to the files the program is run
/// from). The record, as `run` has it.
fn run_editing(options: &[&str], program: &[&str], edits: Vec<(&str, Edit)>) -> Run {
    let mut child = Command::new(FAKE_HOST)
        .args(["--limit", "30", "--passthrough"])
        .args(options)
        .arg("--")
        .args(program)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the fake host starts");
    let mut stdout = child.stdout.take().expect("a pipe");
    let mut stderr = child.stderr.take().expect("a pipe");
    let (tell, lines) = channel::<String>();
    let output = std::thread::spawn(move || {
        let mut bytes = Vec::new();
        let _ = stdout.read_to_end(&mut bytes);
        bytes
    });
    let record = std::thread::spawn(move || {
        let mut text = Vec::new();
        let mut chunk = [0u8; 4096];
        let mut pending = String::new();
        loop {
            match stderr.read(&mut chunk) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    let got = chunk.get(..n).unwrap_or_default();
                    text.extend_from_slice(got);
                    pending.push_str(&String::from_utf8_lossy(got));
                    while let Some(at) = pending.find('\n') {
                        let line: String = pending.drain(..=at).collect();
                        let _ = tell.send(line.trim_end().to_owned());
                    }
                }
            }
        }
        text
    });
    // Each edit waits for its marker to be in the record, and for the view to be quiet a moment.
    for (seen, (marker, edit)) in edits.into_iter().enumerate() {
        let mut found = false;
        while let Ok(line) = lines.recv_timeout(Duration::from_secs(20)) {
            if line.contains(marker) {
                found = true;
                break;
            }
        }
        assert!(found, "no {marker:?} in the record (seen {seen})");
        std::thread::sleep(Duration::from_millis(600));
        edit();
    }
    let status = child.wait().expect("the fake host ends");
    let events = String::from_utf8_lossy(&record.join().expect("the record"))
        .lines()
        .map(str::to_owned)
        .collect();
    Run {
        events,
        output: output.join().expect("the output"),
        status,
    }
}

fn copy_dir(from: &Path, to: &Path) {
    fs::create_dir_all(to).expect("a directory");
    for entry in fs::read_dir(from).expect("a directory to copy") {
        let entry = entry.expect("an entry");
        let target = to.join(entry.file_name());
        if entry.file_type().expect("a type").is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), target).expect("a copy");
        }
    }
}

#[test]
fn pencere_dev_shows_the_view_again_when_the_page_changes_and_runs_the_app_again_when_a_script_does()
 {
    let directory = fresh("dev-lua");
    copy_dir(&root().join("examples/containers-lua"), &directory);
    let page = directory.join("page.lua");
    let app = directory.join("app.lua");
    let (page_edit, app_edit) = (page.clone(), app.clone());
    let command = pencere_command().to_string_lossy().into_owned();
    let run = run_editing(
        &[
            "--subscribe",
            "stats",
            "--on-view",
            "3",
            "--then",
            "interrupt",
        ],
        &[&command, "dev", &directory.to_string_lossy()],
        vec![
            (
                "view: kind=lua version=1.0",
                Box::new(move || {
                    let mut text = fs::read_to_string(&page_edit).expect("the page");
                    text.push_str("\n-- edited\n");
                    fs::write(&page_edit, text).expect("the page is written");
                }),
            ),
            (
                "program: dismiss id=",
                Box::new(move || {
                    // Once the page has been shown again: a script is changed.
                    std::thread::sleep(Duration::from_millis(1500));
                    let mut text = fs::read_to_string(&app_edit).expect("the app");
                    text.push_str("\n-- edited\n");
                    fs::write(&app_edit, text).expect("the app is written");
                }),
            ),
        ],
    );
    assert!(run.status.success(), "{}", run.text());
    run.in_order(&[
        "program: hello id=1",
        "program: open id=2 view=lua version=1.0",
        "view: kind=lua version=1.0",
        // The page changed: the view is closed by the program and shown again, of the new file.
        "program: dismiss id=",
        "host: the program asks to close its view",
        "view: destroyed",
        "host: closed id=* re=* reason=program",
        "program: hello id=",
        "program: open id=* view=lua version=1.0",
        "view: kind=lua version=1.0",
        // A script changed: the view is closed the same way, and the program starts again.
        "program: dismiss id=",
        "view: destroyed",
        "host: closed id=* re=* reason=program",
        "program: hello id=",
        "program: open id=* view=lua version=1.0",
        "view: kind=lua version=1.0",
        "host: the person presses ^C",
        "program: bye id=* reason=signal",
        "program: exited with status 130",
    ]);
    // Two of the three views were not the first: the screen was taken once by the first and kept
    // by the second; the program that started again took it anew.
    assert_eq!(run.count("program: open"), 3, "{}", run.text());
    assert_eq!(run.count("answers CSI c"), 2, "{}", run.text());
}

#[test]
fn an_app_with_a_web_page_made_by_new_opens_a_web_view_and_its_calls_and_pushes_work() {
    let directory = fresh("new-web");
    let (status, said) = pencere(&directory, &["new", "webby", "--page", "web"]);
    assert!(status.success(), "pencere new: {said}");
    let project = directory.join("webby");
    // The bridge to the program is the SDK's page module, as the build of the command took it.
    let bridge = fs::read_to_string(project.join("web/pencere-page.js")).expect("the bridge");
    let sdk =
        fs::read_to_string(root().join("sdk/typescript/dist/page/index.js")).expect("the SDK");
    assert_eq!(
        bridge.trim_end(),
        sdk.lines()
            .filter(|line| !line.starts_with("//# sourceMappingURL="))
            .collect::<Vec<_>>()
            .join("\n")
            .trim_end(),
        "the project carries the SDK's page module"
    );
    let (status, said) = pencere(&project, &["build"]);
    assert!(status.success(), "pencere build: {said}");
    let app = project.join("webby");
    assert_signed(&app);
    let run = run(
        &[
            "--subscribe",
            "clock",
            "--call",
            r#"hello={"name":"web"}"#,
            "--then",
            "interrupt",
        ],
        &[&app.to_string_lossy()],
    );
    assert!(run.status.success(), "{}", run.text());
    run.in_order(&[
        "host: welcome id=1 re=1 version=1 with a token features=dismiss views=web@1.0",
        r#"program: open id=2 view=web version=1.0 title="webby" bundle of 3 files"#,
        "view: started from the bundle: entry index.html, 3 files",
        "view: kind=web version=1.0",
        "view: -> subscribe id=1 clock",
        "program: msg id=* push=1",
        r#"view: <- push id=1 value={"now":"#,
        r#"view: -> call id=2 hello {"name":"web"}"#,
        r#"view: <- response id=2 result={"message":"Hello, web!"}"#,
        "host: the person presses ^C",
        "program: exited with status 130",
    ]);
}

#[test]
fn pencere_dev_shows_a_web_page_again_when_one_of_its_files_changes() {
    let directory = fresh("dev-web");
    let (status, said) = pencere(&directory, &["new", "webdev", "--page", "web"]);
    assert!(status.success(), "pencere new: {said}");
    let project = directory.join("webdev");
    let script = project.join("web/app.js");
    let command = pencere_command().to_string_lossy().into_owned();
    let run = run_editing(
        &[
            "--subscribe",
            "clock",
            "--on-view",
            "2",
            "--then",
            "interrupt",
        ],
        &[&command, "dev", &project.to_string_lossy()],
        vec![(
            "view: kind=web version=1.0",
            Box::new(move || {
                let mut text = fs::read_to_string(&script).expect("the page script");
                text.push_str("\n// edited\n");
                fs::write(&script, text).expect("the page script is written");
            }),
        )],
    );
    assert!(run.status.success(), "{}", run.text());
    run.in_order(&[
        "program: open id=2 view=web version=1.0",
        "view: kind=web version=1.0",
        "program: dismiss id=",
        "host: the program asks to close its view",
        "host: closed id=* re=* reason=program",
        "program: hello id=",
        "program: open id=* view=web version=1.0",
        // The files the page did not change are in the terminal's store already.
        "host: need id=* re=* missing=1",
        "view: kind=web version=1.0",
        "host: the person presses ^C",
        "program: exited with status 130",
    ]);
    assert_eq!(run.count("program: open"), 2, "{}", run.text());
}

#[test]
fn an_app_that_hands_the_terminal_to_a_program_gets_it_back_and_shows_its_view_again() {
    let directory = fresh("handoff");
    fs::write(
        directory.join("app.lua"),
        r#"
        local pencere = require("pencere")
        local handed = false
        pencere.serve {
            page = "page.lua",
            calls = {
                -- The first time, the person's shell stands in: a program that says something and ends.
                shell = function()
                    if handed then
                        return { skipped = true }
                    end
                    handed = true
                    local result = pencere.handoff { "sh", "-c", "echo from-the-child; exit 7" }
                    print("child ended " .. tostring(result.code))
                    return { code = result.code }
                end,
            },
        }
        "#,
    )
    .expect("an app");
    fs::write(
        directory.join("page.lua"),
        "return { render = function() return pencere.div() end }",
    )
    .expect("a page");
    let app = directory.join("handoff-app");
    let (status, said) = pencere(&directory, &["build", ".", "-o", &app.to_string_lossy()]);
    assert!(status.success(), "pencere build: {said}");
    let log = directory.join("app.log");
    let run = run(
        &["--call", "shell", "--on-view", "2", "--then", "interrupt"],
        &[&app.to_string_lossy(), "--log", &log.to_string_lossy()],
    );
    assert!(run.status.success(), "{}", run.text());
    run.in_order(&[
        "program: open id=2 view=lua version=1.0",
        "view: -> call id=1 shell",
        // The view is closed by the program, and the terminal is the program's.
        "program: dismiss id=",
        "view: destroyed",
        "host: closed id=* re=* reason=program",
        // Taken back: a new view, asked for as for a first one.
        "program: hello id=",
        "program: open id=* view=lua version=1.0",
        "view: -> call id=1 shell",
        r#"view: <- response id=1 result={"skipped":true}"#,
        "host: the person presses ^C",
        "program: bye id=* reason=signal",
        "program: exited with status 130",
    ]);
    // What the program wrote went to the screen the person had, not to the view's: after the
    // alternate screen was left and before it was entered again.
    let text = String::from_utf8_lossy(&run.output).into_owned();
    let said = text.find("from-the-child").expect("the program's words");
    let left = text.find("\x1b[?1049l").expect("the screen is given back");
    let entered_again = text
        .rfind("\x1b[?1049h")
        .expect("the screen is taken again");
    assert!(left < said && said < entered_again, "{text:?}");
    assert_eq!(text.matches("\x1b[?1049h").count(), 2, "{text:?}");
    let logged = fs::read_to_string(&log).expect("the log");
    assert!(logged.contains("child ended 7"), "{logged}");
}
