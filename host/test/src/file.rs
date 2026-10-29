//! One test file, run: the runtime of the command with the app's program modules and the file's
//! tests in it, and `pencere.test` made for it.

use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{Duration, Instant};

use pencere_cli::mlua::{self, Lua, Table, Value, VmState};
use pencere_cli::{Config, Env, Exit, Files, Module, Project, Runner, from_json, to_json};
use pencere_drive::headless::{Calls, Failure, Headless, Opening};
use serde_json::{Value as Json, json};

use crate::page::{Handle, Open, Remake};
use crate::peer::Any;
use crate::program::{App, Setting};

/// The Lua of the module, which the natives below serve.
const API: &str = include_str!("api.lua");

/// What the coordinator asks of a worker: one file, run.
#[derive(Clone, Debug)]
pub struct Job {
    /// The file as the report names it.
    pub label: String,
    /// The file, from the app's directory.
    pub path: PathBuf,
    /// The app's directory.
    pub root: PathBuf,
    pub grep: Option<String>,
    pub line: Option<u32>,
    /// Seconds a test has, unless it says otherwise.
    pub timeout: f64,
    pub ci: bool,
    pub artifacts: PathBuf,
}

impl Job {
    pub fn to_json(&self) -> Json {
        json!({
            "label": self.label,
            "path": self.path,
            "root": self.root,
            "grep": self.grep,
            "line": self.line,
            "timeout": self.timeout,
            "ci": self.ci,
            "artifacts": self.artifacts,
        })
    }

    pub fn from_json(value: &Json) -> Option<Self> {
        Some(Self {
            label: value.get("label")?.as_str()?.to_owned(),
            path: PathBuf::from(value.get("path")?.as_str()?),
            root: PathBuf::from(value.get("root")?.as_str()?),
            grep: value.get("grep").and_then(Json::as_str).map(str::to_owned),
            line: value
                .get("line")
                .and_then(Json::as_u64)
                .and_then(|line| u32::try_from(line).ok()),
            timeout: value.get("timeout")?.as_f64()?,
            ci: value.get("ci")?.as_bool()?,
            artifacts: PathBuf::from(value.get("artifacts")?.as_str()?),
        })
    }
}

/// What is going on in the test that is running.
struct Current {
    name: String,
    line: u32,
    began: Instant,
    /// Where in the log the test's words begin.
    log_from: u64,
    deadline: Instant,
}

struct Session {
    job: Job,
    emit: Rc<dyn Fn(Json)>,
    current: Option<Current>,
    pages: Vec<Rc<RefCell<Open>>>,
    temps: Vec<PathBuf>,
    log: PathBuf,
    program_logs: u32,
    /// The test's own deadline, which the interpreter looks at as it runs.
    deadline: Rc<Cell<Option<Instant>>>,
}

type Shared = Rc<RefCell<Session>>;

/// What became of the file.
#[derive(Debug)]
pub enum Ended {
    /// Every test that was to run ran (each told its own way).
    Ran,
    /// The file did not load, or something kept it from running.
    Failed(String),
}

fn words(text: impl Into<String>) -> mlua::Error {
    mlua::Error::runtime(text.into())
}

/// A name that is safe as a directory name.
pub fn slug(name: &str) -> String {
    let mut slug: String = name
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect();
    slug.truncate(80);
    slug
}

/// The module name of a test file: its path under `test/`, with dots.
fn module_of(root: &Path, path: &Path) -> String {
    let tests = root.join("test");
    let relative = path
        .strip_prefix(&tests)
        .or_else(|_| path.strip_prefix(root))
        .unwrap_or(path);
    relative
        .with_extension("")
        .to_string_lossy()
        .replace('/', ".")
}

