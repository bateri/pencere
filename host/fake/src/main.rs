//! A stand-in terminal for the authors of programs and for tests.
//!
//! It runs a program on a pty and plays the host of the views that program
//! opens, with the `pencere` crate holding every rule of the conversation, so a
//! program can be tried, and the SDK tested, without a terminal that hosts pencere.
//! The page is a script, not a web view: what is tried is the conversation, and
//! it is written to stderr in the order it happens. Run it from a terminal and
//! the program's keys are yours: ^C ends the program the way it ends it
//! anywhere.
//!
//! The crate does no I/O, so this file is the terminal around it: the pty, the
//! poll loop, the person's own terminal, and the clock.

#![deny(unsafe_code)]

mod args;
mod host;
mod log;
mod pty;
mod screen;
mod store;
mod view;

use std::io::{self, IsTerminal};
use std::os::unix::process::ExitStatusExt;
use std::process::ExitCode;
use std::time::{Duration, Instant};

use args::{Args, Outcome, USAGE};
use host::Host;
use pty::{Output, Pty, Raw, Size};

/// How often the loop looks at everything, whether or not poll had news.
const TICK_MS: i32 = 50;

/// The most reads of the pane in one turn of the loop, and in the last one.
const READS_PER_TURN: usize = 16;
const DRAIN_READS: usize = 256;

/// The exit code of a run that had to be cut short, as `timeout(1)` has it.
const GAVE_UP: u8 = 124;

fn main() -> ExitCode {
    match args::parse(std::env::args().skip(1)) {
        Outcome::Help => {
            print!("{USAGE}");
            ExitCode::SUCCESS
        }
        Outcome::Usage(message) => {
            eprintln!("pencere-fake-host: {message}\n\n{USAGE}");
            ExitCode::from(64)
        }
        Outcome::Run(args) => match run(&args) {
            Ok(code) => ExitCode::from(code),
            Err(error) => {
                eprintln!("pencere-fake-host: {error}");
                ExitCode::from(70)
            }
        },
    }
}

fn describe(status: std::process::ExitStatus) -> String {
    match (status.code(), status.signal()) {
        (Some(code), _) => format!("exited with status {code}"),
        (None, Some(signal)) => format!("was killed by signal {signal}"),
        (None, None) => "ended".to_owned(),
    }
}

fn run(args: &Args) -> io::Result<u8> {
    let size = args
        .size
        .map(|(cols, rows)| Size { cols, rows })
        .or_else(Size::of_terminal)
        .unwrap_or(Size::DEFAULT);
    let mut host = Host::new(args, size)?;
    let mut pty = Pty::spawn(&args.program, size).map_err(|error| {
        io::Error::new(
            error.kind(),
            format!("cannot start {}: {error}", args.program.join(" ")),
        )
    })?;
    log::line(
        "host",
        format!(
            "started {} (pid {}) on a {}x{} pane",
            args.program.join(" "),
            pty.id(),
            size.cols,
            size.rows
        ),
    );

    // The person's keys go to the pane as they are, when there is a person.
    let mut keys = io::stdin().is_terminal();
    let _raw = if keys { Some(Raw::enter()?) } else { None };

    let deadline = Instant::now() + args.limit;
    let mut gave_up = false;
    let mut stalled = 0u32;
    let mut buffer = [0u8; 4096];
    loop {
        if host.take_kill() {
            pty.kill();
        }
        if host.take_hang_up() {
            pty.hang_up();
        }
        let now = Instant::now();
        if now >= deadline && !gave_up {
            gave_up = true;
            log::line("host", "the time limit is up: kills the program");
            pty.kill();
        }
        host.tick(now);

        // Into the pane, as much as the tty takes.
        if !host.pending().is_empty() {
            match pty.write(host.pending()) {
                Ok(taken) => {
                    stalled = if taken == 0 { stalled + 1 } else { 0 };
                    host.typed(taken);
                }
                // The program is gone: nobody reads what is left.
                Err(_) => host.typed(usize::MAX),
            }
        }

        // Woken when the tty can take more of what is waiting, so that a message
        // far larger than its input queue goes through as fast as the program
        // reads. A tty that said it could and then took nothing, twice running, is
        // not asked again until the next tick, so that a poll that misreports costs
        // a delay and not a busy loop.
        let writable = !host.pending().is_empty() && stalled < 2;
        pty::wait(&pty, writable, keys, TICK_MS);

        if keys {
            let mut typed = [0u8; 256];
            match pty::read_keys(&mut typed) {
                pty::Keys::Typed(count) => {
                    if let Some(bytes) = typed.get(..count) {
                        host.person_typed(bytes);
                    }
                }
                pty::Keys::Nothing => {}
                pty::Keys::Gone => {
                    // Nobody is there to end the program: it ends as it would
                    // with the terminal it ran in.
                    keys = false;
                    log::line("host", "the terminal hung up: kills the program");
                    pty.kill();
                }
            }
        }

        // From the pane, until it has nothing more or the program is gone, but
        // not for ever: a program that never stops writing must not keep the host
        // from writing back, from its clock and from the time limit.
        let mut ended = false;
        for _ in 0..READS_PER_TURN {
            match pty.read(&mut buffer)? {
                Output::Bytes(count) => {
                    if let Some(bytes) = buffer.get(..count) {
                        let modes = pty.modes();
                        host.program_output(bytes, modes);
                    }
                }
                Output::Nothing => break,
                Output::End => {
                    ended = true;
                    break;
                }
            }
        }
        // The pty may report the end of the program only by saying nothing.
        if !ended && pty.finished().is_some() {
            drain(&pty, &mut host, &mut buffer)?;
            ended = true;
        }
        if ended {
            break;
        }
    }

    // The pane has nothing more to say; the program may still be there (it can
    // close its tty and go on), and the time limit holds for it too.
    let status = loop {
        if let Some(status) = pty.finished() {
            break status;
        }
        if Instant::now() >= deadline && !gave_up {
            gave_up = true;
            log::line("host", "the time limit is up: kills the program");
            pty.kill();
        }
        std::thread::sleep(Duration::from_millis(10));
    };
    log::line("program", describe(status));
    host.program_ended();
    Ok(if gave_up { GAVE_UP } else { 0 })
}

/// Reads what is left of the program's output once it is known to be gone. A
/// program that has gone can have left a child holding the pty that goes on
/// writing, so this is bounded too.
fn drain(pty: &Pty, host: &mut Host, buffer: &mut [u8]) -> io::Result<()> {
    for _ in 0..DRAIN_READS {
        match pty.read(buffer)? {
            Output::Bytes(count) => {
                if let Some(bytes) = buffer.get(..count) {
                    host.program_output(bytes, pty.modes());
                }
            }
            Output::Nothing | Output::End => return Ok(()),
        }
    }
    Ok(())
}
