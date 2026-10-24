//! `pencere.handoff`: the app gives the terminal to a program, and takes it back.
//!
//! The view is closed (the terminal is given back first, as for any view that ends: the screen,
//! the modes), the program is run with the terminal as its own and in the foreground, as a shell
//! runs a command, and when it ends the app takes the foreground, and shows its view again — the
//! same `serve`, with the files as they are then. An editor that opens the person's shell in
//! its pane works this way: `exit` brings the editor back.
//!
//! The loop waits while the program runs: the terminal is the program's, and so is the app's
//! attention. Timers that fall due run when it is back, and a process the app started goes on and is
//! read then.

use std::collections::HashMap;
use std::os::unix::process::CommandExt as _;
use std::process::{Command, Stdio};
use std::rc::Rc;
use std::time::Duration;

use mlua::Table;
use pencere::CloseResult;

use super::core::Core;
use super::events::Event;

/// How long the view's thread is waited for after the view was closed: it gives the terminal back
/// before the program is run.
const GIVE_BACK: Duration = Duration::from_secs(5);

/// `handoff{ argv = {...}, cwd =, env = }` (or the argv itself): runs the program in the
/// terminal. What it gives is how the program ended: `{ code =, signal = }`.
pub fn handoff(core: &Rc<Core>, spec: Table) -> mlua::Result<Table> {
    let argv: Vec<String> = match spec.get::<Option<Vec<String>>>("argv")? {
        Some(argv) => argv,
        None => spec
            .sequence_values::<String>()
            .collect::<mlua::Result<_>>()?,
    };
    let Some((program, rest)) = argv.split_first() else {
        return Err(super::fault(
            "handoff needs `argv`, a list: the program and then its arguments",
        ));
    };
    let mut command = Command::new(program);
    command
        .args(rest)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());
    if let Some(cwd) = spec.get::<Option<String>>("cwd")? {
        command.current_dir(cwd);
    }
    if let Some(env) = spec.get::<Option<HashMap<String, String>>>("env")? {
        command.envs(env);
    }

    // The view goes, and the terminal with it.
    let showing = core.serve.borrow().is_some();
    if showing {
        match core.control.close() {
            CloseResult::Closed => {}
            CloseResult::Unsupported => {
                return Err(super::fault(
                    "handoff: this terminal cannot close the view for the program (it does not list dismiss), so it cannot be given the terminal",
                ));
            }
            _ => {
                return Err(super::fault(
                    "handoff: the view cannot be closed now (the terminal has not opened it yet, or it is already being closed)",
                ));
            }
        }
        let thread = core
            .serve
            .borrow_mut()
            .as_mut()
            .and_then(|run| run.thread.take());
        if let Some(thread) = thread {
            let until = std::time::Instant::now() + GIVE_BACK;
            while !thread.is_finished() && std::time::Instant::now() < until {
                std::thread::sleep(Duration::from_millis(5));
            }
            if !thread.is_finished() {
                return Err(super::fault(
                    "handoff: the view did not give the terminal back",
                ));
            }
            let _ = thread.join();
        }
        // The end of that `serve` is not the end of the app's: it is not told. Whatever else the
        // view had left for the loop is.
        core.drain(|event| !matches!(event, Event::Served(_)));
    }

    let ended = run_in_foreground(command);

    if showing {
        super::serve::serve_again(core)?;
    }
    let (code, signal) =
        ended.map_err(|why| super::fault(format!("handoff: {program}: {}", super::why(&why))))?;
    let table = core.lua().create_table()?;
    table.set("code", code)?;
    table.set("signal", signal)?;
    Ok(table)
}

/// Runs `command` as the foreground job of the terminal, and takes the terminal back when it ends:
/// its code, or the signal that ended it.
fn run_in_foreground(mut command: Command) -> std::io::Result<(Option<i32>, Option<i32>)> {
    // The controlling terminal, by its own name: standard input may be a pipe.
    // SAFETY: opens a path of ours.
    let tty = unsafe { libc::open(c"/dev/tty".as_ptr(), libc::O_RDWR | libc::O_CLOEXEC) };
    if tty >= 0 {
        // SAFETY: runs in the child between fork and exec, where only calls that are safe there
        // are made: it makes its own process group and takes the foreground of the terminal for
        // it, with the signal that a background job gets for that ignored for the moment.
        unsafe {
            command.pre_exec(move || {
                libc::setpgid(0, 0);
                let previous = libc::signal(libc::SIGTTOU, libc::SIG_IGN);
                libc::tcsetpgrp(tty, libc::getpid());
                libc::signal(libc::SIGTTOU, previous);
                Ok(())
            });
        }
    }
    let spawned = command.spawn();
    let result = match spawned {
        Ok(child) => {
            let pid = child.id() as libc::pid_t;
            let waited = wait(pid);
            // Reaped by the wait above; dropping the handle does not wait again.
            drop(child);
            waited
        }
        Err(why) => Err(why),
    };
    if tty >= 0 {
        // SAFETY: gives the foreground back to the app's group, ignoring for the moment the signal
        // a job that is not in the foreground gets for it; closes the descriptor opened above.
        unsafe {
            let previous = libc::signal(libc::SIGTTOU, libc::SIG_IGN);
            libc::tcsetpgrp(tty, libc::getpgrp());
            libc::signal(libc::SIGTTOU, previous);
            libc::close(tty);
        }
    }
    result
}

/// Waits for the process to end; one that is stopped (⌃Z reaches a program that is not a shell) is
/// continued, since the app has nothing else to do while the terminal is the program's.
fn wait(pid: libc::pid_t) -> std::io::Result<(Option<i32>, Option<i32>)> {
    loop {
        let mut status = 0;
        // SAFETY: waits for a child of ours.
        let got = unsafe { libc::waitpid(pid, &raw mut status, libc::WUNTRACED) };
        if got < 0 {
            let why = std::io::Error::last_os_error();
            if why.kind() == std::io::ErrorKind::Interrupted {
                continue;
            }
            return Err(why);
        }
        // The layout of the status is the same on macOS and Linux: the low seven bits say what
        // happened (0 it exited, 0x7f it stopped, else the signal that ended it), the next eight
        // the code of an exit.
        let low = status & 0x7f;
        if status & 0xff == 0x7f {
            // SAFETY: a signal to a child of ours.
            unsafe { libc::kill(pid, libc::SIGCONT) };
            continue;
        }
        if low == 0 {
            return Ok((Some((status >> 8) & 0xff), None));
        }
        return Ok((None, Some(low)));
    }
}
