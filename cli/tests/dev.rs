//! `pencere dev` as a person runs it in a terminal, and leaves it.

use std::fs;
use std::io::{BufRead, BufReader};
use std::os::unix::process::CommandExt as _;
use std::process::{Command, Stdio};
use std::sync::mpsc::channel;
use std::time::{Duration, Instant};

#[test]
#[allow(unsafe_code)]
fn control_c_while_a_change_is_waited_for_leaves_with_nothing_wrong() {
    let dir = std::env::temp_dir().join(format!("pencere-dev-leave-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(dir.join("app")).expect("a directory");
    fs::write(dir.join("app/app.lua"), "print(\"done\")\n").expect("an app");
    // In a group of its own, as a terminal's foreground job is: ⌃C goes to the group.
    let mut dev = Command::new(env!("CARGO_BIN_EXE_pencere"))
        .arg("dev")
        .arg(dir.join("app"))
        .arg("--log")
        .arg(dir.join("log"))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .process_group(0)
        .spawn()
        .expect("pencere dev starts");
    let stderr = dev.stderr.take().expect("its standard error");
    let (tell, said) = channel();
    std::thread::spawn(move || {
        for line in BufReader::new(stderr).lines().map_while(Result::ok) {
            if tell.send(line).is_err() {
                return;
            }
        }
    });
    // The app prints and ends; `pencere dev` waits for a change.
    let until = Instant::now() + Duration::from_secs(20);
    loop {
        let left = until.saturating_duration_since(Instant::now());
        let line = said
            .recv_timeout(left)
            .expect("pencere dev says it waits for a change");
        if line.contains("waiting for a change") {
            break;
        }
    }
    let group = libc::pid_t::try_from(dev.id()).expect("a process id");
    // SAFETY: ⌃C, as the terminal sends it, to the group of the process this test started.
    unsafe {
        libc::kill(-group, libc::SIGINT);
    }
    let until = Instant::now() + Duration::from_secs(10);
    let status = loop {
        if let Some(status) = dev.try_wait().expect("pencere dev is waited for") {
            break status;
        }
        if Instant::now() > until {
            let _ = dev.kill();
            panic!("pencere dev did not leave on ⌃C");
        }
        std::thread::sleep(Duration::from_millis(20));
    };
    let _ = fs::remove_dir_all(&dir);
    assert_eq!(status.code(), Some(0), "{status:?}");
}