/// The files the test file's runtime has: the program's modules, the app's `test/` over them,
/// and the entry that runs the file.
fn files_of(job: &Job) -> Result<Files, String> {
    let mut files = BTreeMap::new();
    // The program's modules, if the program is in Lua: a program in another language has none.
    if let Ok(project) = Project::find(&job.root, None) {
        let app = project.files();
        for path in app.paths().map_err(|error| error.to_string())? {
            // The pages and their pictures are the view's, not the program's: a test reaches the
            // page through the app's view.
            if path.starts_with("view/") || path.starts_with("assets/") {
                continue;
            }
            if path.ends_with(".lua") || path.ends_with(".luau") {
                let bytes = app
                    .read(&path)
                    .map_err(|error| format!("{path}: {error}"))?;
                files.insert(path, bytes);
            }
        }
    }
    // The program's own entry is not a module: the entry here runs the tests.
    files.remove("app.lua");
    let tests = job.root.join("test");
    collect(&tests, &tests, &mut files)?;
    let module = module_of(&job.root, &job.path);
    // A file given by its path from elsewhere is the module under its own name.
    if let std::collections::btree_map::Entry::Vacant(slot) =
        files.entry(format!("{}.lua", module.replace('.', "/")))
    {
        let bytes =
            std::fs::read(&job.path).map_err(|error| format!("{}: {error}", job.path.display()))?;
        slot.insert(bytes);
    }
    let entry =
        format!("local test = require(\"pencere.test\")\nrequire({module:?})\ntest.__run()\n");
    files.insert("app.lua".to_owned(), entry.into_bytes());
    Ok(Files::packed(files))
}

fn collect(
    base: &Path,
    directory: &Path,
    files: &mut BTreeMap<String, Vec<u8>>,
) -> Result<(), String> {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return Ok(());
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if name.starts_with('.') {
            continue;
        }
        // A link to a directory is not followed: it may lead back to where it is.
        if std::fs::symlink_metadata(&path).is_ok_and(|meta| meta.is_dir()) {
            collect(base, &path, files)?;
        } else if name.ends_with(".lua") || name.ends_with(".luau") {
            let relative = path
                .strip_prefix(base)
                .map_err(|error| error.to_string())?
                .to_string_lossy()
                .into_owned();
            let bytes =
                std::fs::read(&path).map_err(|error| format!("{}: {error}", path.display()))?;
            files.insert(relative, bytes);
        }
    }
    Ok(())
}

/// Runs the file of `job`; each test tells how it went through `emit`.
pub fn run(job: Job, emit: Rc<dyn Fn(Json)>) -> Ended {
    let files = match files_of(&job) {
        Ok(files) => files,
        Err(why) => return Ended::Failed(why),
    };
    let log = std::env::temp_dir().join(format!(
        "pencere-test-{}-{}.log",
        std::process::id(),
        slug(&job.label)
    ));
    let _ = std::fs::remove_file(&log);
    let env = match Env::of_process() {
        Ok(env) => env,
        Err(error) => return Ended::Failed(error.to_string()),
    };
    let deadline = Rc::new(Cell::new(None));
    let session: Shared = Rc::new(RefCell::new(Session {
        job: job.clone(),
        emit,
        current: None,
        pages: Vec::new(),
        temps: Vec::new(),
        log: log.clone(),
        program_logs: 0,
        deadline: Rc::clone(&deadline),
    }));
    let module = {
        let session = Rc::clone(&session);
        Module {
            name: "pencere.test".to_owned(),
            load: Box::new(move |lua| load(lua, &session)),
        }
    };
    let mut config = Config::process(files, slug(&job.label), Vec::new(), Some(log.clone()));
    // The test file is not the process either: the process's variables and directory are what the
    // programs it starts find, with the app's directory the place it is in.
    config.env = Some(Env {
        cwd: job.root.clone(),
        ..env
    });
    config.modules.push(module);
    let runner = match Runner::new(config) {
        Ok(runner) => runner,
        Err(error) => return Ended::Failed(error.to_string()),
    };
    let exit = runner.run();
    // Whatever the file left open.
    cleanup(&session);
    let _ = std::fs::remove_file(&log);
    match exit {
        Exit::Done(0) => Ended::Ran,
        Exit::Done(status) => Ended::Failed(format!("the file ended with the status {status}")),
        Exit::Failed(why) => Ended::Failed(why),
        Exit::Restart => Ended::Failed("the file was asked to start again".to_owned()),
    }
}

