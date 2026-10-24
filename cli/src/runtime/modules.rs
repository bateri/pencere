//! What the app's `require("pencere")` is made of in Rust: files, paths, the environment, JSON,
//! the clock, and the doors to the loop (timers, processes, the view).
//!
//! The standard library is small on purpose: what an app needs to show what is on the machine it
//! runs on. There is no network client in it (a program that wants one runs `curl`, and the user
//! sees which one). An app runs with the rights of the person who started it, as any program
//! does; nothing here is a sandbox, and the README says so.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use mlua::{Function, Lua, Table, UserData, UserDataMethods, Value, Variadic};

use super::convert::{from_json, object_marker, to_json};
use super::core::{Core, Exit};

fn fail(what: &str, path: impl AsRef<Path>, why: &io::Error) -> mlua::Error {
    super::fault(format!(
        "{what}: {}: {}",
        path.as_ref().display(),
        super::why(why)
    ))
}

fn seconds(value: f64, what: &str) -> mlua::Result<Duration> {
    Duration::try_from_secs_f64(value)
        .map_err(|_| super::fault(format!("{what}: {value} is not a number of seconds")))
}

/// The table the prelude is given: the functions it builds the module from.
pub fn natives(lua: &Lua, core: &Rc<Core>) -> mlua::Result<Table> {
    let native = lua.create_table()?;
    native.set("fs", files(lua)?)?;
    native.set("path", paths(lua)?)?;
    native.set("env", environment(lua)?)?;
    native.set("json", json(lua)?)?;
    native.set("hash", hash(lua)?)?;
    native.set("os", system(lua)?)?;
    native.set("io", terminal(lua)?)?;
    native.set("text", super::text::natives(lua)?)?;
    native.set("regex", super::pattern::natives(lua)?)?;
    native.set("args", core.args.clone())?;
    native.set("name", core.name.as_str())?;

    // ---- the app's own files and the ones it requires --------------------------------
    let hold = Rc::clone(core);
    native.set(
        "app_read",
        lua.create_function(move |lua, path: String| match hold.files.read(&path) {
            Ok(bytes) => Ok(Value::String(lua.create_string(bytes)?)),
            Err(_) => Ok(Value::Nil),
        })?,
    )?;
    let hold = Rc::clone(core);
    native.set(
        "app_has",
        lua.create_function(move |_, path: String| Ok(hold.files.has(&path)))?,
    )?;
    let hold = Rc::clone(core);
    native.set(
        "app_list",
        lua.create_function(move |_, directory: String| {
            hold.files
                .under(&directory)
                .map_err(|why| fail("app.list", &directory, &why))
        })?,
    )?;
    native.set(
        "load",
        lua.create_function(|lua, (source, name): (mlua::String, String)| {
            lua.load(source.as_bytes().to_vec())
                .set_name(format!("@{name}"))
                .into_function()
        })?,
    )?;

    // ---- words ----------------------------------------------------------------------
    let hold = Rc::clone(core);
    native.set(
        "print",
        lua.create_function(move |_, text: String| {
            hold.log.print(&text);
            Ok(())
        })?,
    )?;
    let hold = Rc::clone(core);
    native.set(
        "report",
        lua.create_function(move |_, text: String| {
            hold.log.error(&text);
            Ok(())
        })?,
    )?;
    // To the terminal, always: for what the person must read once the view is gone.
    native.set(
        "eprint",
        lua.create_function(|_, text: String| {
            eprintln!("{text}");
            Ok(())
        })?,
    )?;

    // ---- the end of the app ----------------------------------------------------------
    let hold = Rc::clone(core);
    native.set(
        "finish",
        lua.create_function(move |_, ()| {
            hold.end(Exit::Done(hold.status.get()));
            Ok(())
        })?,
    )?;
    let hold = Rc::clone(core);
    native.set(
        "exit",
        lua.create_function(move |_, code: Option<i32>| {
            hold.end(Exit::Done(code.unwrap_or(0)));
            Ok(())
        })?,
    )?;
    let hold = Rc::clone(core);
    native.set(
        "fail",
        lua.create_function(move |_, text: String| {
            hold.end(Exit::Failed(text));
            Ok(())
        })?,
    )?;
    let hold = Rc::clone(core);
    native.set(
        "set_task",
        lua.create_function(move |_, task: Function| {
            hold.set_task(task);
            Ok(())
        })?,
    )?;

    // ---- the clock and the timers ----------------------------------------------------
    native.set(
        "now",
        lua.create_function(|_, ()| {
            Ok(SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_or(0.0, |since| since.as_secs_f64()))
        })?,
    )?;
    let started = Instant::now();
    native.set(
        "monotonic",
        lua.create_function(move |_, ()| Ok(started.elapsed().as_secs_f64()))?,
    )?;
    let hold = Rc::clone(core);
    native.set(
        "after",
        lua.create_function(move |_, (after, func): (f64, Function)| {
            let after = seconds(after, "time.after")?;
            let id = hold.add_timer(after, None, func);
            Ok(TimerHandle {
                id,
                core: hold.weak(),
            })
        })?,
    )?;
    let hold = Rc::clone(core);
    native.set(
        "every",
        lua.create_function(move |_, (every, func): (f64, Function)| {
            let every = seconds(every, "time.every")?;
            if every.is_zero() {
                return Err(super::fault(
                    "time.every: the time between two runs is more than 0 seconds",
                ));
            }
            let id = hold.add_timer(every, Some(every), func);
            Ok(TimerHandle {
                id,
                core: hold.weak(),
            })
        })?,
    )?;

    // ---- files that change -----------------------------------------------------------
    let hold = Rc::clone(core);
    native.set(
        "watch",
        lua.create_function(
            move |_, (path, func, options): (String, Function, Option<Table>)| {
                let recursive = match &options {
                    Some(options) => options.get::<Option<bool>>("recursive")?.unwrap_or(true),
                    None => true,
                };
                let id = hold.id();
                let events = hold.events.clone();
                let watcher = super::watch::start(Path::new(&path), recursive, move |changes| {
                    events.send(super::events::Event::Watched { id, changes });
                })
                .map_err(|why| fail("fs.watch", &path, &why))?;
                hold.watches.borrow_mut().insert(id, (func, watcher));
                Ok(WatchHandle {
                    id,
                    core: hold.weak(),
                })
            },
        )?,
    )?;

    // ---- the terminal, given to a program --------------------------------------------
    let hold = Rc::clone(core);
    native.set(
        "handoff",
        lua.create_function(move |_, spec: Table| super::handoff::handoff(&hold, spec))?,
    )?;

    // ---- processes and the view ------------------------------------------------------
    let hold = Rc::clone(core);
    native.set(
        "spawn",
        lua.create_function(move |_, options: Table| super::process::spawn(&hold, options))?,
    )?;
    let hold = Rc::clone(core);
    native.set(
        "serve",
        lua.create_function(move |_, (options, done): (Table, Function)| {
            super::serve::start(&hold, options, done)
        })?,
    )?;
    Ok(native)
}

