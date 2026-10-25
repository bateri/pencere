//! The Lua runtime of an app: Luau, the `pencere` module, and the loop that runs them.

mod convert;
mod core;
mod ending;
mod events;
mod handoff;
mod modules;
mod pattern;
mod process;
mod serve;
mod stdin;
mod text;
mod watch;

use std::path::PathBuf;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use mlua::{Function, Lua};
use pencere::{CloseResult, ReopenResult, ViewControl};

pub use self::core::Exit;
use self::core::{Core, Log, ServePlan, ViewSpec};
use self::events::{Event, Events, channel_pair};
use crate::files::Files;

/// How long a loop waits for anything when nothing is due: it wakes for events and for the end of
/// a timer, so this is only the longest it sleeps without a reason.
const LONGEST_WAIT: Duration = Duration::from_secs(3600);

/// How long the view's thread is waited for when the app is over: the terminal is given back by
/// it, and the next thing the process does with the terminal must find it given back.
const VIEW_GRACE: Duration = Duration::from_secs(5);

/// An error to raise in Lua with these words. (The prelude takes the library's wrapping off them.)
fn fault(words: impl Into<String>) -> mlua::Error {
    mlua::Error::runtime(words.into())
}

/// What went wrong with a file or a process, without the number the system gives it.
fn why(error: &std::io::Error) -> String {
    let text = error.to_string();
    match text.rsplit_once(" (os error ") {
        Some((words, _)) => words.to_owned(),
        None => text,
    }
}

/// What an app is run with.
pub struct Config {
    /// The app's files.
    pub files: Files,
    /// What the app calls itself: the name of the program the terminal shows.
    pub name: String,
    /// What the app is given as `args`.
    pub args: Vec<String>,
    /// Where `print` and the errors go, instead of the terminal.
    pub log: Option<PathBuf>,
}

/// The way for another thread to the app that runs: `pencere dev`'s watcher.
#[derive(Clone)]
pub struct Remote {
    events: Events,
    control: ViewControl,
    plan: Arc<Mutex<Option<ServePlan>>>,
    restart: Arc<AtomicBool>,
}

/// What reloading the view came to.
#[derive(Debug, PartialEq, Eq)]
pub enum Reloaded {
    /// The view was closed and the terminal has taken the new one.
    Yes,
    /// There is no view to reload (the app has not shown one yet, or its view is over).
    NoView,
    /// The terminal cannot close a view at the program's request.
    Unsupported,
    /// The files do not make a view, or the terminal did not take it.
    Failed(String),
}

impl Remote {
    /// Asks the app to end and start again. Its view is closed first, so that the terminal is
    /// given back: by the terminal's own `dismiss` if it does that, otherwise as ⌃C does.
    pub fn restart(&self) {
        self.restart.store(true, Ordering::SeqCst);
        if self.control.close() == CloseResult::Unsupported {
            // SAFETY: a signal to this process, which the library's handler for it turns into a
            // `bye`.
            unsafe {
                libc::kill(libc::getpid(), libc::SIGINT);
            }
        }
        self.events.send(Event::Restart);
    }

    /// The scripts that are pages of the view the app shows: a change to one is a change of the
    /// view, and not of the program. Before the app has shown one, those it has.
    pub fn pages(&self, files: &Files) -> Vec<String> {
        let plan = self.plan.lock().ok().and_then(|plan| plan.clone());
        let views = match plan {
            Some(plan) => plan.views,
            None => serve::offered(files, None),
        };
        views
            .into_iter()
            .filter_map(|spec| match spec {
                ViewSpec::Lua { page, .. } => Some(page),
                ViewSpec::Web { .. } => None,
            })
            .collect()
    }

    /// Shows the view again, made from the files as they are now.
    pub fn reload_view(&self, files: &Files) -> Reloaded {
        let plan = self.plan.lock().ok().and_then(|plan| plan.clone());
        let Some(plan) = plan else {
            return Reloaded::NoView;
        };
        let views = match serve::views_of(files, &plan.views) {
            Ok(views) => views,
            Err(why) => return Reloaded::Failed(why),
        };
        match self.control.reopen(Some(views)) {
            Ok(ReopenResult::Reopened) => Reloaded::Yes,
            Ok(ReopenResult::Unsupported) => Reloaded::Unsupported,
            Ok(ReopenResult::Unavailable) => Reloaded::NoView,
            Ok(ReopenResult::NoView(no_view)) => Reloaded::Failed(format!(
                "the terminal did not take the view ({:?})",
                no_view.reason
            )),
            Ok(_) => Reloaded::Failed("the view could not be reopened".into()),
            Err(why) => Reloaded::Failed(why.to_string()),
        }
    }
}

