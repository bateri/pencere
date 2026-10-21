//! End to end, on a real pty: the example tool (`examples/containers`, built
//! with `npx tsc -b`, which the gate runs before these) against the stand-in
//! host, which holds the `pencere` crate's rules and a scripted view.
//!
//! What is read here is the record the host writes to stderr, the bytes the
//! program wrote to its tty, and how the processes ended. Nothing goes to a
//! network, and every file written is under the target directory.

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

fn tool() -> String {
    let main = root().join("examples/containers/dist/main.js");
    assert!(
        main.is_file(),
        "{} is missing: build the TypeScript half first with `npx tsc -b` (make check does)",
        main.display()
    );
    main.to_string_lossy().into_owned()
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

fn run_tool(options: &[&str]) -> Run {
    run(options, &["node", &tool()])
}

fn view_file_size(name: &str) -> u64 {
    fs::metadata(root().join("examples/containers/view").join(name))
        .expect("a file of the view")
        .len()
}

#[test]
fn a_tool_goes_from_the_question_to_the_interrupt() {
    let store = fresh("first");
    let store = store.to_string_lossy();
    let run = run_tool(&[
        "--store",
        &store,
        "--call",
        r#"stop_container={"id":"cache"}"#,
        "--then",
        "interrupt",
    ]);
    assert!(run.status.success(), "{}", run.text());
    run.in_order(&[
        "program: hello id=1",
        // The host looked at the tty before it answered (§6.2): the SDK had made
        // it non-canonical with echo off.
        "host: sees the tty: canonical=false echo=false",
        "host: welcome id=1 re=1 version=1 with a token",
        // The answer to the question comes before the reply to `CSI c` (§5.2).
        "host: answers CSI c",
        "screen: alternate screen on",
        "program: open id=2 tool=list_containers bundle of 3 files",
        "host: need id=2 re=2 missing=3",
        "program: file",
        "program: file",
        "program: file",
        "host: msg id=* request initialize",
        "host: msg id=* notification notifications/initialized",
        "request tools/list",
        "view: started from the bundle: entry index.html, 3 files",
        // What the strip above the view says: the tool's title and the server's
        // name, where a terminal would otherwise show `list_containers` and `node`.
        r#"view: title="Containers" program="containers""#,
        "view: <- ui/initialize result: theme=dark displayMode=inline container=640x384 pane=true",
        "view: -> ui/notifications/initialized",
        "view: <- ui/notifications/tool-input",
        "host: msg id=* request tools/call list_containers",
        "view: <- ui/notifications/tool-result",
        r#"view: -> tools/call stop_container {"id":"cache"}"#,
        "host: msg id=* request tools/call stop_container",
        r#"view: <- tool result "Stopped cache (redis:7)""#,
        "host: the person presses ^C",
        "program: bye id=* reason=signal",
        "view: <- ui/resource-teardown (request)",
        "view: destroyed",
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
fn a_second_run_finds_the_bundle_in_the_store_and_the_person_can_close_the_view() {
    let store = fresh("second");
    let store = store.to_string_lossy();
    let first = run_tool(&["--store", &store, "--then", "close"]);
    first.in_order(&["host: need id=2 re=2 missing=3", "program: file"]);

    let second = run_tool(&["--store", &store, "--then", "close"]);
    assert!(second.status.success(), "{}", second.text());
    second.in_order(&[
        "host: need id=2 re=2 missing=0",
        "view: started from the bundle: entry index.html, 3 files",
        "view: fetched app.js",
        "view: <- ui/notifications/tool-result",
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
        .output_has("containers: the view closed (user)")
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
    let first = run_tool(&["--store", &store, "--then", "close"]);
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

    let second = run_tool(&["--store", &store, "--then", "close"]);
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
fn a_message_far_larger_than_the_ttys_input_queue_gets_through() {
    // The tty takes about a kilobyte of input at a time (specification Appendix B):
    // the host writes the message's frames as the tty takes them, none inside
    // another, and the program's answer comes back in frames of its own.
    let id = "a".repeat(60_000);
    let call = format!(r#"stop_container={{"id":"{id}"}}"#);
    let run = run_tool(&["--call", &call, "--then", "interrupt"]);
    assert!(run.status.success(), "{}", run.text());
    run.in_order(&[
        "view: -> tools/call stop_container",
        "host: msg id=* request tools/call stop_container",
        "program: msg id=* response",
        r#"view: <- tool result error "There is no container \"aaaa"#,
        "program: bye id=* reason=signal",
    ]);
    run.never("dropped a frame");
}

#[test]
fn a_tool_that_is_killed_leaves_no_view_behind() {
    let run = run_tool(&["--then", "kill"]);
    run.in_order(&[
        "view: <- ui/notifications/tool-result",
        "host: kills the program with SIGKILL",
        "program: was killed by signal 9",
        // Nobody said goodbye: the host saw the stream end.
        "view: <- ui/resource-teardown (request)",
        "view: destroyed",
    ]);
    run.never("program: bye");
    assert!(run.status.success(), "{}", run.text());
}

#[test]
fn a_tool_closes_its_view_shows_another_and_closes_that_one_and_goes_on() {
    let program = root().join("host/fake/tests/programs/reopen.mjs");
    let run = run(&[], &["node", &program.to_string_lossy()]);
    assert!(run.status.success(), "{}", run.text());
    run.in_order(&[
        "host: welcome id=1 re=1 version=1 with a token features=bundle,dismiss",
        "program: open id=2 tool=show bundle of 1 files",
        "host: need id=2 re=2 missing=1",
        "program: file",
        "view: started from the bundle: entry index.html, 1 files",
        // The tool has no title: the terminal names the view by the tool.
        r#"view: title=none program="reopen""#,
        "host: msg id=* request tools/call show",
        // The program closes the view that runs; the host tells the view first and
        // the program after the view is gone.
        "program: dismiss id=",
        "host: the program asks to close its view",
        "view: <- ui/resource-teardown (request)",
        "view: destroyed",
        "host: closed id=* re=* reason=program",
        // A new token and a new open, as for a first view: the file that changed
        // crosses the stream again.
        "program: hello id=",
        "host: welcome id=* re=* version=1 with a token",
        "program: open id=* tool=show bundle of 1 files",
        "host: need id=* re=* missing=1",
        "program: file",
        "view: started from the bundle: entry index.html, 1 files",
        "host: msg id=* request tools/call show",
        "program: dismiss id=",
        "view: destroyed",
        "host: closed id=* re=* reason=program",
        "program: exited with status 0",
    ]);
    // The program asked for no second capability query and no second screen.
    assert_eq!(run.count("program: hello"), 2, "{}", run.text());
    assert_eq!(run.count("answers CSI c"), 1, "{}", run.text());
    run.never("program: bye");
    run.never("dropped a frame");
    // It said how it went, on the screen it was given back.
    for line in [
        r#"reopen: {"kind":"reopened"}"#,
        r#"close: {"kind":"closed"}"#,
        r#"serve: {"kind":"closed","by":"program"}"#,
    ] {
        let at = run
            .output_has(line)
            .unwrap_or_else(|| panic!("no {line:?}:\n{}", run.text()));
        let off = run
            .output_has("\x1b[?1049l")
            .expect("it gave the screen back");
        assert!(off < at, "{line}");
    }
    assert_eq!(
        String::from_utf8_lossy(&run.output)
            .matches("\x1b[?1049h")
            .count(),
        1
    );
}

#[test]
fn a_terminal_that_does_not_host_pencere_gets_the_text_interface() {
    let run = run_tool(&["--no-host"]);
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
    // It never took the screen: there was nothing to put on it.
    assert!(run.output_has("\x1b[?1049h").is_none());
}

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
            r#"{stty}printf '\033_pencere;hello;id=1;{{"versions":[1]}}\033\\'
printf '\033[c'
sleep 0.3
printf '\033[?1049h'
printf '\033_pencere;open;id=2;{{"token":"AAAAAAAAAAAAAAAAAAAAAA","tool":{{"name":"x\\u001b[2J","arguments":{{}}}}}}\033\\'
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
        // The tool's name has an escape in it; the record writes it out and does
        // not put it on the person's terminal.
        "program: open id=2 tool=x\\u{1b}[2J without a bundle",
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
printf '\033_pencere;hello;id=1;{"versions":[1]}\033\\'
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

// ---- `pencere run`: a server that knows nothing of pencere --------------------

fn cli() -> String {
    let main = root().join("cli/dist/main.js");
    assert!(
        main.is_file(),
        "{} is missing: build the TypeScript half first with `npx tsc -b` (make check does)",
        main.display()
    );
    main.to_string_lossy().into_owned()
}

/// `pencere <command> [own] -- <server>` in the pane of the stand-in host.
fn run_command(command: &str, options: &[&str], own: &[&str], server: &[&str]) -> Run {
    let main = cli();
    let mut program = vec!["node", main.as_str(), command];
    program.extend_from_slice(own);
    program.push("--");
    program.extend_from_slice(server);
    run(options, &program)
}

/// `pencere run [own] -- <server>` in the pane of the stand-in host.
fn run_cli(options: &[&str], own: &[&str], server: &[&str]) -> Run {
    run_command("run", options, own, server)
}

/// `pencere dev [own] -- <server>` in the pane of the stand-in host.
fn run_dev(options: &[&str], own: &[&str], server: &[&str]) -> Run {
    run_command("dev", options, own, server)
}

/// The example tool as a chat client would start it, through a shell that writes
/// its pid to `log` first (`exec` keeps the pid), so that a test can count how
/// often the server was started and see that none is left. The words of `node`
/// come before the tool's path.
fn logged_tool(log: &Path, node_options: &[&str]) -> Vec<String> {
    let mut server = vec![
        "sh".to_owned(),
        "-c".to_owned(),
        r#"echo $$ >> "$0"; exec node "$@""#.to_owned(),
        log.to_string_lossy().into_owned(),
    ];
    server.extend(node_options.iter().map(|word| (*word).to_owned()));
    server.push(tool());
    server
}

fn words(server: &[String]) -> Vec<&str> {
    server.iter().map(String::as_str).collect()
}

/// The pids of the servers that were started.
fn started(log: &Path) -> Vec<u32> {
    fs::read_to_string(log)
        .unwrap_or_default()
        .lines()
        .map(|line| line.trim().parse().expect("a pid"))
        .collect()
}

/// Whether the process is gone, given the time a process table takes to catch up.
fn gone(pid: u32) -> bool {
    for _ in 0..100 {
        let exists = Command::new("kill")
            .args(["-0", &pid.to_string()])
            .stderr(Stdio::null())
            .status()
            .expect("kill runs")
            .success();
        if !exists {
            return true;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    false
}

#[test]
fn pencere_run_opens_the_view_of_a_server_and_ends_it_on_ctrl_c() {
    let dir = fresh("cli-first");
    let log = dir.join("starts");
    let server = logged_tool(&log, &[]);
    let run = run_cli(
        &[
            "--call",
            r#"stop_container={"id":"cache"}"#,
            "--then",
            "interrupt",
        ],
        &[],
        &words(&server),
    );
    assert!(run.status.success(), "{}", run.text());
    run.in_order(&[
        "program: hello id=1",
        "host: welcome id=1 re=1 version=1 with a token features=bundle,dismiss",
        "screen: alternate screen on",
        // Asked which tool has a view, so the open names it; the server gave no
        // files, so the host reads the view from the server's own resource.
        "program: open id=2 tool=list_containers without a bundle",
        "host: msg id=* request initialize",
        "host: msg id=* notification notifications/initialized",
        "host: msg id=* request tools/list",
        "host: msg id=* request resources/read",
        "view: started from resources/read",
        r#"view: title="Containers" program="containers""#,
        r#"view: -> tools/call stop_container {"id":"cache"}"#,
        "host: msg id=* request tools/call stop_container",
        "program: msg id=* response",
        r#"view: <- tool result "Stopped cache (redis:7)""#,
        "host: the person presses ^C",
        "program: bye id=* reason=signal",
        "view: destroyed",
        "screen: alternate screen off",
        "program: was killed by signal 2",
    ]);
    // Once to ask which tool has a view, once for the view: and neither is left.
    let pids = started(&log);
    assert_eq!(pids.len(), 2, "{pids:?}");
    for pid in pids {
        assert!(gone(pid), "the server {pid} outlived pencere run");
    }
    // The screen it gave back has nothing on it that was not the program's own.
    assert!(run.output_has("pencere:").is_none());
}

#[test]
fn pencere_run_lets_go_of_its_server_when_the_terminal_hangs_up() {
    // ssh going away: the pane's pty is closed, the program's session is hung up and every write
    // it makes to its tty fails. A server that does not leave at the end of its stdin would be left
    // running if `pencere run` were ended by the hangup and not by itself.
    let dir = fresh("cli-hangup");
    let log = dir.join("starts");
    let linger = root().join("host/fake/tests/programs/linger.cjs");
    let linger = linger.to_string_lossy();
    let server = logged_tool(&log, &["--require", &linger]);
    let run = run_cli(
        &["--then", "hangup"],
        &["--tool", "list_containers", "--args", "{}"],
        &words(&server),
    );
    assert!(run.status.success(), "{}", run.text());
    run.in_order(&[
        "program: open id=2 tool=list_containers without a bundle",
        "view: started from resources/read",
        "host: hangs up the terminal",
        // Ended by the signal it was sent, as any program is, once it had let go of the server.
        "program: was killed by signal 1",
    ]);
    let pids = started(&log);
    assert_eq!(pids.len(), 1, "{pids:?}");
    for pid in pids {
        assert!(gone(pid), "the server {pid} outlived the hangup");
    }
}

#[test]
fn pencere_run_does_not_start_a_server_to_ask_when_the_tool_is_named() {
    let dir = fresh("cli-named");
    let log = dir.join("starts");
    let server = logged_tool(&log, &[]);
    let run = run_cli(
        &["--then", "close"],
        &["--tool", "list_containers", "--args", "{}"],
        &words(&server),
    );
    assert!(run.status.success(), "{}", run.text());
    run.in_order(&[
        "program: open id=2 tool=list_containers without a bundle",
        "view: started from resources/read",
        "host: the person closes the view",
        "host: closed id=* reason=user",
        "program: exited with status 0",
    ]);
    let pids = started(&log);
    assert_eq!(pids.len(), 1, "{pids:?}");
    assert!(gone(pids[0]));
}

#[test]
fn pencere_run_sends_the_files_of_a_view_directory_once() {
    let dir = fresh("cli-view");
    let store = dir.join("store");
    let store = store.to_string_lossy();
    let view = root().join("examples/containers/view");
    let view = view.to_string_lossy();
    let log = dir.join("starts");
    let server = logged_tool(&log, &[]);
    let options = ["--store", &store, "--then", "close"];
    let own = ["--tool", "list_containers", "--view", &view];

    let first = run_cli(&options, &own, &words(&server));
    assert!(first.status.success(), "{}", first.text());
    first.in_order(&[
        "program: open id=2 tool=list_containers bundle of 3 files",
        "host: need id=2 re=2 missing=3",
        "program: file",
        "program: file",
        "program: file",
        "view: started from the bundle: entry index.html, 3 files",
    ]);

    // The terminal kept them: the second time only the manifest crosses the stream.
    let second = run_cli(&options, &own, &words(&server));
    assert!(second.status.success(), "{}", second.text());
    second.in_order(&[
        "host: need id=2 re=2 missing=0",
        "view: started from the bundle: entry index.html, 3 files",
    ]);
    second.never("program: file");
}

#[test]
fn pencere_run_starts_the_server_again_when_the_host_starts_the_view_again() {
    let dir = fresh("cli-restart");
    let log = dir.join("starts");
    let server = logged_tool(&log, &[]);
    let run = run_cli(
        &["--then", "restart"],
        &["--tool", "list_containers"],
        &words(&server),
    );
    assert!(run.status.success(), "{}", run.text());
    run.in_order(&[
        "program: open id=2 tool=list_containers without a bundle",
        "view: started from resources/read",
        "host: starts the view again, as after a reconnection",
        "view: destroyed",
        "host: welcome id=* version=1 with a token (restart)",
        "program: open id=* tool=list_containers without a bundle",
        "view: started from resources/read",
        "host: the person presses ^C",
        "program: bye id=* reason=signal",
        "program: was killed by signal 2",
    ]);
    assert_eq!(run.count("program: open"), 2, "{}", run.text());
    // A server has one `initialize` for the life of its process: the second view
    // is a second process, and the first is gone.
    let pids = started(&log);
    assert_eq!(pids.len(), 2, "{pids:?}");
    assert_ne!(pids[0], pids[1]);
    for pid in pids {
        assert!(gone(pid), "the server {pid} outlived pencere run");
    }
}

#[test]
fn pencere_run_keeps_the_terminals_ctrl_c_from_the_server() {
    // ⌃C is the terminal's signal to its foreground group. The server is the
    // program's to end, with `bye` first: one that took the signal too would die
    // under the view, and a Python server would print a traceback.
    let dir = fresh("cli-group");
    let log = dir.join("starts");
    let signals = dir.join("signals");
    let preload = dir.join("record-sigint.cjs");
    fs::write(
        &preload,
        format!(
            r#"process.on("SIGINT", () => require("node:fs").appendFileSync({signals:?}, "SIGINT\n"));"#,
            signals = signals.to_string_lossy()
        ),
    )
    .expect("a preload under the target directory");
    let preload = preload.to_string_lossy().into_owned();
    let server = logged_tool(&log, &["-r", &preload]);
    let run = run_cli(
        &["--then", "interrupt"],
        &["--tool", "list_containers"],
        &words(&server),
    );
    assert!(run.status.success(), "{}", run.text());
    run.in_order(&[
        "view: started from resources/read",
        "host: the person presses ^C",
        "program: bye id=* reason=signal",
        "program: was killed by signal 2",
    ]);
    let pids = started(&log);
    assert_eq!(pids.len(), 1, "{pids:?}");
    assert!(gone(pids[0]));
    assert!(
        !signals.exists(),
        "the server got the terminal's SIGINT: {:?}",
        fs::read_to_string(&signals)
    );
}

#[test]
fn pencere_run_tells_a_terminal_that_does_not_host_pencere_what_a_server_is_for() {
    let dir = fresh("cli-nohost");
    let log = dir.join("starts");
    let server = logged_tool(&log, &[]);
    let run = run_cli(
        &["--no-host"],
        &["--tool", "list_containers"],
        &words(&server),
    );
    assert!(run.status.success(), "{}", run.text());
    run.in_order(&[
        "program: hello id=1",
        "host: does not host pencere: no answer",
        "program: exited with status 3",
    ]);
    run.never("welcome");
    run.never("program: open");
    assert!(
        run.output_has("this terminal does not show pencere views")
            .is_some(),
        "{:?}",
        String::from_utf8_lossy(&run.output)
    );
    assert!(run.output_has("is an MCP server").is_some());
    // It never took the screen, and it started no server for a pane that cannot show it.
    assert!(run.output_has("\x1b[?1049h").is_none());
    assert!(started(&log).is_empty());
}

#[test]
fn pencere_run_says_what_a_server_that_ends_under_its_view_wrote() {
    let run = run_cli(
        &[],
        &["--tool", "list_containers"],
        &[
            "node",
            "-e",
            r#"console.error("Traceback: boom"); setTimeout(() => process.exit(5), 100);"#,
        ],
    );
    assert!(run.status.success(), "{}", run.text());
    run.in_order(&[
        "program: open id=2 tool=list_containers without a bundle",
        "program: bye id=* reason=exit",
        "screen: alternate screen off",
        "program: exited with status 1",
    ]);
    // It is said on the screen the person got back, not under the view.
    let off = run
        .output_has("\x1b[?1049l")
        .expect("the screen was given back");
    let said = run
        .output_has("ended (exit status 5) while its view was opening")
        .expect("it said that the server ended");
    let wrote = run
        .output_has("  Traceback: boom")
        .expect("it showed what the server wrote");
    assert!(off < said && said < wrote, "{off} {said} {wrote}");
}

#[test]
fn the_question_to_the_person_names_the_tool_and_the_program() {
    // The view of this server declares a domain, so the person is asked. The stand-in
    // person refuses, which is what it does unless it is told otherwise.
    let program = root().join("host/fake/tests/programs/declares-domains.mjs");
    let program = program.to_string_lossy();
    let run = run_cli(
        &["--then", "interrupt"],
        &["--tool", "show"],
        &["node", &program],
    );
    assert!(run.status.success(), "{}", run.text());
    run.in_order(&[
        "program: open id=2 tool=show without a bundle",
        "host: msg id=* request resources/read",
        // The tool's name, and the words the program has for it and for itself.
        r#"host: asked the person about 1 domains and 0 permissions of tool="show" title="Show it" program="Domain Desk": refused"#,
        "view: started from resources/read",
        r#"view: title="Show it" program="Domain Desk""#,
    ]);
}

// ---- `pencere dev`: the view is shown again when files change -------------------

/// `program` with `args` as a chat client would start it, through a shell that
/// writes its pid to `log` first, as `logged_tool` does for the example tool.
fn logged_program(log: &Path, program: &Path, args: &[&str]) -> Vec<String> {
    let mut server = vec![
        "sh".to_owned(),
        "-c".to_owned(),
        r#"echo $$ >> "$0"; exec node "$@""#.to_owned(),
        log.to_string_lossy().into_owned(),
        program.to_string_lossy().into_owned(),
    ];
    server.extend(args.iter().map(|word| (*word).to_owned()));
    server
}

fn writes_on_call() -> PathBuf {
    root().join("host/fake/tests/programs/writes-on-call.mjs")
}

#[test]
fn pencere_dev_shows_the_view_again_when_a_file_of_it_changes() {
    let dir = fresh("cli-dev-view");
    let view = dir.join("view");
    fs::create_dir(&view).expect("a view directory");
    fs::write(
        view.join("index.html"),
        "<!doctype html><link rel=stylesheet href=style.css>",
    )
    .expect("a page");
    let style = view.join("style.css");
    fs::write(&style, "body {}").expect("a stylesheet");
    let log = dir.join("starts");
    // Calling the tool edits the stylesheet, which is what a build does under a view.
    let server = logged_program(
        &log,
        &writes_on_call(),
        &[&style.to_string_lossy(), "body { color: red }"],
    );
    let view_arg = view.to_string_lossy();
    let run = run_dev(
        &["--then", "interrupt", "--on-view", "2"],
        &["--tool", "show", "--view", &view_arg],
        &words(&server),
    );
    assert!(run.status.success(), "{}", run.text());
    run.in_order(&[
        "program: hello id=1",
        "host: welcome id=1 re=1 version=1 with a token features=bundle,dismiss",
        "screen: alternate screen on",
        "program: open id=2 tool=show bundle of 2 files",
        "host: need id=2 re=2 missing=2",
        "program: file",
        "program: file",
        "view: started from the bundle: entry index.html, 2 files",
        "host: msg id=* request tools/call show",
        // The file changed under the view: the program closes it, and the host
        // tells the view first and the program after the view is gone.
        "program: dismiss id=",
        "host: the program asks to close its view",
        "view: destroyed",
        "host: closed id=* re=* reason=program",
        // A new token and a new open, as for a first view. The terminal holds the
        // page, so the stylesheet is the only file it asks for.
        "program: hello id=",
        "host: welcome id=* re=* version=1 with a token",
        "program: open id=* tool=show bundle of 2 files",
        "host: need id=* re=* missing=1",
        "program: file",
        "view: started from the bundle: entry index.html, 2 files",
        "host: msg id=* request tools/call show",
        "host: the person presses ^C",
        "program: bye id=* reason=signal",
        "screen: alternate screen off",
        "program: was killed by signal 2",
    ]);
    assert_eq!(run.count("program: file"), 3, "{}", run.text());
    assert_eq!(run.count("program: open"), 2, "{}", run.text());
    // The second view asks for no second capability query and no second screen,
    // and its server finds the file edited already: nothing makes a third.
    assert_eq!(run.count("program: hello"), 2, "{}", run.text());
    assert_eq!(run.count("program: dismiss"), 1, "{}", run.text());
    assert_eq!(run.count("answers CSI c"), 1, "{}", run.text());
    assert_eq!(
        String::from_utf8_lossy(&run.output)
            .matches("\x1b[?1049h")
            .count(),
        1
    );
    assert_eq!(
        fs::read_to_string(&style).expect("the stylesheet"),
        "body { color: red }"
    );
    // Each view is a server of its own, and neither outlives `pencere dev`.
    let pids = started(&log);
    assert_eq!(pids.len(), 2, "{pids:?}");
    assert_ne!(pids[0], pids[1]);
    for pid in pids {
        assert!(gone(pid), "the server {pid} outlived pencere dev");
    }
    assert!(run.output_has("pencere:").is_none());
}

#[test]
fn pencere_dev_starts_the_server_again_when_a_file_it_watches_changes() {
    let dir = fresh("cli-dev-watch");
    let watched = dir.join("settings");
    fs::write(&watched, "before").expect("a file the server reads");
    let log = dir.join("starts");
    let server = logged_program(
        &log,
        &writes_on_call(),
        &[&watched.to_string_lossy(), "after"],
    );
    let watched_arg = watched.to_string_lossy();
    let run = run_dev(
        &["--then", "interrupt", "--on-view", "2"],
        &["--tool", "show", "--watch", &watched_arg],
        &words(&server),
    );
    assert!(run.status.success(), "{}", run.text());
    // No view directory: the terminal reads the page from the server each time.
    run.in_order(&[
        "program: open id=2 tool=show without a bundle",
        "view: started from resources/read",
        "host: msg id=* request tools/call show",
        "program: dismiss id=",
        "host: closed id=* re=* reason=program",
        "program: hello id=",
        "program: open id=* tool=show without a bundle",
        "view: started from resources/read",
        "host: the person presses ^C",
        "program: bye id=* reason=signal",
        "program: was killed by signal 2",
    ]);
    run.never("program: file");
    assert_eq!(run.count("program: open"), 2, "{}", run.text());
    assert_eq!(run.count("program: dismiss"), 1, "{}", run.text());
    assert_eq!(fs::read_to_string(&watched).expect("the file"), "after");
    let pids = started(&log);
    assert_eq!(pids.len(), 2, "{pids:?}");
    for pid in pids {
        assert!(gone(pid), "the server {pid} outlived pencere dev");
    }
}

#[test]
fn pencere_dev_names_a_path_to_watch_that_is_not_there_and_asks_the_terminal_for_nothing() {
    let dir = fresh("cli-dev-missing");
    let log = dir.join("starts");
    let server = logged_program(&log, &writes_on_call(), &["x", "y"]);
    let run = run_dev(
        &[],
        &["--tool", "show", "--watch", "/nonexistent/server.py"],
        &words(&server),
    );
    assert!(run.status.success(), "{}", run.text());
    run.in_order(&["program: exited with status 2"]);
    run.never("program: hello");
    assert!(
        run.output_has("--watch /nonexistent/server.py: no such file or directory")
            .is_some(),
        "{:?}",
        String::from_utf8_lossy(&run.output)
    );
    assert!(started(&log).is_empty());
}

#[test]
fn pencere_dev_says_what_the_server_wrote_and_waits_for_the_next_change_when_it_will_not_start_again()
 {
    // What saving a syntax error does: the first server runs, the one that is started for the
    // reopened view dies at once, and what the person saves next starts a server that runs.
    let dir = fresh("cli-dev-broken");
    let watched = dir.join("settings");
    fs::write(&watched, "before").expect("a file the server reads");
    let log = dir.join("starts");
    let server = vec![
        "sh".to_owned(),
        "-c".to_owned(),
        concat!(
            r#"echo $$ >> "$0"; "#,
            // The second start is the broken one: it says so, and the file is saved again (as
            // the person fixes it) a moment later, with what is then not the size it was.
            r#"if [ "$(wc -l < "$0")" -eq 2 ]; then echo "SyntaxError: nope" >&2; "#,
            r#"sleep 0.3; echo fixed > "$2"; exit 3; fi; "#,
            r#"exec node "$@""#
        )
        .to_owned(),
        log.to_string_lossy().into_owned(),
        writes_on_call().to_string_lossy().into_owned(),
        watched.to_string_lossy().into_owned(),
        "after".to_owned(),
    ];
    let watched_arg = watched.to_string_lossy();
    let run = run_dev(
        &["--then", "interrupt", "--on-view", "2"],
        &["--tool", "show", "--watch", &watched_arg],
        &words(&server),
    );
    assert!(run.status.success(), "{}", run.text());
    run.in_order(&[
        "program: dismiss id=",
        "host: closed id=* re=* reason=program",
        "program: hello id=",
        "program: open id=* tool=show without a bundle",
        // The server fell before the view ran: the program leaves, and the screen is the person's.
        "program: bye id=* reason=exit",
        "screen: alternate screen off",
        // And then it is asked for the view again, which is the change that fixes the server.
        "program: hello id=",
        "program: open id=* tool=show without a bundle",
        "view: started from resources/read",
        "host: the person presses ^C",
        "program: bye id=* reason=signal",
        "program: was killed by signal 2",
    ]);
    let off = run
        .output_has("\x1b[?1049l")
        .expect("the screen was given back");
    let said = run
        .output_has("ended (exit status 3) while its view was opening")
        .expect("it said that the server ended");
    let wrote = run
        .output_has("  SyntaxError: nope")
        .expect("it showed what the server wrote");
    let waits = run
        .output_has("Waiting for a change")
        .expect("it said that it waits");
    assert!(
        off < said && said < wrote && wrote < waits,
        "{off} {said} {wrote} {waits}"
    );
    // The broken server's words are not said again for the view that came after.
    assert_eq!(
        String::from_utf8_lossy(&run.output)
            .matches("SyntaxError: nope")
            .count(),
        2,
        "once in the command that was run, once as what the server wrote"
    );
    let pids = started(&log);
    assert_eq!(pids.len(), 3, "{pids:?}");
    for pid in pids {
        assert!(gone(pid), "the server {pid} outlived pencere dev");
    }
}

// ---- a filter in a pipeline -------------------------------------------------------

/// `text` in single quotes, for `sh -c`.
fn quoted(text: &str) -> String {
    format!("'{}'", text.replace('\'', r"'\''"))
}

/// `tablepen` (`examples/tablepen`) in the middle of a pipeline: CSV in from
/// `printf`, its stdout to `out`, and its status after it.
fn pipeline(dir: &Path, options: &[&str], args: &str) -> (Run, String, String) {
    let main = root().join("examples/tablepen/dist/main.js");
    assert!(
        main.is_file(),
        "{} is missing: build the TypeScript half first with `npx tsc -b` (make check does)",
        main.display()
    );
    let out = dir.join("out.csv");
    let status = dir.join("status");
    let script = format!(
        r#"printf 'name,size\nweb,3\ndb,40\n"cache, hot",5\n' | node {} {args} > {}; echo $? > {}"#,
        quoted(&main.to_string_lossy()),
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
        &["--call", r#"choose_rows={"rows":[2,0]}"#],
        "--select",
    );
    assert!(run.status.success(), "{}", run.text());
    run.in_order(&[
        "program: hello id=1",
        "host: sees the tty: canonical=false echo=false",
        "screen: alternate screen on",
        "program: open id=2 tool=show_table bundle of 3 files",
        r#"view: <- ui/notifications/tool-result "3 rows, 2 columns: name, size""#,
        r#"view: -> tools/call choose_rows {"rows":[2,0]}"#,
        // The program answers the call before it closes the view, so the view hears
        // that the rows were taken.
        "program: msg id=* response",
        "program: dismiss",
        "host: closed id=* reason=program",
        "screen: alternate screen off",
    ]);
    assert_eq!(
        run.count(r#"view: <- tool result "Chose 2 of 3 rows, 2 columns.""#),
        1,
        "{}",
        run.text()
    );
    // The terminal had the protocol; the pipe has the data and nothing else.
    assert_eq!(out, "name,size\nweb,3\n\"cache, hot\",5\n");
    assert_eq!(status, "0");
}

#[test]
fn a_filter_whose_view_closes_with_nothing_chosen_writes_nothing_and_fails() {
    let dir = fresh("filter-nothing");
    let (run, out, status) = pipeline(&dir, &["--then", "close"], "--select");
    assert!(run.status.success(), "{}", run.text());
    run.in_order(&[
        "screen: alternate screen on",
        "host: the person closes the view",
        "host: closed id=* reason=user",
        "screen: alternate screen off",
    ]);
    // `rows=$(… | tablepen --select) && mail …` stops here.
    assert_eq!(out, "");
    assert_eq!(status, "1");
}