/// A directory the app watches.
struct WatchHandle {
    id: u64,
    core: std::rc::Weak<Core>,
}

impl UserData for WatchHandle {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        // watch:stop(): whether it was still watching.
        methods.add_method("stop", |_, this, ()| {
            let removed = this
                .core
                .upgrade()
                .and_then(|core| core.watches.borrow_mut().remove(&this.id));
            let was = removed.is_some();
            // Dropped (and its thread waited for) with no borrow of the loop's state held.
            drop(removed);
            Ok(was)
        });
    }
}

/// A timer the app started.
struct TimerHandle {
    id: u64,
    core: std::rc::Weak<Core>,
}

impl UserData for TimerHandle {
    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        // timer:cancel(): whether it was still waiting.
        methods.add_method("cancel", |_, this, ()| {
            Ok(this
                .core
                .upgrade()
                .is_some_and(|core| core.cancel_timer(this.id)))
        });
        methods.add_method("active", |_, this, ()| {
            Ok(this
                .core
                .upgrade()
                .is_some_and(|core| core.has_timer(this.id)))
        });
    }
}

// ---- fs ------------------------------------------------------------------------------------

fn kind_of(meta: &fs::Metadata) -> &'static str {
    if meta.is_dir() {
        "dir"
    } else if meta.is_file() {
        "file"
    } else {
        "other"
    }
}

fn seconds_since_epoch(time: io::Result<SystemTime>) -> f64 {
    time.ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map_or(0.0, |since| since.as_secs_f64())
}

