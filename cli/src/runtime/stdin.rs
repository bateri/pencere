//! The app's standard input, read as it arrives.
//!
//! A filter in a pipeline gets its data on stdin, and its view (`serve{ filter = true }`) is on the
//! controlling terminal. The data comes at the pace of whatever writes it, so it is read by the loop
//! like the pipes of the processes the app started: the loop waits for it with everything else, gives
//! a piece to the app, and goes back to the app's other work. A read that waited in Lua would hold
//! the view.

use std::rc::{Rc, Weak};

use mlua::{Function, Lua, UserData, UserDataMethods, Value};

use super::core::Core;

/// How much of stdin one turn of the loop reads: what a pipe holds at most, so that a writer that
/// is faster than the app does not fill the memory with what the app has not yet looked at.
const PIECE: usize = 64 * 1024;

/// The reading of stdin that is going on: who is given what.
pub struct Stdin {
    pub on_data: Function,
    pub on_end: Option<Function>,
}

/// `io.read_stdin(on_data[, on_end])`: starts the reading. One reading at a time.
pub fn start(core: &Rc<Core>, on_data: Function, on_end: Option<Function>) -> mlua::Result<Handle> {
    if core.stdin.borrow().is_some() {
        return Err(super::fault(
            "io.read_stdin: standard input is already being read (it ends or is stopped first)",
        ));
    }
    *core.stdin.borrow_mut() = Some(Stdin { on_data, on_end });
    Ok(Handle { core: core.weak() })
}

/// The reading, for the app to stop.
pub struct Handle {
    core: Weak<Core>,
}

impl UserData for Handle {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        // reading:stop(): no more is read and `on_end` is not called. Standard input is closed, as
        // far as what writes it can tell (it is told the pipe has no reader), and descriptor 0
        // stays taken, so that nothing the app opens is later mistaken for it. Whether it was still
        // being read.
        methods.add_method("stop", |_, this, ()| {
            let Some(core) = this.core.upgrade() else {
                return Ok(false);
            };
            let was = core.stdin.borrow_mut().take().is_some();
            if was {
                close();
            }
            Ok(was)
        });
        methods.add_method("active", |_, this, ()| {
            Ok(this
                .core
                .upgrade()
                .is_some_and(|core| core.stdin.borrow().is_some()))
        });
    }
}

/// Descriptor 0 is made the empty device, which lets go of the pipe it was.
fn close() {
    // SAFETY: opens a path we name and makes the descriptor it gives descriptor 0; nothing else
    // holds the new one.
    unsafe {
        let null = libc::open(c"/dev/null".as_ptr(), libc::O_RDONLY | libc::O_CLOEXEC);
        if null >= 0 {
            libc::dup2(null, libc::STDIN_FILENO);
            if null != libc::STDIN_FILENO {
                libc::close(null);
            }
        }
    }
}

/// Whether the descriptor `poll` is to watch for standard input.
pub fn watched(core: &Core) -> Option<libc::c_int> {
    core.stdin.borrow().as_ref().map(|_| libc::STDIN_FILENO)
}

/// Reads what is waiting on standard input (the loop found it readable), and gives it to the app:
/// a piece to `on_data`, or the end to `on_end`.
pub fn read(core: &Core) {
    let (on_data, on_end) = match core.stdin.borrow().as_ref() {
        Some(reading) => (reading.on_data.clone(), reading.on_end.clone()),
        None => return,
    };
    let mut chunk = vec![0u8; PIECE];
    let (piece, ended) = loop {
        // SAFETY: reads into a buffer of ours from a descriptor that is the process's own; the loop
        // found the descriptor readable, so this does not wait.
        let got = unsafe { libc::read(libc::STDIN_FILENO, chunk.as_mut_ptr().cast(), chunk.len()) };
        if got > 0 {
            chunk.truncate(got as usize);
            break (Some(chunk), false);
        }
        if got == 0 {
            break (None, true);
        }
        let error = std::io::Error::last_os_error();
        match error.kind() {
            std::io::ErrorKind::Interrupted => continue,
            std::io::ErrorKind::WouldBlock => break (None, false),
            // An input that cannot be read is an input that has ended.
            _ => break (None, true),
        }
    };
    if let Some(piece) = piece {
        let Some(lua) = piece_value(core.lua(), &piece) else {
            return;
        };
        core.callback(on_data, vec![lua]);
        if core.exit().is_some() {
            return;
        }
    }
    if ended {
        // Taken out before `on_end` runs, so that it can start a reading of its own.
        let was = core.stdin.borrow_mut().take().is_some();
        if was {
            if let Some(on_end) = on_end {
                core.callback(on_end, Vec::new());
            }
        }
    }
}

fn piece_value(lua: &Lua, piece: &[u8]) -> Option<Value> {
    lua.create_string(piece).ok().map(Value::String)
}
