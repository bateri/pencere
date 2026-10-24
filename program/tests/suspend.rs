//! ⌃Z: the program gives the terminal back, stops, and takes the terminal again when it is
//! continued.
//!
//! A program that stops stops the process, so this one is run as a process of its own: the test
//! starts itself again as the program (`suspend_child`), with a pty for its controlling
//! terminal, and plays the terminal and the job control of a shell on the other end of it.

// A test fails by panicking, and the helpers here are the test's.
#![allow(clippy::expect_used, clippy::panic)]

use std::os::unix::process::CommandExt;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use pencere::{Options, Outcome, View};
use serde_json::json;

mod common;

use common::{Pane, TOKEN, WITHIN, page, termios_of, web_views};

const MARKER: &str = "PENCERE_SUSPEND_CHILD";

/// The program: only when the test started itself as one.
#[test]
fn suspend_child() {
    if std::env::var_os(MARKER).is_none() {
        return;
    }
    // A job the shell has not yet made the foreground one is stopped by the tty when it changes
    // its modes: the program waits for its turn, as a program started from a shell is given it.
    let until = Instant::now() + WITHIN;
    // SAFETY: asks which process group has the terminal.
    while unsafe { libc::tcgetpgrp(0) != libc::getpgrp() } && Instant::now() < until {
        std::thread::sleep(Duration::from_millis(5));
    }
    let outcome =
        pencere::serve(Options::new().view(View::web(page())).program("suspend")).expect("served");
    // A view that closed because the person pressed ⌃C: the status a shell would report.
    assert!(matches!(outcome, Outcome::Closed(_)), "{outcome:?}");
}

/// Kills the shell if the test fails before it has ended.
struct Program(Child);

impl Drop for Program {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn wait_for(what: &str, mut condition: impl FnMut() -> bool) {
    let until = Instant::now() + WITHIN;
    while !condition() {
        assert!(Instant::now() < until, "{what}");
        std::thread::sleep(Duration::from_millis(10));
    }
}

const SHELL_MARKER: &str = "PENCERE_SUSPEND_SHELL";

/// A shell with job control, as far as the program can tell: it runs the program as a job of its
/// own, in a process group of its own that it makes the foreground one; when the job stops it
/// takes the terminal back and says so; when it is told to (`go`), it gives the terminal to the
/// job again and continues it. (A real shell is not used: the job control of a script differs
/// from shell to shell, and a process group whose parent is not in its session is an orphan,
/// which a stop signal does not stop.)
#[test]
fn suspend_shell() {
    if std::env::var_os(SHELL_MARKER).is_none() {
        return;
    }
    // SAFETY: a shell ignores the signals that a background job's tty calls would stop it with.
    unsafe {
        libc::signal(libc::SIGTTOU, libc::SIG_IGN);
        libc::signal(libc::SIGTTIN, libc::SIG_IGN);
        libc::signal(libc::SIGTSTP, libc::SIG_IGN);
    }
    let mut command = Command::new(std::env::current_exe().expect("this test's own file"));
    command
        .args([
            "--exact",
            "suspend_child",
            "--nocapture",
            "--test-threads=1",
        ])
        .env(MARKER, "1")
        .env_remove(SHELL_MARKER);
    // SAFETY: runs in the child between fork and exec: its own process group, and the signals it
    // is to be stopped by back to what they are for a program.
    unsafe {
        command.pre_exec(|| {
            libc::setpgid(0, 0);
            libc::signal(libc::SIGTTOU, libc::SIG_DFL);
            libc::signal(libc::SIGTTIN, libc::SIG_DFL);
            libc::signal(libc::SIGTSTP, libc::SIG_DFL);
            Ok(())
        });
    }
    // The job is waited for below with `waitpid`, which also tells when it stops.
    #[allow(clippy::zombie_processes)]
    let job = command.spawn().expect("the job starts");
    let pid = libc::pid_t::try_from(job.id()).expect("a pid");
    // SAFETY: gives the terminal to the job's process group.
    unsafe {
        libc::setpgid(pid, pid);
        libc::tcsetpgrp(0, pid);
    }
    loop {
        let mut status = 0;
        // SAFETY: waits for a child of ours, stopped ones included.
        unsafe { libc::waitpid(pid, &mut status, libc::WUNTRACED) };
        if libc::WIFSTOPPED(status) {
            // SAFETY: takes the terminal back for the shell.
            unsafe { libc::tcsetpgrp(0, libc::getpgrp()) };
            println!("PENCERE-STOPPED");
            let mut line = String::new();
            let _ = std::io::stdin().read_line(&mut line);
            // SAFETY: `fg`: the terminal to the job again, and the job continued.
            unsafe {
                libc::tcsetpgrp(0, pid);
                libc::kill(-pid, libc::SIGCONT);
            }
        } else {
            let code = if libc::WIFEXITED(status) {
                libc::WEXITSTATUS(status)
            } else {
                -1
            };
            // SAFETY: takes the terminal back for the shell.
            unsafe { libc::tcsetpgrp(0, libc::getpgrp()) };
            println!("PENCERE-EXIT-{code}");
            return;
        }
    }
}

#[test]
fn ctrl_z_gives_the_terminal_back_and_continuing_takes_it_again() {
    let mut pane = Pane::new();
    let slave = pane.slave;
    let mut command = Command::new(std::env::current_exe().expect("this test's own file"));
    command
        .args([
            "--exact",
            "suspend_shell",
            "--nocapture",
            "--test-threads=1",
        ])
        .env(SHELL_MARKER, "1")
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());
    // SAFETY: runs in the child between fork and exec, with calls that are safe there: the pty
    // becomes its standard streams and its controlling terminal.
    unsafe {
        command.pre_exec(move || {
            libc::setsid();
            libc::ioctl(slave, libc::TIOCSCTTY as _, 0);
            for fd in 0..3 {
                libc::dup2(slave, fd);
            }
            if slave > 2 {
                libc::close(slave);
            }
            Ok(())
        });
    }
    let shell = Program(command.spawn().expect("the shell starts"));

