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