/// Lets go of every page the test opened and the folders it made.
fn cleanup(session: &Shared) {
    let (pages, temps) = {
        let mut session = session.borrow_mut();
        (
            std::mem::take(&mut session.pages),
            std::mem::take(&mut session.temps),
        )
    };
    for page in pages {
        if let Ok(mut open) = page.try_borrow_mut() {
            open.close();
        }
    }
    for directory in temps {
        let _ = std::fs::remove_dir_all(directory);
    }
}

fn load(lua: &Lua, session: &Shared) -> mlua::Result<Value> {
    // The interpreter looks at the test's deadline as it runs: a loop that never ends is cut off
    // in words, and the test fails as any other does.
    {
        let deadline = Rc::clone(&session.borrow().deadline);
        lua.set_interrupt(move |_| {
            if deadline.get().is_some_and(|by| Instant::now() > by) {
                deadline.set(None);
                return Err(words(
                    "the test ran out of time: it did not end within its timeout",
                ));
            }
            Ok(VmState::Continue)
        });
    }
    let native = lua.create_table()?;
    native.set(
        "config",
        lua.create_function({
            let session = Rc::clone(session);
            move |lua, ()| {
                let session = session.borrow();
                let table = lua.create_table()?;
                table.set("grep", session.job.grep.as_deref())?;
                table.set("line", session.job.line)?;
                table.set("ci", session.job.ci)?;
                Ok(table)
            }
        })?,
    )?;
    native.set(
        "clock",
        lua.create_function(|_, ()| {
            static START: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();
            Ok(START.get_or_init(Instant::now).elapsed().as_secs_f64())
        })?,
    )?;
    native.set(
        "temp_dir",
        lua.create_function({
            let session = Rc::clone(session);
            move |_, ()| {
                let mut session = session.borrow_mut();
                let directory = std::env::temp_dir().join(format!(
                    "pencere-test-dir-{}-{}",
                    std::process::id(),
                    session.temps.len()
                ));
                let _ = std::fs::remove_dir_all(&directory);
                std::fs::create_dir_all(&directory)
                    .map_err(|error| words(format!("temp_dir: {error}")))?;
                let shown = directory.display().to_string();
                session.temps.push(directory);
                Ok(shown)
            }
        })?,
    )?;
    native.set(
        "begin",
        lua.create_function({
            let session = Rc::clone(session);
            move |_, (name, line, timeout): (String, u32, Option<f64>)| {
                let mut session = session.borrow_mut();
                let timeout = timeout.unwrap_or(session.job.timeout);
                let now = Instant::now();
                let by =
                    now + Duration::try_from_secs_f64(timeout).unwrap_or(Duration::from_secs(30));
                let log_from = std::fs::metadata(&session.log).map_or(0, |meta| meta.len());
                (session.emit)(
                    json!({ "ev": "start", "name": name, "line": line, "timeout": timeout }),
                );
                session.deadline.set(Some(by));
                session.current = Some(Current {
                    name,
                    line,
                    began: now,
                    log_from,
                    deadline: by,
                });
                Ok(())
            }
        })?,
    )?;
    native.set(
        "finished",
        lua.create_function({
            let session = Rc::clone(session);
            move |_, (ok, message, trace): (bool, Option<String>, Option<String>)| {
                finished(&session, ok, message, trace);
                Ok(())
            }
        })?,
    )?;
    native.set(
        "plan",
        lua.create_function({
            let session = Rc::clone(session);
            move |_, names: Vec<String>| {
                (session.borrow().emit)(json!({ "ev": "plan", "tests": names }));
                Ok(())
            }
        })?,
    )?;
    native.set(
        "skipped",
        lua.create_function({
            let session = Rc::clone(session);
            move |_, (name, line): (String, u32)| {
                (session.borrow().emit)(json!({
                    "ev": "test", "name": name, "line": line, "status": "skip", "ms": 0,
                }));
                Ok(())
            }
        })?,
    )?;
    native.set(
        "file_failed",
        lua.create_function({
            let session = Rc::clone(session);
            move |_, message: String| {
                (session.borrow().emit)(json!({ "ev": "file_error", "message": message }));
                Ok(())
            }
        })?,
    )?;
    native.set(
        "open",
        lua.create_function({
            let session = Rc::clone(session);
            move |lua, options: Table| open(lua, &session, &options)
        })?,
    )?;
    let module: Value = lua.load(API).set_name("=pencere.test").call(native)?;
    Ok(module)
}