fn files(lua: &Lua) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    table.set(
        "read",
        lua.create_function(|lua, path: String| {
            let bytes = fs::read(&path).map_err(|why| fail("fs.read", &path, &why))?;
            lua.create_string(bytes)
        })?,
    )?;
    table.set(
        "write",
        lua.create_function(|_, (path, data): (String, mlua::String)| {
            fs::write(&path, data.as_bytes()).map_err(|why| fail("fs.write", &path, &why))
        })?,
    )?;
    table.set(
        "append",
        lua.create_function(|_, (path, data): (String, mlua::String)| {
            use std::io::Write as _;
            fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&path)
                .and_then(|mut file| file.write_all(&data.as_bytes()))
                .map_err(|why| fail("fs.append", &path, &why))
        })?,
    )?;
    // fs.create(path): a new empty file, and an error if anything is at the path already (a file, a
    // directory, a link, even one that points at nothing). The path is made with `O_EXCL`, which
    // does not follow a link: an existing file is not truncated and a link's target is not made.
    table.set(
        "create",
        lua.create_function(|_, path: String| {
            fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
                .map(drop)
                .map_err(|why| fail("fs.create", &path, &why))
        })?,
    )?;
    // fs.rename(from, to): moves a file or a directory; a file at `to` is replaced.
    table.set(
        "rename",
        lua.create_function(|_, (from, to): (String, String)| {
            fs::rename(&from, &to).map_err(|why| fail("fs.rename", &from, &why))
        })?,
    )?;
    // fs.writable(path): whether this process may write to the file, as the system answers it
    // (the file's mode is not all that decides it).
    table.set(
        "writable",
        lua.create_function(|_, path: String| {
            let Ok(path) = std::ffi::CString::new(path) else {
                return Ok(false);
            };
            // SAFETY: asks the system about a path of ours.
            Ok(unsafe { libc::access(path.as_ptr(), libc::W_OK) } == 0)
        })?,
    )?;
    table.set(
        "list",
        lua.create_function(|lua, path: String| {
            let mut entries = Vec::new();
            for entry in fs::read_dir(&path).map_err(|why| fail("fs.list", &path, &why))? {
                let entry = entry.map_err(|why| fail("fs.list", &path, &why))?;
                let Ok(name) = entry.file_name().into_string() else {
                    continue;
                };
                let link = entry.file_type().is_ok_and(|kind| kind.is_symlink());
                let meta = fs::metadata(entry.path()).or_else(|_| entry.metadata());
                entries.push((name, link, meta.ok()));
            }
            entries.sort_by(|a, b| a.0.cmp(&b.0));
            let list = lua.create_table()?;
            for (index, (name, link, meta)) in entries.into_iter().enumerate() {
                let item = lua.create_table()?;
                item.set("name", name)?;
                // What a link points at, when it points at something (a link to nothing is
                // `other`); `link` says it is one.
                item.set("kind", meta.as_ref().map_or("other", kind_of))?;
                item.set("size", meta.as_ref().map_or(0, fs::Metadata::len))?;
                item.set("link", link)?;
                list.raw_set(index + 1, item)?;
            }
            Ok(list)
        })?,
    )?;
    table.set(
        "stat",
        lua.create_function(|lua, path: String| match fs::metadata(&path) {
            Ok(meta) => {
                let stat = lua.create_table()?;
                stat.set("kind", kind_of(&meta))?;
                stat.set("size", meta.len())?;
                stat.set("modified", seconds_since_epoch(meta.modified()))?;
                Ok(Value::Table(stat))
            }
            Err(why) if why.kind() == io::ErrorKind::NotFound => Ok(Value::Nil),
            Err(why) => Err(fail("fs.stat", &path, &why)),
        })?,
    )?;
    table.set(
        "exists",
        lua.create_function(|_, path: String| Ok(Path::new(&path).exists()))?,
    )?;
    // fs.mkdir(path): the directory and the ones above it that are not there.
    table.set(
        "mkdir",
        lua.create_function(|_, path: String| {
            fs::create_dir_all(&path).map_err(|why| fail("fs.mkdir", &path, &why))
        })?,
    )?;
    // fs.remove(path[, recursive]): a file, or a directory (with what is in it only if told).
    table.set(
        "remove",
        lua.create_function(|_, (path, recursive): (String, Option<bool>)| {
            let meta = fs::symlink_metadata(&path).map_err(|why| fail("fs.remove", &path, &why))?;
            let done = if meta.is_dir() {
                if recursive.unwrap_or(false) {
                    fs::remove_dir_all(&path)
                } else {
                    fs::remove_dir(&path)
                }
            } else {
                fs::remove_file(&path)
            };
            done.map_err(|why| fail("fs.remove", &path, &why))
        })?,
    )?;
    table.set(
        "cwd",
        lua.create_function(|_, ()| {
            std::env::current_dir()
                .map(|dir| dir.display().to_string())
                .map_err(|why| fail("fs.cwd", ".", &why))
        })?,
    )?;
    Ok(table)
}

