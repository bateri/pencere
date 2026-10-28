//! The run: which files, how many workers, what each worker is told, what is said and in what
//! order, and how a run ends.

use std::collections::VecDeque;
use std::io::{BufRead as _, BufReader, Write as _};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, ExitCode, Stdio};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::time::{Duration, Instant};

use pencere_cli::Project;
use serde_json::Value;

use crate::file::{Job, slug};
use crate::report::{self, FileResult, Pretty, Reporter, Status, Summary, Tap, TestResult};

/// How long past its own timeout a test is let be before its worker is ended: the worker cuts a
/// test short itself, and this is for the one stuck in a call it cannot cut.
const GRACE: Duration = Duration::from_secs(5);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Pretty,
    Tap,
}

#[derive(Debug)]
pub struct Options {
    pub paths: Vec<String>,
    pub grep: Option<String>,
    pub jobs: Option<usize>,
    pub timeout: f64,
    pub format: Format,
    pub junit: Option<PathBuf>,
    pub artifacts: Option<PathBuf>,
    pub root: Option<PathBuf>,
}

/// The app a run is of: the nearest directory, from `start` up, that is one.
pub fn find_root(start: &Path) -> Option<PathBuf> {
    let mut at = Some(start);
    while let Some(directory) = at {
        if Project::find(directory, None).is_ok() {
            return Some(directory.to_path_buf());
        }
        at = directory.parent();
    }
    None
}

/// The test files under `directory`: `*_test.lua`, sorted.
pub fn find_tests(directory: &Path, found: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    let mut entries: Vec<PathBuf> = entries.flatten().map(|entry| entry.path()).collect();
    entries.sort();
    for path in entries {
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("");
        if name.starts_with('.') {
            continue;
        }
        if path.is_dir() {
            find_tests(&path, found);
        } else if name.ends_with("_test.lua") {
            found.push(path);
        }
    }
}

/// `file`, `file:line` or a directory, as a person types it: the files and the line, if one.
fn resolve(argument: &str, root: &Path, cwd: &Path) -> Result<Vec<(PathBuf, Option<u32>)>, String> {
    let (path_part, line) = match argument.rsplit_once(':') {
        Some((path, line))
            if !path.is_empty() && line.chars().all(|c| c.is_ascii_digit()) && !line.is_empty() =>
        {
            (path, line.parse::<u32>().ok())
        }
        _ => (argument, None),
    };
    let given = Path::new(path_part);
    let path = if given.is_absolute() {
        given.to_path_buf()
    } else if cwd.join(given).exists() {
        cwd.join(given)
    } else {
        root.join(given)
    };
    if path.is_dir() {
        let mut files = Vec::new();
        find_tests(&path, &mut files);
        return Ok(files.into_iter().map(|file| (file, line)).collect());
    }
    if path.is_file() {
        return Ok(vec![(path, line)]);
    }
    Err(format!("{argument}: no such test file or directory"))
}

struct Worker {
    child: Child,
    stdin: Option<ChildStdin>,
    generation: u64,
    /// The file it is given: its place in the run.
    file: Option<usize>,
    /// When the test it is on must be over, with the grace.
    by: Option<Instant>,
    killed: bool,
}

enum Message {
    Line(usize, u64, String),
    Closed(usize, u64),
}

struct Progress {
    result: FileResult,
    planned: Vec<String>,
    running: Option<(String, u32)>,
    finished: bool,
    started: bool,
    shown: usize,
}

fn spawn(
    exe: &Path,
    id: usize,
    generation: u64,
    sender: &Sender<Message>,
) -> Result<Worker, String> {
    let mut child = Command::new(exe)
        .arg("--worker")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
        .map_err(|error| format!("{}: {error}", exe.display()))?;
    let stdout = child.stdout.take().ok_or("the worker has no output")?;
    let stdin = child.stdin.take();
    let sender = sender.clone();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            let Ok(line) = line else {
                break;
            };
            if sender.send(Message::Line(id, generation, line)).is_err() {
                return;
            }
        }
        let _ = sender.send(Message::Closed(id, generation));
    });
    Ok(Worker {
        child,
        stdin,
        generation,
        file: None,
        by: None,
        killed: false,
    })
}

fn reporter(format: Format, root: &Path) -> Box<dyn Reporter> {
    match format {
        Format::Pretty => Box::new(Pretty::new(root)),
        Format::Tap => Box::new(Tap::new()),
    }
}