/// A test ended: what became of it is told, with what a failure leaves for the eye.
fn finished(session: &Shared, ok: bool, message: Option<String>, trace: Option<String>) {
    let current = {
        let mut session = session.borrow_mut();
        session.deadline.set(None);
        session.current.take()
    };
    let Some(current) = current else {
        return;
    };
    let (emit, label, artifacts, log, pages) = {
        let session = session.borrow();
        (
            Rc::clone(&session.emit),
            session.job.label.clone(),
            session.job.artifacts.clone(),
            session.log.clone(),
            session.pages.clone(),
        )
    };
    let mut kept = Vec::new();
    let mut said = Vec::new();
    let mut page_words = Vec::new();
    if !ok {
        let directory = artifacts.join(slug(&label)).join(slug(&current.name));
        let _ = std::fs::create_dir_all(&directory);
        for (index, page) in pages.iter().enumerate() {
            let Ok(open) = page.try_borrow() else {
                continue;
            };
            let Some(headless) = open.page.as_ref() else {
                continue;
            };
            let number = index + 1;
            if let Ok(png) = headless.shot(&pencere_drive::shot::Fonts::none()) {
                let path = directory.join(format!("page-{number}.png"));
                if std::fs::write(&path, png).is_ok() {
                    kept.push(path.display().to_string());
                }
            }
            let mut text = String::new();
            for word in headless.words() {
                text.push_str(word);
                text.push('\n');
            }
            text.push_str("\n-- what a screen reader is told --\n");
            for item in &headless.outline().accessible {
                text.push_str(&format!("{:?} {:?}\n", item.role, item.words));
            }
            let path = directory.join(format!("page-{number}.words.txt"));
            if std::fs::write(&path, text).is_ok() {
                kept.push(path.display().to_string());
            }
            let mut messages = String::new();
            for sent in headless.sent() {
                messages.push_str(&format!("{{\"to\":\"program\",\"message\":{sent}}}\n"));
            }
            for received in headless.received() {
                messages.push_str(&format!("{{\"to\":\"page\",\"message\":{received}}}\n"));
            }
            let path = directory.join(format!("page-{number}.messages.jsonl"));
            if std::fs::write(&path, messages).is_ok() {
                kept.push(path.display().to_string());
            }
            if let Some(file) = &open.log
                && let Ok(text) = std::fs::read_to_string(file)
                && !text.is_empty()
            {
                let path = directory.join(format!("page-{number}.program.log"));
                if std::fs::write(&path, text).is_ok() {
                    kept.push(path.display().to_string());
                }
            }
            if page_words.is_empty() {
                page_words = headless
                    .words()
                    .into_iter()
                    .map(str::to_owned)
                    .take(40)
                    .collect();
            }
        }
    }
    // What the test printed.
    if let Ok(mut file) = std::fs::File::open(&log) {
        use std::io::{Read as _, Seek as _, SeekFrom};
        let mut text = String::new();
        if file.seek(SeekFrom::Start(current.log_from)).is_ok()
            && file.read_to_string(&mut text).is_ok()
        {
            said.push(text);
        }
    }
    cleanup(session);
    let timed_out = !ok
        && message
            .as_deref()
            .is_some_and(|text| text.contains("ran out of time"));
    emit(json!({
        "ev": "test",
        "name": current.name,
        "line": current.line,
        "status": if ok { "pass" } else if timed_out { "timeout" } else { "fail" },
        "ms": u64::try_from(current.began.elapsed().as_millis()).unwrap_or(u64::MAX),
        "message": message,
        "trace": trace,
        "words": page_words,
        "artifacts": kept,
        "output": said.concat(),
        "deadline": u64::try_from(current.deadline.saturating_duration_since(current.began).as_millis()).unwrap_or(0),
    }));
}

