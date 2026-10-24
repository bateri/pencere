//! `pencere dev`: the app in a directory, run in this pane, and run again when its files change.
//!
//! A change to a script starts the program again; a change to the page or to what is in `assets/`
//! shows the view again without ending the program (the terminal is asked to close the view and
//! open the new one; one that cannot is handled as a change to a script). The program's `print` and
//! errors go to a log file, because the terminal is under the view.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, SystemTime};

use crate::files::Files;
use crate::runtime::{Config, Exit, Reloaded, Remote, Runner};

/// How often the files are looked at.
const EVERY: Duration = Duration::from_millis(250);

/// What a change to a file is to the app that runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Change {
    /// A script: the program is run again.
    Program(String),
    /// A page, a picture, a file of a web page: the view is shown again.
    View(String),
}

/// What changed in a file at `path`, when `pages` are the scripts that are pages of the view.
pub fn classify(path: &str, pages: &[String]) -> Change {
    if path.ends_with(".lua") && !pages.iter().any(|page| page == path) {
        Change::Program(path.to_owned())
    } else {
        Change::View(path.to_owned())
    }
}

/// The files of a directory and when each last changed.
type Stamps = BTreeMap<String, (SystemTime, u64)>;

fn stamps(root: &Path) -> Stamps {
    let files = Files::in_dir(root);
    let mut stamps = BTreeMap::new();
    for path in files.paths().unwrap_or_default() {
        if let Ok(meta) = std::fs::metadata(root.join(&path)) {
            stamps.insert(
                path,
                (
                    meta.modified().unwrap_or(SystemTime::UNIX_EPOCH),
                    meta.len(),
                ),
            );
        }
    }
    stamps
}

/// The files that are different between two looks at a directory: changed, new, or gone.
pub fn changed(before: &Stamps, after: &Stamps) -> Vec<String> {
    let mut found = Vec::new();
    for (path, stamp) in after {
        if before.get(path) != Some(stamp) {
            found.push(path.clone());
        }
    }
    for path in before.keys() {
        if !after.contains_key(path) {
            found.push(path.clone());
        }
    }
    found
}

/// Looks at the directory until `stop`, and tells which files change.
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

/// Waits for a change; the first one, and what came with it at the same time is let go.
fn wait_for_change(changed: &Receiver<String>) -> Option<String> {
    let first = changed.recv().ok()?;
    // A save is often two or three writes.
    std::thread::sleep(Duration::from_millis(120));
    while changed.try_recv().is_ok() {}
    Some(first)
}

/// Follows the changes while one run of the app goes on: reloads the view, or ends the run.
fn follow(
    changes: Arc<Mutex<Receiver<String>>>,
    remote: Remote,
    root: PathBuf,
    log: PathBuf,
    over: Arc<AtomicBool>,
) {
    while !over.load(Ordering::SeqCst) {
        let received = match changes.lock() {
            Ok(changes) => changes.recv_timeout(Duration::from_millis(100)),
            Err(_) => return,
        };
        let first = match received {
            Ok(path) => path,
            Err(RecvTimeoutError::Timeout) => continue,
            Err(RecvTimeoutError::Disconnected) => return,
        };
        // A save is often two or three writes.
        std::thread::sleep(Duration::from_millis(120));
        let mut paths = vec![first];
        if let Ok(changes) = changes.lock() {
            paths.extend(changes.try_iter());
        }
        let pages = remote.pages();
        let program_changed = paths
            .iter()
            .any(|path| matches!(classify(path, &pages), Change::Program(_)));
        if program_changed {
            remote.restart();
            return;
        }
        match remote.reload_view(&Files::in_dir(&root)) {
            Reloaded::Yes | Reloaded::NoView => {}
            Reloaded::Unsupported => {
                // The terminal cannot close a view for the program: the program is run again.
                remote.restart();
                return;
            }
            Reloaded::Failed(why) => {
                append(
                    &log,
                    &format!("pencere dev: the view could not be shown again: {why}"),
                );
            }
        }
    }
}

fn append(log: &Path, line: &str) {
    use std::io::Write as _;
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(log)
    {
        let _ = writeln!(file, "{line}");
    }
}