    // The program asks, is answered, and opens its view.
    let hello = pane.expect("hello");
    let modes = termios_of(pane.master);
    assert_eq!(modes.c_lflag & libc::ICANON, 0);
    pane.welcome(hello.id, TOKEN, &web_views());
    let open = pane.expect("open");
    pane.send("need", Some(open.id), &json!({ "missing": [] }));
    wait_for("the alternate screen", || {
        pane.read_some(Duration::from_millis(20));
        pane.text().contains("\x1b[?1049h")
    });

    // ⌃Z: the tty turns the character into SIGTSTP for the program, and the shell, which waits
    // for its job, finds it stopped.
    pane.write_raw(&[0x1a]);
    let until = Instant::now() + WITHIN;
    while !pane.text().contains("PENCERE-STOPPED") {
        assert!(
            Instant::now() < until,
            "the program did not stop: {:?}",
            pane.text()
        );
        pane.read_some(Duration::from_millis(20));
    }
    // It gave the screen back before the shell said so, and the modes: the shell that has the
    // terminal now finds it as it left it.
    let off = pane
        .text()
        .find("\x1b[?1049l")
        .expect("the screen was given back");
    assert!(off < pane.text().find("PENCERE-STOPPED").expect("stopped"));
    let modes = termios_of(pane.master);
    assert_ne!(modes.c_lflag & libc::ICANON, 0, "canonical");
    assert_ne!(modes.c_lflag & libc::ECHO, 0, "echo");

    // `fg`: the program continues, and takes the terminal again as it was.
    pane.write_raw(b"go\n");
    let before = pane.text().matches("\x1b[?1049h").count();
    wait_for("the screen to come back", || {
        pane.read_some(Duration::from_millis(20));
        pane.text().matches("\x1b[?1049h").count() > before
    });
    let modes = termios_of(pane.master);
    assert_eq!(modes.c_lflag & (libc::ICANON | libc::ECHO), 0, "raw again");
    assert_ne!(modes.c_lflag & libc::ISIG, 0, "signals on");

    // And it is the same program: ⌃C ends the view with a `bye`.
    pane.write_raw(&[0x03]);
    let bye = pane.expect("bye");
    pane.send("closed", Some(bye.id), &json!({ "reason": "program" }));
    wait_for("the program to end", || {
        pane.read_some(Duration::from_millis(20));
        pane.text().contains("PENCERE-EXIT-")
    });
    assert!(pane.text().contains("PENCERE-EXIT-0"), "{:?}", pane.text());
    drop(shell);
}
