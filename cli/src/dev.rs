//! `pencere dev`: the app in a directory, run in this pane, and run again when its files change.
//! What is looked at is the app's own files (`src/` of a directory with the layout, the page's
//! build directory when `--web` names one), and no leavings of its tools.
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

use crate::files::{Files, Project};
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

/// What changed in a file at `path`, when `pages` are the scripts that are pages of the view. The
/// scripts in `view/` are the page's to `require`, as the pages are: a change to one shows the view
/// again.
pub fn classify(path: &str, pages: &[String]) -> Change {
    let of_the_view = pages.iter().any(|page| page == path) || path.starts_with("view/");
    if path.ends_with(".lua") && !of_the_view {
        Change::Program(path.to_owned())
    } else {
        Change::View(path.to_owned())
    }
}

/// The files of a directory and when each last changed.
type Stamps = BTreeMap<String, (SystemTime, u64)>;

fn stamps(files: &Files) -> Stamps {
    let mut stamps = BTreeMap::new();
    for path in files.paths().unwrap_or_default() {
        if let Some(disk) = files.on_disk(&path)
            && let Ok(meta) = std::fs::metadata(disk)
        {
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
fn watch(project: Arc<Project>, tell: Sender<String>, stop: Arc<AtomicBool>) {
    let mut before = stamps(&project.files());
    while !stop.load(Ordering::SeqCst) {
        std::thread::sleep(EVERY);
        let after = stamps(&project.files());
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
    project: Arc<Project>,
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
        let pages = remote.pages(&project.files());
        let program_changed = paths
            .iter()
            .any(|path| matches!(classify(path, &pages), Change::Program(_)));
        if program_changed {
            remote.restart();
            return;
        }
        match remote.reload_view(&project.files()) {
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
    let mut web = None;
    let mut name = None;
    let mut log = None;
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--log" => match args.next() {
                Some(file) => log = Some(PathBuf::from(file)),
                None => return crate::usage("--log needs the file to write to"),
            },
            "--web" => match args.next() {
                Some(directory) => web = Some(PathBuf::from(directory)),
                None => return crate::usage("--web needs the directory the page is built into"),
            },
            "--name" => match args.next() {
                Some(value) => name = Some(value.clone()),
                None => return crate::usage("--name needs a name"),
            },
            other if other.starts_with('-') => {
                return crate::usage(format!("{other:?} is not an option of `dev`"));
            }
            other if dir.is_none() => dir = Some(PathBuf::from(other)),
            other => return crate::usage(format!("{other:?}: one directory is enough")),
        }
    }
    let root = dir.unwrap_or_else(|| PathBuf::from("."));
    let project = match Project::find(&root, web.as_deref()) {
        Ok(project) => Arc::new(project),
        Err(why) => {
            eprintln!("pencere: {why}");
            return ExitCode::from(1);
        }
    };
    let name = match name {
        Some(name) => name,
        None => match project.name() {
            Ok(name) => name,
            Err(why) => {
                eprintln!("pencere: {why}");
                return ExitCode::from(1);
            }
        },
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
        let (project, stop) = (Arc::clone(&project), Arc::clone(&stop));
        std::thread::spawn(move || watch(project, tell, stop));
    }
    let changed = Arc::new(Mutex::new(changed));
    let status = loop {
        let config = Config {
            files: project.files(),
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
            let (changed, project, log, over) = (
                Arc::clone(&changed),
                Arc::clone(&project),
                log.clone(),
                Arc::clone(&over),
            );
            std::thread::spawn(move || follow(changed, remote, project, log, over))
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
        // The page of an app with the layout is in `view/`.
        let pages = ["view/page.lua".to_owned()];
        assert_eq!(
            classify("view/page.lua", &pages),
            Change::View("view/page.lua".into())
        );
        assert_eq!(
            classify("api/files.lua", &pages),
            Change::Program("api/files.lua".into())
        );
        // What the page requires is in `view/` too, and is the page's.
        assert_eq!(
            classify("view/screens/logs.lua", &pages),
            Change::View("view/screens/logs.lua".into())
        );
        assert_eq!(
            classify("view/parts/init.luau", &pages),
            Change::View("view/parts/init.luau".into())
        );
    }
}
