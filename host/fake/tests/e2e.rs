//! End to end, on a real pty: small programs written against the TypeScript SDK
//! (`tests/programs`, which import it from its build, made by `npx tsc -b`, which the
//! gate runs before these) against the stand-in host, which holds the `pencere` crate's
//! rules and a scripted page.
//!
//! What is read here is the record the host writes to stderr, the bytes the program wrote
//! to its tty, and how the processes ended. Nothing goes to a network, and every file
//! written is under the target directory.

// A test fails by panicking, and the helpers here are the test's.
#![allow(clippy::expect_used, clippy::panic)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};

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

/// The containers example (`examples/containers`), built by `npx tsc -b`, run in the pane.
fn run_example(options: &[&str]) -> Run {
    let main = root().join("examples/containers/dist/main.js");
    assert!(
        main.is_file(),
        "{} is missing: build the TypeScript half first with `npx tsc -b` (make check does)",
        main.display()
    );
    run(options, &["node", &main.to_string_lossy()])
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
        "host: welcome id=1 re=1 version=1 with a token features=bundle,dismiss views=web@1.0",
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
        "host: welcome id=1 re=1 version=1 with a token features=bundle,dismiss views=web@1.0",
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
        "host: welcome id=* version=1 with a token (restart) features=bundle,dismiss views=web@1.0",
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
fn pipeline(dir: &Path, options: &[&str]) -> (Run, String, String) {
    let out = dir.join("out.txt");
    let status = dir.join("status");
    let script = format!(
        r#"printf 'web\napi\ndb\n' | node {} > {}; echo $? > {}"#,
        quoted(&program("filter.mjs")),
        quoted(&out.to_string_lossy()),
        quoted(&status.to_string_lossy()),
    );
    let run = run(options, &["sh", "-c", &script]);
    let out = fs::read_to_string(&out).expect("the pipeline wrote its output");
    let status = fs::read_to_string(&status).expect("the pipeline wrote its status");
    (run, out, status.trim().to_owned())
}

#[test]
fn a_filter_asks_for_its_view_on_the_terminal_and_writes_what_was_chosen_to_the_pipe() {
    let dir = fresh("filter-chosen");
    let (run, out, status) = pipeline(
        &dir,
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

#[test]
fn a_filter_whose_view_closes_with_nothing_chosen_writes_nothing_and_fails() {
    let dir = fresh("filter-nothing");
    let (run, out, status) = pipeline(&dir, &["--call", "rows", "--then", "close"]);
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

// ---- the example ----------------------------------------------------------------

#[test]
fn the_containers_example_opens_answers_a_call_and_pushes_to_its_page() {
    let run = run_example(&[
        "--subscribe",
        r#"stats={"every":20}"#,
        "--call",
        "list",
        "--call",
        r#"stop={"id":"web"}"#,
        "--then",
        "interrupt",
    ]);
    assert!(run.status.success(), "{}", run.text());
    run.in_order(&[
        "host: welcome id=1 re=1 version=1 with a token features=bundle,dismiss views=web@1.0",
        r#"program: open id=2 view=web version=1.0 title="Containers" bundle of 3 files"#,
        "view: started from the bundle: entry index.html, 3 files",
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
        "host: the person presses ^C",
        "program: bye id=* reason=signal",
        "view: destroyed",
        "host: closed id=* re=* reason=program",
        "screen: alternate screen off",
        "program: was killed by signal 2",
    ]);

    // The files the page would fetch are the files on disk, whole.
    for (name, media_type) in [
        ("index.html", "text/html"),
        ("app.js", "text/javascript"),
        ("style.css", "text/css"),
    ] {
        let size = fs::metadata(root().join("examples/containers/view").join(name))
            .expect("a file of the view")
            .len();
        let wanted = format!("view: fetched {name} {media_type} {size} bytes");
        assert!(
            run.events.contains(&wanted),
            "no {wanted:?}:\n{}",
            run.text()
        );
    }
}

#[test]
fn the_containers_example_prints_its_containers_where_no_view_can_be_shown() {
    let run = run_example(&["--no-host"]);
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
