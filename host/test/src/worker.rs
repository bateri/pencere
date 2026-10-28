//! A worker: a process that runs the test files it is given, one at a time, each in a Lua state of
//! its own, and tells what happens of them to the one that started it.
//!
//! The coordinator writes a line of JSON for each file (`Job`) to its standard input and reads a line
//! for each thing that happens from a descriptor of its own: the worker's standard output is given up
//! to its standard error, so that what a program of the test writes to the terminal can never be
//! taken for one of the lines.

use std::io::{BufRead as _, Write as _};
use std::os::fd::FromRawFd as _;
use std::process::ExitCode;
use std::rc::Rc;

use serde_json::{Value, json};

use crate::file::{Ended, Job, run};

pub fn main() -> ExitCode {
    // The protocol's descriptor is a copy of the standard output; the standard output itself is
    // the standard error from here on.
    // SAFETY: duplicates descriptors of this process and redirects one of them; the copy is owned
    // by the `File` made of it.
    let channel = unsafe {
        let copy = libc::dup(libc::STDOUT_FILENO);
        if copy < 0 || libc::dup2(libc::STDERR_FILENO, libc::STDOUT_FILENO) < 0 {
            eprintln!("pencere-test: the worker's channel cannot be set up");
            return ExitCode::from(70);
        }
        // Not inherited: a program the app under test starts must not hold the channel open past the
        // worker's end, or the coordinator would never see it end.
        libc::fcntl(copy, libc::F_SETFD, libc::FD_CLOEXEC);
        std::fs::File::from_raw_fd(copy)
    };
    let channel = Rc::new(std::cell::RefCell::new(channel));
    let emit: Rc<dyn Fn(Value)> = {
        let channel = Rc::clone(&channel);
        Rc::new(move |event| {
            let mut line = event.to_string();
            line.push('\n');
            let _ = channel.borrow_mut().write_all(line.as_bytes());
        })
    };
    for line in std::io::stdin().lock().lines() {
        let Ok(line) = line else {
            break;
        };
        if line.trim().is_empty() {
            continue;
        }
        let job = serde_json::from_str::<Value>(&line)
            .ok()
            .as_ref()
            .and_then(Job::from_json);
        let Some(job) = job else {
            emit(
                json!({ "ev": "file_error", "message": "the worker was given a job it cannot read" }),
            );
            emit(json!({ "ev": "done" }));
            continue;
        };
        match run(job, Rc::clone(&emit)) {
            Ended::Ran => {}
            Ended::Failed(why) => emit(json!({ "ev": "file_error", "message": why })),
        }
        emit(json!({ "ev": "done" }));
    }
    ExitCode::SUCCESS
}
