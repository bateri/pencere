//! The files beside the view's bundle, for an app (specification §8.8 to §8.10): what it attaches to
//! its view for the view to draw, a file the person gave the view, received by its handle, and a
//! file written to the place the person chose in the Save panel. The library's helpers wait for the
//! terminal, so each runs on a thread of its own and tells the loop how far it has got and how it
//! ended; the loop never waits on one.

use std::collections::HashMap;
use std::path::Path;
use std::rc::Rc;
use std::sync::mpsc::{SyncSender, sync_channel};

use mlua::{Function, Lua, Table, Value};
use pencere::{FileError, ViewControl, safe_name};

use super::core::Core;
use super::events::{Event, Events};

/// A helper that runs: what to tell how far it got, and what to tell how it ended.
pub struct Transfer {
    progress: Option<Function>,
    done: Function,
}

/// The transfers that run, by number.
pub type Transfers = HashMap<u64, Transfer>;

/// How a helper ended, for the loop: what it came to, or the code and the words of its failure.
pub type Ended = Result<serde_json::Value, (String, String)>;

fn ended(result: Result<serde_json::Value, FileError>) -> Ended {
    result.map_err(|why| {
        let code = match &why {
            FileError::Unsupported => "unsupported".to_owned(),
            FileError::Refused(code) => code.clone(),
            FileError::Ended => "ended".to_owned(),
            _ => "io".to_owned(),
        };
        (code, why.to_string())
    })
}

/// Starts `work` on a thread of its own as transfer `id`, telling the loop how it ends.
fn start(
    core: &Core,
    progress: Option<Function>,
    done: Function,
    work: impl FnOnce(&ViewControl, &dyn Fn(u64) -> bool) -> Result<serde_json::Value, FileError>
    + Send
    + 'static,
) -> mlua::Result<()> {
    let id = core.id();
    let told = progress.is_some();
    core.transfers
        .borrow_mut()
        .insert(id, Transfer { progress, done });
    let control = core.control.clone();
    let events: Events = core.events.clone();
    let started = std::thread::Builder::new()
        .name("pencere-file".into())
        .spawn(move || {
            let loop_events = events.clone();
            // The app's `progress` answers before the next piece: `false` stops at this one. A
            // loop that is gone (the app is ending) stops it too.
            let progress = move |bytes: u64| {
                if !told {
                    return true;
                }
                let (answer, answered) = sync_channel(1);
                loop_events.send(Event::FileProgress { id, bytes, answer });
                answered.recv().unwrap_or(false)
            };
            let result = ended(work(&control, &progress));
            events.send(Event::FileDone { id, result });
        });
    if let Err(why) = started {
        core.transfers.borrow_mut().remove(&id);
        return Err(super::fault(format!("a file cannot be moved: {why}")));
    }
    Ok(())
}

/// The functions of `files` that the prelude wraps (it waits for them without holding the loop).
pub fn natives(lua: &Lua, core: &Rc<Core>, native: &Table) -> mlua::Result<()> {
    let hold = Rc::clone(core);
    native.set(
        "file_attach",
        lua.create_function(
            move |_, (what, value, done): (String, mlua::String, Function)| {
                let bytes = value.as_bytes().to_vec();
                let path = hold.world.path(&String::from_utf8_lossy(&bytes));
                start(&hold, None, done, move |control, _| {
                    let name = if what == "path" {
                        control.attach_file(&path)?
                    } else {
                        control.attach(bytes)?
                    };
                    Ok(serde_json::Value::String(name))
                })
            },
        )?,
    )?;
    let hold = Rc::clone(core);
    native.set(
        "file_detach",
        lua.create_function(move |_, name: String| {
            hold.control.detach(&name);
            Ok(())
        })?,
    )?;
    let hold = Rc::clone(core);
    native.set(
        "file_receive",
        lua.create_function(
            move |_,
                  (handle, to, progress, path, size, done): (
                String,
                String,
                Option<Function>,
                Option<String>,
                Option<u64>,
                Function,
            )| {
                let to = hold.world.path(&to);
                start(&hold, progress, done, move |control, progress| {
                    // The path the terminal handed the page, which the page passed on: the file is
                    // the person's, not one of the app's, so it is never joined to the app's directory.
                    let bytes = match path.zip(size) {
                        Some((path, size)) => {
                            control.receive_from(&handle, Path::new(&path), size, &to, progress)?
                        }
                        None => control.receive(&handle, &to, progress)?,
                    };
                    Ok(serde_json::Value::from(bytes))
                })
            },
        )?,
    )?;
    let hold = Rc::clone(core);
    native.set(
        "file_send",
        lua.create_function(
            move |_, (handle, from, progress, done): (String, String, Option<Function>, Function)| {
                let from = hold.world.path(&from);
                // A save cannot be stopped (the protocol has no word for it): `progress` is told.
                start(&hold, progress, done, move |control, progress| {
                    let bytes = control.send(&handle, &from, |sent| {
                        progress(sent);
                    })?;
                    Ok(serde_json::Value::from(bytes))
                })
            },
        )?,
    )?;
    native.set(
        "file_safe_name",
        lua.create_function(|_, name: String| Ok(safe_name(&name)))?,
    )?;
    Ok(())
}

/// A transfer got `bytes` far: its `progress` is told, and the helper waits for its answer: `false`
/// (or an error in it) gives a receiving up.
pub fn on_progress(core: &Core, id: u64, bytes: u64, answer: &SyncSender<bool>) {
    let progress = core
        .transfers
        .borrow()
        .get(&id)
        .and_then(|transfer| transfer.progress.clone());
    let go_on = match progress.map(|progress| progress.call::<Value>(bytes)) {
        None => false,
        Some(Ok(Value::Boolean(false))) => false,
        Some(Ok(_)) => true,
        Some(Err(why)) => {
            core.log.error(&format!("pencere: {why}"));
            false
        }
    };
    let _ = answer.send(go_on);
}

/// A transfer ended: what waits for it is told what it came to, or `nil` and `{ code =, message = }`.
pub fn on_done(core: &Core, lua: &Lua, id: u64, result: Ended) {
    let Some(transfer) = core.transfers.borrow_mut().remove(&id) else {
        return;
    };
    let args = match result {
        Ok(value) => super::convert::from_json(lua, &value).map(|value| vec![value]),
        Err((code, message)) => (|| {
            let failure = lua.create_table()?;
            failure.set("code", code)?;
            failure.set("message", message)?;
            Ok(vec![Value::Nil, Value::Table(failure)])
        })(),
    };
    match args {
        Ok(args) => core.callback(transfer.done, args),
        Err(why) => core.log.error(&format!("pencere: {why}")),
    }
}
