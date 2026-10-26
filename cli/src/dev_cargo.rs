//! `pencere dev` for a program written in Rust: a Cargo package laid out as `pencere new --lang rust`
//! lays it out (`src/program/`, `src/view/`, `src/assets/`).
//!
//! The program is built with Cargo and run in this pane, with `PENCERE_DEV_APP` telling the
//! `pencere` library where the project's files are: the library reads the view from there and shows
//! it again, without ending the program, when the page or a picture changes. A change to anything
//! else (the program's sources, the manifest, the build script) builds the program and runs it
//! again. The program's standard error goes to the log, because the terminal is under the view;
//! Cargo's messages, which come while there is no view, stay in the pane.

use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitCode, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use crate::dev::{Stamps, append, changed};

/// How often the files are looked at.
const EVERY: Duration = Duration::from_millis(250);

/// The directories of a project that are not its sources: what Cargo, pencere and git keep there.
const NOT_SOURCES: [&str; 5] = ["target", "dist", ".pencere", ".git", "node_modules"];

/// Whether `root` is a project of this kind: a Cargo package (and not a Lua app).
pub fn is_project(root: &Path) -> bool {
    root.join("Cargo.toml").is_file()
}

/// Whether a change to the file at `path` (from the project's root) is the view's, which the program
/// shows again itself, rather than the program's, which is built and run again.
pub fn is_view(path: &str) -> bool {
    path.starts_with("src/view/") || path.starts_with("src/assets/")
}

/// The files of the project, outside what is not its sources, and when each last changed.
fn stamps(root: &Path) -> Stamps {
    let mut stamps = Stamps::new();
    let mut pending = vec![PathBuf::new()];
    while let Some(relative) = pending.pop() {
        let Ok(entries) = std::fs::read_dir(root.join(&relative)) else {
            continue;
        };
        for entry in entries.flatten() {
            let name = entry.file_name();
            let Some(name) = name.to_str() else { continue };
            if relative.as_os_str().is_empty() && NOT_SOURCES.contains(&name) {
                continue;
            }
            let path = relative.join(name);
            let Ok(meta) = entry.metadata() else { continue };
            if meta.is_dir() {
                pending.push(path);
            } else if let Some(text) = path.to_str() {
                stamps.insert(
                    text.replace(std::path::MAIN_SEPARATOR, "/"),
                    (
                        meta.modified().unwrap_or(SystemTime::UNIX_EPOCH),
                        meta.len(),
                    ),
                );
            }
        }
    }
    stamps
}

/// Looks at the project until `stop`, and tells which files change.
fn watch(root: PathBuf, tell: Sender<String>, stop: Arc<AtomicBool>) {
    let mut before = stamps(&root);
    while !stop.load(Ordering::SeqCst) {
        std::thread::sleep(EVERY);
        let after = stamps(&root);
        for path in changed(&before, &after) {
            if tell.send(path).is_err() {
                return;
            }
        }
        before = after;
    }
}

/// Waits for a change to the program (the view's changes, which no program runs to show, count
/// too: the person may be fixing the page of a program that did not build). `None` when the watch
/// has ended.
fn wait_for_change(changes: &Receiver<String>) -> Option<String> {
    let first = changes.recv().ok()?;
    std::thread::sleep(Duration::from_millis(120));
    while changes.try_recv().is_ok() {}
    Some(first)
}