// ---- opening a page ------------------------------------------------------------------------

/// The program an app has, which is what a page with no program named speaks to.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Program {
    /// Lua: run in this process, by the runtime of the command.
    Lua,
    /// A Cargo package: built once, and run on a pty.
    Package,
    /// Another language: it is named (`command`), or the page has a stand-in.
    Other,
}

/// Where the files of the page are.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Page {
    /// `src/view/` with `src/assets/`, as `pencere new` lays an app out.
    Laid,
    /// `src/view/` alone, the pictures in it (a TypeScript project's).
    View,
    /// The directory itself: the simple form, with `page.lua` beside `app.lua`.
    Simple,
}

/// The app the test file is in, as the page and the program of it are found.
struct Where {
    root: PathBuf,
    page: Page,
    program: Program,
}

fn app_where(root: &Path) -> Where {
    let program = if Project::find(root, None).is_ok() {
        Program::Lua
    } else if pencere_cli::is_package(root) {
        Program::Package
    } else {
        Program::Other
    };
    let page = if root.join("src/view/page.lua").is_file() {
        if root.join("src/assets").is_dir() {
            Page::Laid
        } else {
            Page::View
        }
    } else {
        Page::Simple
    };
    Where {
        root: root.to_path_buf(),
        page,
        program,
    }
}

/// The program of a Cargo package, built (once for each package in a process) with Cargo.
fn built(root: &Path) -> Result<PathBuf, String> {
    static BUILT: std::sync::Mutex<BTreeMap<PathBuf, PathBuf>> =
        std::sync::Mutex::new(BTreeMap::new());
    let mut built = BUILT
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    if let Some(program) = built.get(root) {
        return Ok(program.clone());
    }
    let name = root
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("app");
    let program = pencere_cli::build_package(root, name, &["--quiet"])?;
    built.insert(root.to_path_buf(), program.clone());
    Ok(program)
}

/// What a page is opened with, from the options of `test.open`.
struct Spec {
    entry: String,
    script: Option<String>,
    component: Option<PathBuf>,
    size: (f32, f32),
    scale: f64,
    reduce_motion: bool,
    at: f64,
    patience: Duration,
    place: Where,
}

fn number(options: &Table, name: &str) -> mlua::Result<Option<f64>> {
    options.get::<Option<f64>>(name)
}

fn spec_of(session: &Shared, options: &Table) -> mlua::Result<Spec> {
    let (root, remaining) = {
        let session = session.borrow();
        let remaining = session
            .current
            .as_ref()
            .map_or(Duration::from_secs(60), |current| {
                current.deadline.saturating_duration_since(Instant::now())
            });
        (session.job.root.clone(), remaining)
    };
    let place = app_where(&root);
    let mut size = (800.0, 600.0);
    if let Some(list) = options.get::<Option<Table>>("size")? {
        size = (
            list.get::<f32>(1)
                .map_err(|_| words("size is { width, height }"))?,
            list.get::<f32>(2)
                .map_err(|_| words("size is { width, height }"))?,
        );
    }
    if let Some(fonts) = options.get::<Option<Value>>("fonts")? {
        let fixed = matches!(&fonts, Value::String(text) if text.to_str().is_ok_and(|text| &*text == "fixed"));
        if !fixed && !matches!(fonts, Value::Nil) {
            return Err(words(
                "fonts: only \"fixed\" is supported (every character a box of the same size, the same on every machine)",
            ));
        }
    }
    Ok(Spec {
        entry: options.get::<Option<String>>("entry")?.unwrap_or_else(|| {
            // The simple form has its page beside `app.lua`, or in `view/`.
            if place.page == Page::Simple
                && !root.join("page.lua").is_file()
                && root.join("view/page.lua").is_file()
            {
                "view/page.lua".to_owned()
            } else {
                "page.lua".to_owned()
            }
        }),
        script: options.get::<Option<String>>("script")?,
        component: options
            .get::<Option<String>>("component")?
            .map(|path| root.join(path)),
        size,
        scale: number(options, "scale")?.unwrap_or(1.0),
        reduce_motion: options
            .get::<Option<bool>>("reduce_motion")?
            .unwrap_or(false),
        at: number(options, "at")?.unwrap_or(0.0),
        patience: remaining.max(Duration::from_secs(1)),
        place,
    })
}