pub fn command(args: &[String]) -> ExitCode {
    let mut dir = None;
    let mut log = None;
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--log" => match args.next() {
                Some(file) => log = Some(PathBuf::from(file)),
                None => return crate::usage("--log needs the file to write to"),
            },
            other if other.starts_with('-') => {
                return crate::usage(format!("{other:?} is not an option of `dev`"));
            }
            other if dir.is_none() => dir = Some(PathBuf::from(other)),
            other => return crate::usage(format!("{other:?}: one directory is enough")),
        }
    }
    let root = dir.unwrap_or_else(|| PathBuf::from("."));
    if !root.join("app.lua").is_file() {
        eprintln!(
            "pencere: {} has no app.lua: an app is a directory with one (`pencere new` makes one)",
            root.display()
        );
        return ExitCode::from(1);
    }
    let name = match std::path::absolute(&root) {
        Ok(path) => path.file_name().map_or_else(
            || "app".to_owned(),
            |name| name.to_string_lossy().into_owned(),
        ),
        Err(why) => {
            eprintln!("pencere: {}: {why}", root.display());
            return ExitCode::from(1);
        }
    };
    let log = log.unwrap_or_else(|| root.join(".pencere/log"));
    if let Some(parent) = log.parent()
        && let Err(why) = std::fs::create_dir_all(parent)
    {
        eprintln!("pencere: {}: {why}", parent.display());
        return ExitCode::from(1);
    }
    eprintln!(
        "pencere dev: {name}; print and errors go to {} (a change to a file runs the app again)",
        log.display()
    );
    let (tell, changed) = channel();
    let stop = Arc::new(AtomicBool::new(false));
    {
        let (root, stop) = (root.clone(), Arc::clone(&stop));
        std::thread::spawn(move || watch(root, tell, stop));
    }
    let changed = Arc::new(Mutex::new(changed));
    let status = loop {
        let config = Config {
            files: Files::in_dir(&root),
            name: name.clone(),
            args: Vec::new(),
            log: Some(log.clone()),
        };
        let runner = match Runner::new(config) {
            Ok(runner) => runner,
            Err(why) => {
                eprintln!("pencere: {why}");
                break 1;
            }
        };
        let remote = runner.remote();
        let over = Arc::new(AtomicBool::new(false));
        let following = {
            let (changed, root, log, over) = (
                Arc::clone(&changed),
                root.clone(),
                log.clone(),
                Arc::clone(&over),
            );
            std::thread::spawn(move || follow(changed, remote, root, log, over))
        };
        let exit = runner.run();
        over.store(true, Ordering::SeqCst);
        let _ = following.join();
        match exit {
            Exit::Restart => {
                eprintln!("pencere dev: a file changed; running {name} again");
            }
            Exit::Done(status) if status == 130 || status == 143 => break status,
            Exit::Done(status) => {
                eprintln!(
                    "pencere dev: {name} finished (status {status}); waiting for a change, ⌃C to leave"
                );
                if let Ok(changed) = changed.lock()
                    && wait_for_change(&changed).is_none()
                {
                    break status;
                }
            }
            Exit::Failed(why) => {
                eprintln!("pencere dev: {why}\npencere dev: waiting for a change, ⌃C to leave");
                if let Ok(changed) = changed.lock()
                    && wait_for_change(&changed).is_none()
                {
                    break 1;
                }
            }
        }
    };
    stop.store(true, Ordering::SeqCst);
    ExitCode::from(u8::try_from(status & 0xff).unwrap_or(1))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stamp(n: u64) -> (SystemTime, u64) {
        (SystemTime::UNIX_EPOCH + Duration::from_secs(n), n)
    }

    #[test]
    fn a_script_runs_the_program_again_and_the_page_and_the_files_of_the_view_show_it_again() {
        let before: Stamps = [
            ("app.lua".to_owned(), stamp(1)),
            ("page.lua".to_owned(), stamp(1)),
            ("assets/a.svg".to_owned(), stamp(1)),
            ("web/app.js".to_owned(), stamp(1)),
            ("lib/x.lua".to_owned(), stamp(1)),
        ]
        .into();
        let mut after = before.clone();
        assert!(changed(&before, &after).is_empty());
        after.insert("app.lua".to_owned(), stamp(2));
        after.insert("page.lua".to_owned(), stamp(2));
        after.remove("assets/a.svg");
        after.insert("web/app.js".to_owned(), stamp(2));
        after.insert("lib/new.lua".to_owned(), stamp(2));
        let pages = ["page.lua".to_owned()];
        let found: Vec<Change> = changed(&before, &after)
            .iter()
            .map(|path| classify(path, &pages))
            .collect();
        assert!(found.contains(&Change::Program("app.lua".into())));
        assert!(found.contains(&Change::Program("lib/new.lua".into())));
        assert!(found.contains(&Change::View("page.lua".into())));
        assert!(found.contains(&Change::View("assets/a.svg".into())));
        assert!(found.contains(&Change::View("web/app.js".into())));
        // A view that is only a web page has no script that is its page.
        assert_eq!(
            classify("page.lua", &[]),
            Change::Program("page.lua".into())
        );
    }
}