/// Builds the package with Cargo (`cargo build` and `extra`), its messages on standard error, and
/// returns the program it made: the binary named `name`, else the only one, else the first.
pub fn build(root: &Path, name: &str, extra: &[&str]) -> Result<PathBuf, String> {
    let mut cargo = Command::new(std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into()))
        .args(["build", "--message-format=json-render-diagnostics"])
        .args(extra)
        .current_dir(root)
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|why| format!("cargo could not be run: {why}"))?;
    let mut binaries: Vec<(String, PathBuf)> = Vec::new();
    if let Some(out) = cargo.stdout.take() {
        for line in BufReader::new(out).lines().map_while(Result::ok) {
            let Ok(message) = serde_json::from_str::<serde_json::Value>(&line) else {
                continue;
            };
            let field = |path: &str| message.pointer(path);
            if field("/reason").and_then(serde_json::Value::as_str) != Some("compiler-artifact") {
                continue;
            }
            let is_bin = field("/target/kind")
                .and_then(serde_json::Value::as_array)
                .is_some_and(|kinds| kinds.iter().any(|kind| kind == "bin"));
            if let (true, Some(executable), Some(target)) = (
                is_bin,
                field("/executable").and_then(serde_json::Value::as_str),
                field("/target/name").and_then(serde_json::Value::as_str),
            ) {
                binaries.push((target.to_owned(), PathBuf::from(executable)));
            }
        }
    }
    let status = cargo
        .wait()
        .map_err(|why| format!("cargo could not be waited for: {why}"))?;
    if !status.success() {
        return Err("the program did not build".to_owned());
    }
    let named = binaries.iter().position(|(target, _)| target == name);
    let chosen = named.or(if binaries.is_empty() { None } else { Some(0) });
    chosen
        .and_then(|at| binaries.get(at))
        .map(|(_, path)| path.clone())
        .ok_or_else(|| "the package has no program (a [[bin]] target) to run".to_owned())
}

/// How a run of the program ended.
enum Ran {
    /// A file of the program changed and it was stopped, to be built and run again.
    Changed,
    /// It ended by itself, with this status (128 and the signal for one that a signal ended).
    Done(i32),
}

/// What is done with ⌃C while the program runs: nothing, in this process. The terminal sends it to
/// the program too, which ends its view and exits; `pencere dev` then ends with it. A handler, not
/// `SIG_IGN`, so that the program, which is started with the handlers reset, takes ⌃C as it would.
#[allow(unsafe_code)]
fn hold_interrupts(hold: bool) {
    extern "C" fn nothing(_signal: libc::c_int) {}
    let handler: libc::sighandler_t = if hold {
        nothing as extern "C" fn(libc::c_int) as libc::sighandler_t
    } else {
        libc::SIG_DFL
    };
    // SAFETY: setting the disposition of SIGINT and SIGQUIT to a function that does nothing (which
    // is safe in a signal handler) or to the default one.
    unsafe {
        libc::signal(libc::SIGINT, handler);
        libc::signal(libc::SIGQUIT, handler);
    }
}

/// Runs the program until it ends, or until a file of the program changes and it is stopped.
fn run(
    program: &Path,
    src: &Path,
    log: &Path,
    changes: &Mutex<Receiver<String>>,
) -> Result<Ran, String> {
    let stderr = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log)
        .map_err(|why| format!("{}: {why}", log.display()))?;
    hold_interrupts(true);
    let child = Command::new(program)
        .env(pencere::DEV_VARIABLE, src)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::from(stderr))
        .spawn();
    let mut child: Child = match child {
        Ok(child) => child,
        Err(why) => {
            hold_interrupts(false);
            return Err(format!("{} could not be run: {why}", program.display()));
        }
    };
    let ran = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Ran::Done(status_of(status)),
            Ok(None) => {}
            Err(why) => {
                hold_interrupts(false);
                return Err(format!("the program could not be waited for: {why}"));
            }
        }
        let received = match changes.lock() {
            Ok(changes) => changes.recv_timeout(Duration::from_millis(100)),
            Err(_) => Err(RecvTimeoutError::Disconnected),
        };
        match received {
            Ok(path) if is_view(&path) => {}
            Ok(_) => {
                std::thread::sleep(Duration::from_millis(120));
                if let Ok(changes) = changes.lock() {
                    while changes.try_recv().is_ok() {}
                }
                stop(&mut child);
                break Ran::Changed;
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => {
                let status = child.wait().map(status_of).unwrap_or(1);
                break Ran::Done(status);
            }
        }
    };
    hold_interrupts(false);
    Ok(ran)
}