impl Spec {
    /// The page of the app, with the host's settings, and a table of calls for a program.
    fn opening(&self) -> Result<Opening<Calls>, String> {
        let opening = if let Some(source) = &self.script {
            Headless::script(source.clone())
        } else if let Some(path) = &self.component {
            let bytes =
                std::fs::read(path).map_err(|error| format!("{}: {error}", path.display()))?;
            Headless::component(bytes)
        } else {
            match self.place.page {
                Page::Laid => Headless::bundle(
                    pencere_drive::headless::Files::app(&self.place.root),
                    &self.entry,
                ),
                Page::View => Headless::bundle(
                    pencere_drive::headless::Files::dir(self.place.root.join("src/view")),
                    &self.entry,
                ),
                Page::Simple => Headless::bundle(
                    pencere_drive::headless::Files::dir(&self.place.root),
                    &self.entry,
                ),
            }
        };
        Ok(opening
            .size(self.size.0, self.size.1)
            .scale(self.scale)
            .reduce_motion(self.reduce_motion)
            .at(self.at)
            .patience(self.patience))
    }
}

/// The stand-in program of `calls`: the handlers a test wrote, each wrapped in Lua to give
/// `(ok, result_or_failure)`.
fn stand_in(lua: &Lua, table: &Table) -> mlua::Result<Calls> {
    let mut calls = Calls::new();
    for pair in table.pairs::<String, mlua::Function>() {
        let (name, handler) = pair?;
        let lua = lua.clone();
        calls = calls.on(&name, move |params| {
            let answer = from_json(&lua, params)
                .and_then(|params| handler.call::<(bool, Value)>(params))
                .map_err(|error| Failure {
                    code: "internal".to_owned(),
                    message: error.to_string(),
                })?;
            match answer {
                (true, value) => to_json(&lua, &value).map_err(|why| Failure {
                    code: "internal".to_owned(),
                    message: why,
                }),
                (false, Value::Table(table)) => Err(Failure {
                    code: table
                        .get::<Option<String>>("code")
                        .ok()
                        .flatten()
                        .unwrap_or_else(|| "internal".to_owned()),
                    message: table
                        .get::<Option<String>>("message")
                        .ok()
                        .flatten()
                        .unwrap_or_default(),
                }),
                (false, other) => Err(Failure {
                    code: "internal".to_owned(),
                    message: format!("{other:?}"),
                }),
            }
        });
    }
    Ok(calls)
}

fn strings(table: &Table, name: &str) -> mlua::Result<Vec<String>> {
    Ok(table.get::<Option<Vec<String>>>(name)?.unwrap_or_default())
}

