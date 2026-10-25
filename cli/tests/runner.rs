//! The runner that `cargo build` makes is one an app can be built from: its Mach-O header has the
//! room for the section `pencere build` adds, and the app made of it runs.

// A test fails by panicking, and the helpers here are the test's.
#![allow(clippy::expect_used, clippy::panic)]

use std::fs;
use std::process::Command;

// The code that reads the header is the program's own; it has nothing of the crate in it.
#[path = "../src/macho.rs"]
mod macho;

#[test]
fn the_runner_has_the_room_in_its_header_that_an_app_built_from_it_needs() {
    let runner = fs::read(env!("CARGO_BIN_EXE_pencere")).expect("the runner");
    if !macho::is_arm_64(&runner) {
        // Only a Mac with an Arm processor gets a load command; nothing is asked of other runners.
        return;
    }
    let room = macho::header_room(&runner).expect("a header that can be read");
    assert!(
        room >= macho::NEW_COMMAND,
        "the runner has {room} bytes free in its header, the app's section needs {} (build.rs asks the \
         linker for 0x4000)",
        macho::NEW_COMMAND
    );
}

#[test]
fn an_app_built_from_the_runner_runs() {
    let dir = std::env::temp_dir().join(format!("pencere-runner-test-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("app")).expect("a directory");
    fs::write(
        dir.join("app/app.lua"),
        "print(\"hello from the app\", #require(\"pencere\").args)\n",
    )
    .expect("an app");
    let built = Command::new(env!("CARGO_BIN_EXE_pencere"))
        .arg("build")
        .arg(dir.join("app"))
        .arg("-o")
        .arg(dir.join("hello"))
        .output()
        .expect("the runner builds");
    assert!(
        built.status.success(),
        "{}",
        String::from_utf8_lossy(&built.stderr)
    );
    let ran = Command::new(dir.join("hello"))
        .arg("one")
        .output()
        .expect("the app runs");
    let said = String::from_utf8_lossy(&ran.stdout).into_owned();
    let _ = fs::remove_dir_all(&dir);
    assert!(ran.status.success(), "{said}");
    assert_eq!(said, "hello from the app\t1\n");
}

/// Whether a process exists (a zombie that nobody has waited for yet still does, for a moment).
fn exists(pid: libc::pid_t) -> bool {
    // SAFETY: asks whether a process exists; no signal is sent.
    unsafe { libc::kill(pid, 0) == 0 }
}

fn wait_for(what: &str, mut done: impl FnMut() -> bool) {
    let until = std::time::Instant::now() + std::time::Duration::from_secs(10);
    while !done() {
        assert!(std::time::Instant::now() < until, "{what}");
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
}

#[test]
fn an_app_that_shows_nothing_and_is_ended_by_the_terminals_signal_takes_its_programs_with_it() {
    use std::os::unix::process::ExitStatusExt as _;
    let dir = std::env::temp_dir().join(format!("pencere-signal-test-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("app")).expect("a directory");
    // A program in the background of a shell, in the group the app puts the shell in.
    fs::write(
        dir.join("app/app.lua"),
        r#"local pencere = require("pencere")
pencere.process.spawn { argv = { "sh", "-c", "sleep 30 & echo $! > " .. pencere.args[1] .. "; wait" } }
pencere.time.sleep(30)
"#,
    )
    .expect("an app");
    let built = Command::new(env!("CARGO_BIN_EXE_pencere"))
        .arg("build")
        .arg(dir.join("app"))
        .arg("-o")
        .arg(dir.join("waiting"))
        .output()
        .expect("the runner builds");
    assert!(
        built.status.success(),
        "{}",
        String::from_utf8_lossy(&built.stderr)
    );
    let pids = dir.join("pids");
    let mut app = Command::new(dir.join("waiting"))
        .arg(&pids)
        .spawn()
        .expect("the app runs");
    wait_for("the program the app started wrote its number", || {
        fs::read_to_string(&pids).is_ok_and(|text| text.trim().parse::<libc::pid_t>().is_ok())
    });
    let sleeping: libc::pid_t = fs::read_to_string(&pids)
        .expect("a number")
        .trim()
        .parse()
        .expect("a number");
    assert!(exists(sleeping));
    // What the terminal sends to the app's group when ⌃C is pressed, which the program is not in.
    // SAFETY: a signal to the process this test started.
    unsafe {
        libc::kill(
            libc::pid_t::try_from(app.id()).expect("a pid"),
            libc::SIGINT,
        );
    }
    let mut status = None;
    wait_for("the app ended", || {
        status = app.try_wait().expect("a status");
        status.is_some()
    });
    assert_eq!(
        status.and_then(|status| status.signal()),
        Some(libc::SIGINT)
    );
    wait_for("the program the app started is gone", || !exists(sleeping));
    let _ = fs::remove_dir_all(&dir);
}

/// Builds the app in `dir/app` (whose `app.lua` is `source`) into `dir/<name>`.
fn built(dir: &std::path::Path, name: &str, source: &str) -> std::path::PathBuf {
    let _ = fs::remove_dir_all(dir);
    fs::create_dir_all(dir.join("app")).expect("a directory");
    fs::write(dir.join("app/app.lua"), source).expect("an app");
    let built = Command::new(env!("CARGO_BIN_EXE_pencere"))
        .arg("build")
        .arg(dir.join("app"))
        .arg("-o")
        .arg(dir.join(name))
        .output()
        .expect("the runner builds");
    assert!(
        built.status.success(),
        "{}",
        String::from_utf8_lossy(&built.stderr)
    );
    dir.join(name)
}

#[test]
fn an_app_reads_its_standard_input_as_it_arrives_while_its_other_work_goes_on() {
    use std::io::Write as _;
    use std::process::Stdio;
    let dir = std::env::temp_dir().join(format!("pencere-stdin-test-{}", std::process::id()));
    let app = built(
        &dir,
        "reader",
        r#"local pencere = require("pencere")
local seen, pieces, ticks = 0, 0, 0
local ticker = pencere.time.every(0.02, function() ticks += 1 end)
local reading = pencere.io.read_stdin(function(bytes)
	seen += #bytes
	pieces += 1
end, function()
	ticker:cancel()
end)
reading:wait()
pencere.io.write(string.format("%d %s %s\n", seen, pieces > 1 and "pieces" or "piece", ticks > 0 and "ticked" or "still"))
"#,
    );
    let mut reader = Command::new(&app)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("the app runs");
    let mut input = reader.stdin.take().expect("its input");
    // More than one piece, and a pause in the middle: the app's timer runs in it.
    let big = vec![b'x'; 150_000];
    input.write_all(&big).expect("written");
    std::thread::sleep(std::time::Duration::from_millis(150));
    input.write_all(b"\xff\x00end\n").expect("written");
    drop(input);
    let output = reader.wait_with_output().expect("it ends");
    let said = String::from_utf8_lossy(&output.stdout).into_owned();
    let _ = fs::remove_dir_all(&dir);
    assert!(output.status.success(), "{said}");
    assert_eq!(said, "150006 pieces ticked\n");
}

#[test]
fn stopping_the_reading_tells_what_writes_the_input_that_it_has_no_reader() {
    use std::io::Write as _;
    use std::process::Stdio;
    let dir = std::env::temp_dir().join(format!("pencere-stdin-stop-{}", std::process::id()));
    let app = built(
        &dir,
        "stopper",
        r#"local pencere = require("pencere")
local reading
reading = pencere.io.read_stdin(function(bytes)
	reading:stop()
end, function() pencere.io.write("ended\n") end)
reading:wait()
pencere.io.write("stopped " .. tostring(reading:active()) .. "\n")
"#,
    );
    let mut reader = Command::new(&app)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("the app runs");
    let mut input = reader.stdin.take().expect("its input");
    // The writer keeps writing until the pipe has no reader: it is told, and does not wait for ever.
    let block = vec![b'y'; 64 * 1024];
    let mut told = false;
    for _ in 0..2000 {
        if input.write_all(&block).is_err() {
            told = true;
            break;
        }
    }
    let output = reader.wait_with_output().expect("it ends");
    let said = String::from_utf8_lossy(&output.stdout).into_owned();
    let _ = fs::remove_dir_all(&dir);
    assert!(told, "the writer was never told the pipe has no reader");
    assert_eq!(said, "stopped false\n");
}

#[test]
fn an_app_can_tell_whether_its_streams_are_terminals() {
    use std::process::Stdio;
    let dir = std::env::temp_dir().join(format!("pencere-tty-test-{}", std::process::id()));
    let app = built(
        &dir,
        "asker",
        r#"local pencere = require("pencere")
local io = pencere.io
io.write(string.format("%s %s %s\n", tostring(io.is_terminal("stdin")), tostring(io.is_terminal("stdout")), tostring(io.is_terminal("stderr"))))
local ok, why = pcall(io.is_terminal, "stdlog")
io.write(tostring(ok) .. "\n")
"#,
    );
    let output = Command::new(&app)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .expect("the app runs");
    let said = String::from_utf8_lossy(&output.stdout).into_owned();
    let _ = fs::remove_dir_all(&dir);
    assert_eq!(said, "false false false\nfalse\n");
}
