//! The app's own program, run in this process for a test: the Lua runtime of the command, given an
//! environment of its own and a view handed over instead of a terminal, on a thread of its own.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{RecvTimeoutError, channel};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use pencere_cli::{Config, Env, Exit, Handed, Project, Remote, Runner, Serve, Served};
use pencere_drive::headless::InProcess;

/// How long the program is given to end when its page is let go of.
const GRACE: Duration = Duration::from_secs(5);

/// What the program is run with: the layers over the process's own.
pub struct Setting {
    pub root: PathBuf,
    pub args: Vec<String>,
    pub env: BTreeMap<String, String>,
    pub cwd: Option<PathBuf>,
    /// Where its `print` and errors go.
    pub log: PathBuf,
}

/// The program running, and what ends it.
pub struct App {
    remote: Remote,
    thread: Option<JoinHandle<Exit>>,
    /// The `serve` of the program, which ends when this is let go of.
    served: Option<Served>,
}

impl App {
    /// Starts the program of the app in `setting` and waits, at most `within`, for it to show its
    /// view: the conversation with it, and the program.
    pub fn start(setting: Setting, within: Duration) -> Result<(Self, InProcess), String> {
        let (hand, handed) = channel::<Handed>();
        let (ready, remote) = channel::<Result<Remote, String>>();
        let log = setting.log.clone();
        let thread = std::thread::Builder::new()
            .name("pencere-test-program".into())
            .spawn(move || run(setting, hand, &ready))
            .map_err(|error| format!("the app's program cannot be started: {error}"))?;
        let remote = match remote.recv_timeout(within) {
            Ok(Ok(remote)) => remote,
            Ok(Err(why)) => return Err(why),
            Err(_) => return Err("the app's program did not start".to_owned()),
        };
        let mut app = Self {
            remote,
            thread: Some(thread),
            served: None,
        };
        let began = Instant::now();
        loop {
            match handed.recv_timeout(Duration::from_millis(20)) {
                Ok(taken) => {
                    app.served = Some(taken.served);
                    return Ok((app, InProcess::new(taken.options)));
                }
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => {}
            }
            let over = app.thread.as_ref().is_none_or(JoinHandle::is_finished);
            if over {
                let exit = app.thread.take().map(JoinHandle::join);
                let how = match exit {
                    Some(Ok(Exit::Failed(why))) => format!("it failed: {why}"),
                    Some(Ok(Exit::Done(status))) => format!("it ended with the status {status}"),
                    Some(Ok(Exit::Restart)) => "it was asked to start again".to_owned(),
                    _ => "its thread panicked".to_owned(),
                };
                return Err(format!(
                    "the app's program ended before it showed its view: {how}{}",
                    tail(&log)
                ));
            }
            if began.elapsed() > within {
                app.stop();
                return Err(format!(
                    "the app's program did not show its view within {within:?}{}",
                    tail(&log)
                ));
            }
        }
    }

    /// Ends the program: its view is over, and it is asked to end if it goes on.
    pub fn stop(mut self) {
        drop(self.served.take());
        self.remote.restart();
        let Some(thread) = self.thread.take() else {
            return;
        };
        let until = Instant::now() + GRACE;
        while !thread.is_finished() && Instant::now() < until {
            std::thread::sleep(Duration::from_millis(5));
        }
        if thread.is_finished() {
            let _ = thread.join();
        }
    }
}

/// The last of what the program printed, for the words of a failure.
fn tail(log: &Path) -> String {
    let Ok(text) = std::fs::read_to_string(log) else {
        return String::new();
    };
    let lines: Vec<&str> = text.lines().collect();
    let from = lines.len().saturating_sub(12);
    let shown = lines.get(from..).unwrap_or_default().join("\n");
    if shown.is_empty() {
        String::new()
    } else {
        format!("\nwhat it printed:\n{shown}")
    }
}

fn run(
    setting: Setting,
    hand: std::sync::mpsc::Sender<Handed>,
    ready: &std::sync::mpsc::Sender<Result<Remote, String>>,
) -> Exit {
    let made = (|| {
        let project = Project::find(&setting.root, None)?;
        let name = project.name()?;
        let mut env = Env::of_process().map_err(|error| error.to_string())?;
        env.vars.extend(setting.env);
        env.cwd = setting.cwd.unwrap_or(setting.root.clone());
        let config = Config {
            files: project.files(),
            name,
            args: setting.args,
            log: Some(setting.log),
            env: Some(env),
            modules: Vec::new(),
            serve: Serve::Hand(hand),
        };
        Runner::new(config).map_err(|error| error.to_string())
    })();
    match made {
        Ok(runner) => {
            let _ = ready.send(Ok(runner.remote()));
            runner.run()
        }
        Err(why) => {
            let _ = ready.send(Err(format!("the app's program cannot be run: {why}")));
            Exit::Failed(why)
        }
    }
}
