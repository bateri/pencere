//! A program that moves files with its view (specification §8.8 to §8.10): it attaches a file for
//! the view to draw, receives a file the person gave the view, and writes a file to the place the
//! person chose in the Save panel. The view passes it the handles in its calls.
//!
//! ```text
//! files [--attach PATH] [--into DIR] [--source PATH]
//! ```
//!
//! The calls of its view: `attach` (the file of `--attach`; the answer is the name the view draws it
//! by), `detach {name}`, `reopen` (the view is shown again, once, and what it had attached is no
//! longer),
//! `receive {handle, name}` (into `--into`), `send {handle}` (the file of `--source`) and `gap
//! {handle}` (a write that leaves a gap, which the terminal refuses). The stand-in host plays the
//! person and the view: `pencere-fake-host --drop PATH --call 'receive={"handle":"$1","name":"a"}'
//! -- files --into DIR`.

use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, Ordering};

use pencere::{Bundle, FileError, Kind, Options, View, ViewControl, ViewError, safe_name};
use serde_json::{Value, json};

/// The page: the stand-in host plays it as a script, so what it says is never run.
const SCRIPT: &[u8] = b"return { render = function() end }";

/// The version of a Lua page that has files beside its bundle.
const VERSION: &str = "2.1";

/// The view has been shown again once.
static REOPENED: AtomicBool = AtomicBool::new(false);

fn failed(why: &FileError) -> ViewError {
    let code = match why {
        FileError::Unsupported => "unsupported".to_owned(),
        FileError::Refused(code) => code.clone(),
        FileError::Ended => "ended".to_owned(),
        _ => "io".to_owned(),
    };
    ViewError::with_message(code, why.to_string())
}

fn text(params: &Value, key: &str) -> Result<String, ViewError> {
    params[key]
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| ViewError::with_message("bad-params", format!("no {key}")))
}

fn main() -> ExitCode {
    let mut attach: Option<PathBuf> = None;
    let mut into = PathBuf::from(".");
    let mut source: Option<PathBuf> = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        let value = args.next().map(PathBuf::from);
        match (arg.as_str(), value) {
            ("--attach", Some(path)) => attach = Some(path),
            ("--into", Some(path)) => into = path,
            ("--source", Some(path)) => source = Some(path),
            _ => {
                eprintln!("usage: files [--attach PATH] [--into DIR] [--source PATH]");
                return ExitCode::from(64);
            }
        }
    }
    let bundle = match Bundle::single("files.lua", SCRIPT) {
        Ok(bundle) => bundle,
        Err(why) => {
            eprintln!("files: {why}");
            return ExitCode::from(66);
        }
    };
    let control = ViewControl::new();
    let (c1, c2, c3, c4, c5, c6) = (
        control.clone(),
        control.clone(),
        control.clone(),
        control.clone(),
        control.clone(),
        control.clone(),
    );
    let options = Options::new()
        .view(View::new(Kind::Lua, &[VERSION], bundle))
        .program("files")
        .control(control)
        .call("attach", move |_params, _call| {
            let path = attach
                .as_deref()
                .ok_or_else(|| ViewError::with_message("bad-params", "no --attach"))?;
            let name = c1.attach_file(path).map_err(|why| failed(&why))?;
            Ok(json!({ "name": name }))
        })
        .call("detach", move |params, _call| {
            c2.detach(&text(&params, "name")?);
            Ok(json!({}))
        })
        .call("reopen", move |_params, _call| {
            // Once: the view that comes back asks again, and is not closed again.
            if !REOPENED.swap(true, Ordering::SeqCst) {
                let control = c3.clone();
                std::thread::spawn(move || {
                    let _ = control.reopen(None);
                });
            }
            Ok(json!({}))
        })
        .call("receive", move |params, _call| {
            let handle = text(&params, "handle")?;
            let to = into.join(safe_name(&text(&params, "name")?));
            let bytes = c4
                .receive(&handle, &to, |_| true)
                .map_err(|why| failed(&why))?;
            Ok(json!({ "bytes": bytes }))
        })
        .call("send", move |params, _call| {
            let from = source
                .as_deref()
                .ok_or_else(|| ViewError::with_message("bad-params", "no --source"))?;
            let bytes = c5
                .send(&text(&params, "handle")?, from, |_| {})
                .map_err(|why| failed(&why))?;
            Ok(json!({ "bytes": bytes }))
        })
        .call("gap", move |params, _call| {
            let handle = text(&params, "handle")?;
            c6.write(&handle, 5, b"late".to_vec(), false)
                .and_then(|()| c6.write(&handle, 9, b"!".to_vec(), true))
                .map_err(|why| failed(&why))?;
            Ok(json!({}))
        });
    match pencere::serve(options) {
        Ok(outcome) => ExitCode::from(outcome.shell_status()),
        Err(why) => {
            eprintln!("files: {why}");
            ExitCode::from(70)
        }
    }
}