/// Asks the program to end (`SIGTERM`, on which the `pencere` library closes the view and gives the
/// terminal back), and waits for it; one that does not end within two seconds is killed.
#[allow(unsafe_code)]
fn stop(child: &mut Child) {
    if let Ok(pid) = libc::pid_t::try_from(child.id()) {
        // SAFETY: a signal to the process this one started, which it has not waited for yet.
        unsafe {
            libc::kill(pid, libc::SIGTERM);
        }
    }
    let until = std::time::Instant::now() + Duration::from_secs(2);
    while std::time::Instant::now() < until {
        if matches!(child.try_wait(), Ok(Some(_))) {
            return;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    let _ = child.kill();
    let _ = child.wait();
}

/// The status a shell would report: the code, or 128 and the signal.
fn status_of(status: std::process::ExitStatus) -> i32 {
    use std::os::unix::process::ExitStatusExt as _;
    status
        .code()
        .unwrap_or_else(|| 128 + status.signal().unwrap_or(0))
}

/// `pencere dev` for the Cargo package at `root`.
pub fn command(root: &Path, name: Option<String>, log: Option<PathBuf>) -> ExitCode {
    let root = match root.canonicalize() {
        Ok(root) => root,
        Err(why) => {
            eprintln!("pencere: {}: {why}", root.display());
            return ExitCode::from(1);
        }
    };
    let name = name.unwrap_or_else(|| {
        root.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("app")
            .to_owned()
    });
    let log = log.unwrap_or_else(|| root.join(".pencere/log"));
    if let Some(parent) = log.parent()
        && let Err(why) = std::fs::create_dir_all(parent)
    {
        eprintln!("pencere: {}: {why}", parent.display());
        return ExitCode::from(1);
    }
    let src = root.join("src");
    eprintln!(
        "pencere dev: {name} (Rust); its standard error goes to {} (a change to the program builds and \
         runs it again, a change to the page shows it again)",
        log.display()
    );
    let (tell, changes) = channel();
    let stop_watch = Arc::new(AtomicBool::new(false));
    {
        let (root, stop_watch) = (root.clone(), Arc::clone(&stop_watch));
        std::thread::spawn(move || watch(root, tell, stop_watch));
    }
    let changes = Mutex::new(changes);
    let status = loop {
        let program = match build(&root, &name, &[]) {
            Ok(program) => program,
            Err(why) => {
                eprintln!("pencere dev: {why}; waiting for a change, ⌃C to leave");
                match changes.lock() {
                    Ok(changes) if wait_for_change(&changes).is_some() => continue,
                    _ => break 1,
                }
            }
        };
        match run(&program, &src, &log, &changes) {
            Ok(Ran::Changed) => {
                eprintln!("pencere dev: the program changed; building {name} again")
            }
            Ok(Ran::Done(status)) if status == 130 || status == 143 => break status,
            Ok(Ran::Done(status)) => {
                append(
                    &log,
                    &format!("pencere dev: {name} finished (status {status})"),
                );
                eprintln!(
                    "pencere dev: {name} finished (status {status}); waiting for a change, ⌃C to leave"
                );
                match changes.lock() {
                    Ok(changes) if wait_for_change(&changes).is_some() => {}
                    _ => break status,
                }
            }
            Err(why) => {
                eprintln!("pencere dev: {why}; waiting for a change, ⌃C to leave");
                match changes.lock() {
                    Ok(changes) if wait_for_change(&changes).is_some() => {}
                    _ => break 1,
                }
            }
        }
    };
    stop_watch.store(true, Ordering::SeqCst);
    ExitCode::from(u8::try_from(status & 0xff).unwrap_or(1))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_page_and_the_pictures_are_the_views_and_the_rest_is_the_programs() {
        assert!(is_view("src/view/page.lua"));
        assert!(is_view("src/view/parts/ui.lua"));
        assert!(is_view("src/assets/icon.svg"));
        assert!(!is_view("src/program/main.rs"));
        assert!(!is_view("Cargo.toml"));
        assert!(!is_view("build.rs"));
        assert!(!is_view("src/viewer.rs"));
    }

    #[test]
    fn what_cargo_and_git_keep_is_not_looked_at() {
        let root = std::env::temp_dir().join(format!("pencere-dev-cargo-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        for path in [
            "Cargo.toml",
            "src/program/main.rs",
            "src/view/page.lua",
            "target/debug/x",
            ".git/HEAD",
        ] {
            let path = root.join(path);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, "x").unwrap();
        }
        let found: Vec<String> = stamps(&root).into_keys().collect();
        assert_eq!(
            found,
            ["Cargo.toml", "src/program/main.rs", "src/view/page.lua"]
        );
        std::fs::remove_dir_all(root).unwrap();
    }
}