/// An app made ready to run, and the way to it from other threads.
pub struct Runner {
    config: Config,
    events: Events,
    inbox: events::Inbox,
    remote: Remote,
}

impl Runner {
    pub fn new(config: Config) -> std::io::Result<Self> {
        let (events, inbox) = channel_pair()?;
        let remote = Remote {
            events: events.clone(),
            control: ViewControl::new(),
            plan: Arc::new(Mutex::new(None)),
            restart: Arc::new(AtomicBool::new(false)),
        };
        Ok(Self {
            config,
            events,
            inbox,
            remote,
        })
    }

    pub fn remote(&self) -> Remote {
        self.remote.clone()
    }

    /// Runs the app until it ends.
    pub fn run(self) -> Exit {
        let Config {
            files,
            name,
            args,
            log,
        } = self.config;
        let log = match Log::new(log) {
            Ok(log) => log,
            Err(why) => return Exit::Failed(format!("the log cannot be opened: {why}")),
        };
        let remote = self.remote;
        let core = Rc::new_cyclic(|me| {
            Core::new(
                Lua::new(),
                self.events,
                self.inbox,
                files,
                args,
                name,
                log,
                remote.control.clone(),
                Arc::clone(&remote.plan),
                Arc::clone(&remote.restart),
                me.clone(),
            )
        });
        let exit = start_and_wait(&core);
        // The view's thread gives the terminal back before anything else uses it.
        wait_for_view(&core);
        core.stop_children();
        core.clear();
        debug_assert_eq!(Rc::strong_count(&core), 1, "the loop outlives its run");
        exit
    }
}

fn start_and_wait(core: &Rc<Core>) -> Exit {
    if let Err(why) = start(core) {
        return Exit::Failed(why.to_string());
    }
    while core.exit().is_none() {
        // Nothing is left that could wake the loop: the app waits for what will never come.
        if !core.is_waiting() {
            core.end(Exit::Done(core.status.get()));
            break;
        }
        core.turn(LONGEST_WAIT);
    }
    core.exit().unwrap_or(Exit::Done(0))
}

/// Whether the files of an app are ones that can run: every script compiles, and the views the
/// app has are ones the library would offer.
pub fn check(files: &std::collections::BTreeMap<String, Vec<u8>>) -> Result<(), String> {
    let lua = Lua::new();
    for (path, bytes) in files {
        if crate::files::is_page_script(path) {
            lua.load(bytes.as_slice())
                .set_name(format!("@{path}"))
                .into_function()
                .map_err(|why| why.to_string())?;
        }
    }
    let packed = Files::packed(files.clone());
    for spec in serve::offered(&packed, None) {
        let what = match &spec {
            ViewSpec::Lua { page, .. } => page.clone(),
            ViewSpec::Web { dir, .. } => format!("{dir}/"),
        };
        serve::view_of(&packed, &spec).map_err(|why| format!("{what}: {why}"))?;
    }
    Ok(())
}

/// Loads the prelude and the app's script, and runs the script until it first waits.
fn start(core: &Rc<Core>) -> mlua::Result<()> {
    let lua = core.lua();
    let native = modules::natives(lua, core)?;
    let begin: Function = lua
        .load(include_str!("prelude.lua"))
        .set_name("=prelude")
        .call(native)?;
    let script = core
        .files
        .read("app.lua")
        .map_err(|why| mlua::Error::runtime(format!("app.lua cannot be read: {why}")))?;
    let main = lua.load(script).set_name("@app.lua").into_function()?;
    begin.call::<()>(main)
}

/// The app is over: if a view is up, its thread is waited for (a `Closed`/`NoView` it has already
/// posted is the end of that).
fn wait_for_view(core: &Rc<Core>) {
    let Some(mut run) = core.serve.borrow_mut().take() else {
        return;
    };
    let Some(thread) = run.thread.take() else {
        return;
    };
    // A view that is still up is told to go: the app has nothing left to serve it with. A terminal
    // that cannot close a view at the program's request is told as ⌃C tells it, with a `bye`.
    if core.control.close() == CloseResult::Unsupported {
        // SAFETY: a signal to this process, which the library's handler for it turns into a
        // `bye`.
        unsafe {
            libc::kill(libc::getpid(), libc::SIGINT);
        }
    }
    let until = Instant::now() + VIEW_GRACE;
    while !thread.is_finished() && Instant::now() < until {
        std::thread::sleep(Duration::from_millis(10));
    }
    if thread.is_finished() {
        let _ = thread.join();
    }
}

#[cfg(test)]
mod tests;