pub fn run(options: &Options, exe: &Path) -> ExitCode {
    let cwd = match std::env::current_dir() {
        Ok(cwd) => cwd,
        Err(error) => {
            eprintln!("pencere-test: {error}");
            return ExitCode::from(1);
        }
    };
    let Some(root) = options.root.clone().or_else(|| find_root(&cwd)) else {
        eprintln!(
            "pencere-test: no app here: a directory with src/program/app.lua (or an app.lua) is the app whose tests run, here or above"
        );
        return ExitCode::from(1);
    };
    let root = std::fs::canonicalize(&root).unwrap_or(root);
    let mut files: Vec<(PathBuf, Option<u32>)> = Vec::new();
    if options.paths.is_empty() {
        let mut found = Vec::new();
        find_tests(&root.join("test"), &mut found);
        files.extend(found.into_iter().map(|file| (file, None)));
    } else {
        for argument in &options.paths {
            match resolve(argument, &root, &cwd) {
                Ok(found) => files.extend(found),
                Err(why) => {
                    eprintln!("pencere-test: {why}");
                    return ExitCode::from(64);
                }
            }
        }
    }
    if files.is_empty() {
        eprintln!(
            "pencere-test: no test files (*_test.lua under {}/test): a run that looked at nothing proves nothing",
            root.display()
        );
        return ExitCode::from(1);
    }
    let artifacts = options.artifacts.clone().map_or_else(
        || root.join(".pencere/test"),
        |dir| {
            if dir.is_absolute() {
                dir
            } else {
                cwd.join(dir)
            }
        },
    );
    let ci = std::env::var("CI")
        .is_ok_and(|value| !value.is_empty() && value != "false" && value != "0");
    let jobs: Vec<Job> = files
        .iter()
        .map(|(path, line)| {
            let label = path.strip_prefix(&root).map_or_else(
                |_| path.display().to_string(),
                |relative| relative.display().to_string(),
            );
            // The files a run keeps are the ones of this run alone.
            let _ = std::fs::remove_dir_all(artifacts.join(slug(&label)));
            Job {
                label,
                path: path.clone(),
                root: root.clone(),
                grep: options.grep.clone(),
                line: *line,
                timeout: options.timeout,
                ci,
                artifacts: artifacts.clone(),
            }
        })
        .collect();
    let count = options
        .jobs
        .unwrap_or_else(|| {
            std::thread::available_parallelism()
                .map_or(2, usize::from)
                .min(8)
        })
        .clamp(1, jobs.len());
    let began = Instant::now();
    let mut sink = reporter(options.format, &root);
    let results = match execute(&jobs, count, exe, sink.as_mut()) {
        Ok(results) => results,
        Err(why) => {
            eprintln!("pencere-test: {why}");
            return ExitCode::from(1);
        }
    };
    let summary = Summary::of(
        &results,
        u64::try_from(began.elapsed().as_millis()).unwrap_or(u64::MAX),
    );
    sink.finish(&summary, &results);
    if let Some(path) = &options.junit {
        let text = report::junit(&results, &summary);
        if let Err(error) = std::fs::write(path, text) {
            eprintln!("pencere-test: {}: {error}", path.display());
            return ExitCode::from(1);
        }
    }
    if summary.ok() {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}

fn string(value: &Value, name: &str) -> Option<String> {
    value.get(name).and_then(Value::as_str).map(str::to_owned)
}

fn test_of(event: &Value) -> TestResult {
    TestResult {
        name: string(event, "name").unwrap_or_default(),
        line: event
            .get("line")
            .and_then(Value::as_u64)
            .and_then(|line| u32::try_from(line).ok())
            .unwrap_or(0),
        status: Status::named(
            event
                .get("status")
                .and_then(Value::as_str)
                .unwrap_or("fail"),
        ),
        ms: event.get("ms").and_then(Value::as_u64).unwrap_or(0),
        message: string(event, "message"),
        trace: string(event, "trace"),
        words: event
            .get("words")
            .and_then(Value::as_array)
            .map(|list| {
                list.iter()
                    .filter_map(|word| word.as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default(),
        artifacts: event
            .get("artifacts")
            .and_then(Value::as_array)
            .map(|list| {
                list.iter()
                    .filter_map(|item| item.as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default(),
        output: string(event, "output").unwrap_or_default(),
    }
}

/// The files, run on `count` workers; what each did, in the order of the files.
fn execute(
    jobs: &[Job],
    count: usize,
    exe: &Path,
    sink: &mut dyn Reporter,
) -> Result<Vec<FileResult>, String> {
    let (sender, receiver): (Sender<Message>, Receiver<Message>) = channel();
    let mut workers: Vec<Worker> = Vec::new();
    for id in 0..count {
        workers.push(spawn(exe, id, 0, &sender)?);
    }
    let mut queue: VecDeque<usize> = (0..jobs.len()).collect();
    let mut progress: Vec<Progress> = jobs
        .iter()
        .map(|job| Progress {
            result: FileResult {
                label: job.label.clone(),
                tests: Vec::new(),
                error: None,
            },
            planned: Vec::new(),
            running: None,
            finished: false,
            started: false,
            shown: 0,
        })
        .collect();
    let mut head = 0;
    while head < jobs.len() {
        // Idle workers are given files.
        for worker in &mut workers {
            if worker.file.is_some() {
                continue;
            }
            let Some(next) = queue.pop_front() else {
                continue;
            };
            let Some(job) = jobs.get(next) else {
                continue;
            };
            let mut line = job.to_json().to_string();
            line.push('\n');
            let sent = worker.stdin.as_mut().is_some_and(|stdin| {
                stdin
                    .write_all(line.as_bytes())
                    .and_then(|()| stdin.flush())
                    .is_ok()
            });
            worker.file = Some(next);
            if !sent && let Some(item) = progress.get_mut(next) {
                item.result.error = Some("the worker cannot be reached".to_owned());
                item.finished = true;
                worker.file = None;
            }
        }
        match receiver.recv_timeout(Duration::from_millis(50)) {
            Ok(Message::Line(id, generation, text)) => {
                let Some(worker) = workers.get_mut(id) else {
                    continue;
                };
                if worker.generation != generation {
                    continue;
                }
                let Ok(event) = serde_json::from_str::<Value>(&text) else {
                    continue;
                };
                let Some(file) = worker.file else {
                    continue;
                };
                let Some(item) = progress.get_mut(file) else {
                    continue;
                };
                match event.get("ev").and_then(Value::as_str).unwrap_or("") {
                    "plan" => {
                        item.planned = event
                            .get("tests")
                            .and_then(Value::as_array)
                            .map(|list| {
                                list.iter()
                                    .filter_map(|n| n.as_str().map(str::to_owned))
                                    .collect()
                            })
                            .unwrap_or_default();
                    }
                    "start" => {
                        let seconds = event.get("timeout").and_then(Value::as_f64).unwrap_or(30.0);
                        worker.by = Some(
                            Instant::now()
                                + Duration::try_from_secs_f64(seconds)
                                    .unwrap_or(Duration::from_secs(30))
                                + GRACE,
                        );
                        item.running = Some((
                            string(&event, "name").unwrap_or_default(),
                            event
                                .get("line")
                                .and_then(Value::as_u64)
                                .and_then(|line| u32::try_from(line).ok())
                                .unwrap_or(0),
                        ));
                    }
                    "test" => {
                        worker.by = None;
                        item.running = None;
                        item.result.tests.push(test_of(&event));
                    }
                    "file_error" => {
                        item.result.error = string(&event, "message");
                    }
                    "done" => {
                        item.finished = true;
                        worker.file = None;
                        worker.by = None;
                    }
                    _ => {}
                }
            }
            Ok(Message::Closed(id, generation)) => {
                let Some(worker) = workers.get_mut(id) else {
                    continue;
                };
                if worker.generation != generation {
                    continue;
                }
                let killed = worker.killed;
                let _ = worker.child.wait();
                if let Some(file) = worker.file.take()
                    && let Some(item) = progress.get_mut(file)
                {
                    end_early(item, killed);
                }
                let next = worker.generation + 1;
                *worker = spawn(exe, id, next, &sender)?;
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return Err("the workers are gone".to_owned()),
        }
        // A test past its time and its grace is stuck where Lua cannot cut it: its worker is ended.
        let now = Instant::now();
        for worker in &mut workers {
            if worker.by.is_some_and(|by| now > by) && !worker.killed {
                worker.killed = true;
                let _ = worker.child.kill();
            }
        }
        // What can be said, in the order of the files.
        while let Some(item) = progress.get_mut(head) {
            if !item.started {
                item.started = true;
                sink.file_start(&item.result.label);
            }
            while let Some(test) = item.result.tests.get(item.shown) {
                sink.test(&item.result.label, test);
                item.shown += 1;
            }
            if item.finished {
                sink.file_end(&item.result);
                head += 1;
            } else {
                break;
            }
        }
    }
    for worker in &mut workers {
        drop(worker.stdin.take());
    }
    for worker in &mut workers {
        let _ = worker.child.wait();
    }
    Ok(progress.into_iter().map(|item| item.result).collect())
}

/// A worker ended while it had a file: the test it was on failed with it, and the ones it did not
/// reach did not run.
fn end_early(item: &mut Progress, killed: bool) {
    if item.finished {
        return;
    }
    if let Some((name, line)) = item.running.take() {
        item.result.tests.push(TestResult {
            name,
            line,
            status: if killed { Status::Timeout } else { Status::Fail },
            ms: 0,
            message: Some(if killed {
                "the test did not end, and its process was ended: it was stuck where the interpreter cannot cut it short (a call that does not return)".to_owned()
            } else {
                "the process that ran the test ended while the test ran (it crashed)".to_owned()
            }),
            trace: None,
            words: Vec::new(),
            artifacts: Vec::new(),
            output: String::new(),
        });
    }
    let reported: Vec<String> = item
        .result
        .tests
        .iter()
        .map(|test| test.name.clone())
        .collect();
    for name in &item.planned {
        if !reported.contains(name) {
            item.result.tests.push(TestResult {
                name: name.clone(),
                line: 0,
                status: Status::NotRun,
                ms: 0,
                message: Some(
                    "it did not run: the process that ran the file ended before it".to_owned(),
                ),
                trace: None,
                words: Vec::new(),
                artifacts: Vec::new(),
                output: String::new(),
            });
        }
    }
    if item.result.error.is_none() && item.planned.is_empty() && item.result.tests.is_empty() {
        item.result.error =
            Some("the process that ran the file ended before any test ran".to_owned());
    }
    item.finished = true;
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("pencere-test-run-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap_or_default();
        dir
    }

    #[test]
    fn the_app_is_the_nearest_directory_with_a_program_and_its_tests_are_the_files_named_for_it() {
        let root = temp("find");
        for path in [
            "src/program/app.lua",
            "test/a_test.lua",
            "test/deep/b_test.lua",
            "test/helper.lua",
            "test/.hidden_test.lua",
        ] {
            let file = root.join(path);
            if let Some(parent) = file.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let _ = std::fs::write(file, "");
        }
        assert_eq!(find_root(&root.join("test/deep")), Some(root.clone()));
        let mut found = Vec::new();
        find_tests(&root.join("test"), &mut found);
        let shown: Vec<String> = found
            .iter()
            .filter_map(|path| path.strip_prefix(&root).ok())
            .map(|path| path.display().to_string())
            .collect();
        assert_eq!(shown, ["test/a_test.lua", "test/deep/b_test.lua"]);
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_path_with_a_line_picks_that_line_and_a_missing_one_is_said() {
        let root = temp("resolve");
        let _ = std::fs::create_dir_all(root.join("test"));
        let _ = std::fs::write(root.join("test/a_test.lua"), "");
        let found = resolve("test/a_test.lua:12", &root, &root).unwrap_or_default();
        assert_eq!(found, vec![(root.join("test/a_test.lua"), Some(12))]);
        assert!(resolve("test/none_test.lua", &root, &root).is_err());
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn a_worker_that_ended_mid_test_fails_that_test_and_leaves_the_rest_not_run() {
        let mut item = Progress {
            result: FileResult {
                label: "x".to_owned(),
                tests: vec![],
                error: None,
            },
            planned: vec!["a".to_owned(), "b".to_owned(), "c".to_owned()],
            running: Some(("b".to_owned(), 5)),
            finished: false,
            started: false,
            shown: 0,
        };
        item.result.tests.push(TestResult {
            name: "a".to_owned(),
            line: 1,
            status: Status::Pass,
            ms: 1,
            message: None,
            trace: None,
            words: vec![],
            artifacts: vec![],
            output: String::new(),
        });
        end_early(&mut item, true);
        let seen: Vec<(String, Status)> = item
            .result
            .tests
            .iter()
            .map(|test| (test.name.clone(), test.status))
            .collect();
        assert_eq!(
            seen,
            [
                ("a".to_owned(), Status::Pass),
                ("b".to_owned(), Status::Timeout),
                ("c".to_owned(), Status::NotRun)
            ]
        );
        assert!(item.finished);
    }
}