// ---- path ----------------------------------------------------------------------------------

fn paths(lua: &Lua) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    table.set("separator", "/")?;
    // path.join("a", "b", "/c") -> "/c": a part that starts with a separator starts again.
    table.set(
        "join",
        lua.create_function(|_, parts: Variadic<String>| {
            let mut joined = PathBuf::new();
            for part in parts.iter() {
                joined.push(part);
            }
            Ok(joined.display().to_string())
        })?,
    )?;
    table.set(
        "dirname",
        lua.create_function(|_, path: String| {
            Ok(match Path::new(&path).parent() {
                Some(parent) if parent.as_os_str().is_empty() => ".".to_owned(),
                Some(parent) => parent.display().to_string(),
                None => path,
            })
        })?,
    )?;
    table.set(
        "basename",
        lua.create_function(|_, path: String| {
            Ok(Path::new(&path)
                .file_name()
                .map_or_else(String::new, |name| name.to_string_lossy().into_owned()))
        })?,
    )?;
    // path.extension("a/b.tar.gz") -> "gz"; "" when it has none.
    table.set(
        "extension",
        lua.create_function(|_, path: String| {
            Ok(Path::new(&path)
                .extension()
                .map_or_else(String::new, |ext| ext.to_string_lossy().into_owned()))
        })?,
    )?;
    table.set(
        "absolute",
        lua.create_function(|_, path: String| {
            std::path::absolute(&path)
                .map(|path| path.display().to_string())
                .map_err(|why| fail("path.absolute", &path, &why))
        })?,
    )?;
    Ok(table)
}

// ---- env -----------------------------------------------------------------------------------

fn environment(lua: &Lua) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    table.set(
        "get",
        lua.create_function(|_, name: String| Ok(std::env::var(name).ok()))?,
    )?;
    table.set(
        "all",
        lua.create_function(|lua, ()| {
            let all = lua.create_table()?;
            for (name, value) in std::env::vars() {
                all.set(name, value)?;
            }
            Ok(all)
        })?,
    )?;
    Ok(table)
}

// ---- json ----------------------------------------------------------------------------------

fn json(lua: &Lua) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    // json.encode(value[, pretty]) -> text
    table.set(
        "encode",
        lua.create_function(|lua, (value, pretty): (Value, Option<bool>)| {
            let json =
                to_json(lua, &value).map_err(|why| super::fault(format!("json.encode: {why}")))?;
            let text = if pretty.unwrap_or(false) {
                serde_json::to_string_pretty(&json)
            } else {
                serde_json::to_string(&json)
            };
            text.map_err(|why| super::fault(format!("json.encode: {why}")))
        })?,
    )?;
    // json.decode(text) -> value; a `null` is nil.
    table.set(
        "decode",
        lua.create_function(|lua, text: mlua::String| {
            let json: serde_json::Value = serde_json::from_slice(&text.as_bytes())
                .map_err(|why| super::fault(format!("json.decode: {why}")))?;
            from_json(lua, &json)
        })?,
    )?;
    // json.null: what is sent as `null` (a field that is nil is not sent at all).
    table.set("null", Value::NULL)?;
    // json.object() -> an empty table that is sent as `{}` and not as `[]`.
    table.set(
        "object",
        lua.create_function(|lua, ()| {
            let object = lua.create_table()?;
            object.set_metatable(Some(object_marker(lua)?))?;
            Ok(object)
        })?,
    )?;
    Ok(table)
}

// ---- hash ----------------------------------------------------------------------------------

fn hash(lua: &Lua) -> mlua::Result<Table> {
    use sha2::{Digest as _, Sha256};
    let table = lua.create_table()?;
    // hash.sha256(data) -> 64 lowercase hex digits
    table.set(
        "sha256",
        lua.create_function(|_, data: mlua::String| {
            Ok(Sha256::digest(&*data.as_bytes())
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>())
        })?,
    )?;
    Ok(table)
}

