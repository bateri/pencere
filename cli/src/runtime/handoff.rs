//! `pencere.handoff`: the app gives the terminal to a program, and takes it back.
//!
//! The library does it ([`pencere::ViewControl::hand_off`]): the view is closed and the terminal
//! given back, the program runs with the terminal as its own and in the foreground, as a shell runs a
//! command, and when it ends the same `serve` shows its view again. An editor that opens the person's
//! shell in its pane works this way: `exit` brings the editor back. With no view up the program runs
//! all the same, with the terminal the app is not using.
//!
//! The loop waits while the program runs: the terminal is the program's, and so is the app's
//! attention. Timers that fall due run when it is back, and a process the app started goes on and is
//! read then.

use std::collections::HashMap;
use std::os::unix::process::ExitStatusExt as _;
use std::process::{Command, Stdio};
use std::rc::Rc;

use mlua::Table;
use pencere::HandOffResult;

use super::core::Core;

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

    let ended = match core.control.hand_off(command).wait() {
        // A view that did not come back is the app's `serve`'s to tell, as it ends.
        HandOffResult::Returned(ended)
        | HandOffResult::NoView {
            status: Some(ended),
            ..
        } => ended,
        HandOffResult::NoView { status: None, .. } => {
            return Err(super::fault(
                "handoff: the terminal did not close the view, so the program was not run",
            ));
        }
        HandOffResult::Unsupported => {
            return Err(super::fault(
                "handoff: this terminal cannot close the view for the program (it does not list dismiss), so it cannot be given the terminal",
            ));
        }
        HandOffResult::Busy => {
            return Err(super::fault(
                "handoff: the terminal is given to another program already",
            ));
        }
        _ => {
            return Err(super::fault(
                "handoff: the view cannot be closed now (the terminal has not opened it yet, or it is already being closed)",
            ));
        }
    };
    let status =
        ended.map_err(|why| super::fault(format!("handoff: {program}: {}", super::why(&why))))?;
    let table = core.lua().create_table()?;
    table.set("code", status.code())?;
    table.set("signal", status.signal())?;
    Ok(table)
}