#[allow(clippy::too_many_lines)]
fn open(lua: &Lua, session: &Shared, options: &Table) -> mlua::Result<Handle> {
    let spec = spec_of(session, options)?;
    let (artifacts, label, test_name) = {
        let session = session.borrow();
        (
            session.job.artifacts.clone(),
            session.job.label.clone(),
            session
                .current
                .as_ref()
                .map_or_else(|| "file".to_owned(), |current| current.name.clone()),
        )
    };
    let shots_dir = artifacts.join(slug(&label)).join(slug(&test_name));
    let shots: Rc<dyn Fn(&str) -> PathBuf> = Rc::new(move |name| {
        let mut file = slug(name);
        file.push_str(".png");
        shots_dir.join(file)
    });
    let calls = options.get::<Option<Table>>("calls")?;
    let command = options.get::<Option<Vec<String>>>("command")?;
    let root = spec.place.root.clone();
    let env: BTreeMap<String, String> = options
        .get::<Option<BTreeMap<String, String>>>("env")?
        .unwrap_or_default();
    let cwd = options.get::<Option<String>>("cwd")?.map(|path| {
        let path = PathBuf::from(path);
        if path.is_absolute() {
            path
        } else {
            root.join(path)
        }
    });
    let args = strings(options, "args")?;
    let spec = Rc::new(spec);
    // The page can be opened again with its program only where the program is this process's: a
    // stand-in, or the app's own.
    let mut remake: Option<Remake> = {
        let spec = Rc::clone(&spec);
        Some(Box::new(move || spec.opening()))
    };
    let mut log = None;
    let mut app = None;
    let page = if let Some(calls) = calls {
        let peer = Any::Calls(stand_in(lua, &calls)?);
        spec.opening()
            .map_err(words)?
            .peer(peer)
            .open()
            .map_err(|error| words(error.to_string()))?
    } else if let Some(command) = command {
        remake = None;
        start_command(&command, &args, &env, cwd.as_deref(), &root, &spec)?
    } else if spec.place.program == Program::Package {
        // A program in Rust: built once, and run on a pty as a terminal runs it.
        remake = None;
        let program = built(&root).map_err(words)?;
        let mut command = vec![program.display().to_string()];
        command.extend(args.iter().cloned());
        start_command(&command, &[], &env, cwd.as_deref(), &root, &spec)?
    } else if spec.place.program == Program::Other {
        return Err(words(
            "the app's program is not in Lua or a Cargo package, so it is not run for you: open the page with `calls` (stand-in handlers) or name the program with `command = { \"deno\", \"run\", \"-A\", \"src/program/main.ts\" }`",
        ));
    } else {
        let number = {
            let mut session = session.borrow_mut();
            session.program_logs += 1;
            session.program_logs
        };
        let file = std::env::temp_dir().join(format!(
            "pencere-test-program-{}-{number}.log",
            std::process::id()
        ));
        let _ = std::fs::remove_file(&file);
        let started = App::start(
            Setting {
                root: root.clone(),
                args: args.clone(),
                env: env.clone(),
                cwd: cwd.clone(),
                log: file.clone(),
            },
            spec.patience.min(Duration::from_secs(30)),
        )
        .map_err(words)?;
        log = Some(file);
        let (running, program) = started;
        app = Some(running);
        let opened = spec
            .opening()
            .map_err(words)?
            .peer(Any::InProcess(Box::new(program)))
            .open();
        match opened {
            Ok(page) => page,
            Err(error) => {
                if let Some(running) = app.take() {
                    running.stop();
                }
                return Err(words(error.to_string()));
            }
        }
    };
    let open = Rc::new(RefCell::new(Open {
        page: Some(page),
        app,
        remake,
        log,
        shots,
    }));
    session.borrow_mut().pages.push(Rc::clone(&open));
    Ok(Handle(open))
}

/// A program on a pty, and the page it opens.
fn start_command(
    command: &[String],
    args: &[String],
    env: &BTreeMap<String, String>,
    cwd: Option<&Path>,
    root: &Path,
    spec: &Spec,
) -> mlua::Result<Headless<Any>> {
    use pencere_fake_host::program::{Program, view_of};
    let mut parts: Vec<String> = command.to_vec();
    parts.extend(args.iter().cloned());
    let parts: Vec<&str> = parts.iter().map(String::as_str).collect();
    let environment: Vec<(String, String)> = env
        .iter()
        .map(|(name, value)| (name.clone(), value.clone()))
        .collect();
    let mut program = Program::start_in(&parts, &environment, Some(cwd.unwrap_or(root)))
        .map_err(|error| words(format!("{}: {error}", command.join(" "))))?;
    let opening =
        view_of(&mut program, Duration::from_secs(60)).map_err(|error| words(error.to_string()))?;
    opening
        .size(spec.size.0, spec.size.1)
        .scale(spec.scale)
        .reduce_motion(spec.reduce_motion)
        .at(spec.at)
        .patience(spec.patience)
        .peer(Any::Program(Box::new(program)))
        .open()
        .map_err(|error| words(error.to_string()))
}