// ---- os ------------------------------------------------------------------------------------

fn system(lua: &Lua) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    table.set("platform", std::env::consts::OS)?;
    table.set("arch", std::env::consts::ARCH)?;
    // The person's home directory, or nil where there is none to be found.
    table.set(
        "home",
        lua.create_function(|_, ()| {
            Ok(std::env::var("HOME").ok().filter(|home| !home.is_empty()))
        })?,
    )?;
    table.set(
        "tmpdir",
        lua.create_function(|_, ()| Ok(std::env::temp_dir().display().to_string()))?,
    )?;
    // The machine's name, as it calls itself ("" where it cannot be found).
    table.set(
        "hostname",
        lua.create_function(|_, ()| {
            let mut name = [0u8; 256];
            // SAFETY: fills a buffer of ours, with room for the end of the text.
            let got = unsafe { libc::gethostname(name.as_mut_ptr().cast(), name.len() - 1) };
            if got != 0 {
                return Ok(String::new());
            }
            let end = name
                .iter()
                .position(|byte| *byte == 0)
                .unwrap_or(name.len());
            Ok(String::from_utf8_lossy(name.get(..end).unwrap_or_default()).into_owned())
        })?,
    )?;
    // The name of the person who runs the app, from the system's records of users ("" where the
    // user has no entry).
    table.set(
        "username",
        lua.create_function(|_, ()| {
            let mut buffer = vec![0u8; 1024];
            // SAFETY: the record is filled into buffers of ours, which outlive its use below.
            let name = unsafe {
                let mut record: libc::passwd = std::mem::zeroed();
                let mut found: *mut libc::passwd = std::ptr::null_mut();
                let status = libc::getpwuid_r(
                    libc::geteuid(),
                    &raw mut record,
                    buffer.as_mut_ptr().cast(),
                    buffer.len(),
                    &raw mut found,
                );
                if status != 0 || found.is_null() || record.pw_name.is_null() {
                    String::new()
                } else {
                    std::ffi::CStr::from_ptr(record.pw_name)
                        .to_string_lossy()
                        .into_owned()
                }
            };
            Ok(name)
        })?,
    )?;
    Ok(table)
}

// ---- io ------------------------------------------------------------------------------------

/// The app's text interface: for a terminal that shows no view, and a program that asks the person
/// something. It is the terminal's, whatever `--log` does with `print`.
fn terminal(lua: &Lua) -> mlua::Result<Table> {
    let table = lua.create_table()?;
    // io.write(text): to the terminal, as it is (no newline after it).
    table.set(
        "write",
        lua.create_function(|_, text: mlua::String| {
            use std::io::Write as _;
            let mut out = io::stdout().lock();
            out.write_all(&text.as_bytes())
                .and_then(|()| out.flush())
                .map_err(|why| fail("io.write", "stdout", &why))
        })?,
    )?;
    // io.ewrite(text): to the terminal's error output, as it is: what a program says of its own
    // failure, apart from what it prints for a person or a pipe to read.
    table.set(
        "ewrite",
        lua.create_function(|_, text: mlua::String| {
            use std::io::Write as _;
            let mut out = io::stderr().lock();
            out.write_all(&text.as_bytes())
                .and_then(|()| out.flush())
                .map_err(|why| fail("io.ewrite", "stderr", &why))
        })?,
    )?;
    // io.read_line([prompt]) -> the line without its end, or nil at the end of the input.
    table.set(
        "read_line",
        lua.create_function(|lua, prompt: Option<mlua::String>| {
            use std::io::Write as _;
            if let Some(prompt) = prompt {
                let mut out = io::stdout().lock();
                let _ = out.write_all(&prompt.as_bytes());
                let _ = out.flush();
            }
            let mut line = Vec::new();
            let got = io::BufRead::read_until(&mut io::stdin().lock(), b'\n', &mut line)
                .map_err(|why| fail("io.read_line", "stdin", &why))?;
            if got == 0 {
                return Ok(Value::Nil);
            }
            while matches!(line.last(), Some(b'\n' | b'\r')) {
                line.pop();
            }
            Ok(Value::String(lua.create_string(line)?))
        })?,
    )?;
    Ok(table)
}
