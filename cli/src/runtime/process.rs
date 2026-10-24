//! `process.spawn`: a program the app starts, whose output comes to it line by line.

use std::collections::HashMap;
use std::os::fd::OwnedFd;
use std::process::{Command, Stdio};
use std::rc::Weak;

use mlua::{Function, Table, UserData, UserDataFields, UserDataMethods};

use super::core::{Child, Core, Stream};

fn nonblocking(fd: &OwnedFd) {
    use std::os::fd::AsRawFd as _;
    // SAFETY: flags of a descriptor of ours.
    unsafe {
        let flags = libc::fcntl(fd.as_raw_fd(), libc::F_GETFL);
        libc::fcntl(fd.as_raw_fd(), libc::F_SETFL, flags | libc::O_NONBLOCK);
    }
}

/// `spawn{ argv = {"docker", "ps"}, cwd =, env =, input =, on_stdout =, on_stderr =, on_exit = }`.
///
/// `argv` is a list: no shell reads it, so nothing in it is a command to be taken apart. The
/// environment is the app's, with `env` added to it. The process reads nothing from the terminal:
/// what `input` holds (a string) is what it is given on its standard input, and then the end.
pub fn spawn(core: &Core, options: Table) -> mlua::Result<ChildHandle> {
    let argv: Vec<String> = options
        .get::<Option<Vec<String>>>("argv")?
        .unwrap_or_default();
    let Some((program, rest)) = argv.split_first() else {
        return Err(super::fault(
            "process.spawn needs `argv`, a list: the program and then its arguments",
        ));
    };
    let mut command = Command::new(program);
    command
        .args(rest)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(cwd) = options.get::<Option<String>>("cwd")? {
        command.current_dir(cwd);
    }
    if let Some(env) = options.get::<Option<HashMap<String, String>>>("env")? {
        command.envs(env);
    }
    // Read before the process is started: a handler of the wrong type is told, and nothing runs.
    let on_stdout: Option<Function> = options.get("on_stdout")?;
    let on_stderr: Option<Function> = options.get("on_stderr")?;
    let on_exit: Option<Function> = options.get("on_exit")?;
    let input: Option<mlua::String> = options.get("input")?;
    let input: Vec<u8> = input
        .map(|text| text.as_bytes().to_vec())
        .unwrap_or_default();
    let mut process = command
        .spawn()
        .map_err(|why| super::fault(format!("process.spawn: {program}: {}", super::why(&why))))?;
    if let Some(mut stdin) = process.stdin.take() {
        // Written on a thread of its own: a process that does not read must not hold the loop.
        if input.is_empty() {
            drop(stdin);
        } else {
            std::thread::spawn(move || {
                use std::io::Write as _;
                let _ = stdin.write_all(&input);
            });
        }
    }
    let (Some(stdout), Some(stderr)) = (process.stdout.take(), process.stderr.take()) else {
        let _ = process.kill();
        let _ = process.wait();
        return Err(super::fault("process.spawn: no pipes"));
    };
    let stdout: OwnedFd = stdout.into();
    let stderr: OwnedFd = stderr.into();
    nonblocking(&stdout);
    nonblocking(&stderr);
    let id = core.id();
    let pid = process.id();
    let stream = |fd: OwnedFd, handler: Option<Function>| Stream {
        fd,
        buffer: Vec::new(),
        handler,
        open: true,
    };
    core.children.borrow_mut().push(Child {
        id,
        process,
        stdout: stream(stdout, on_stdout),
        stderr: stream(stderr, on_stderr),
        on_exit,
    });
    Ok(ChildHandle {
        pid,
        id,
        core: Weak::clone(&core.weak()),
    })
}

/// A process the app started.
pub struct ChildHandle {
    pid: u32,
    id: u64,
    core: Weak<Core>,
}

/// The signal a name or a number stands for.
fn signal_of(value: &mlua::Value) -> mlua::Result<libc::c_int> {
    match value {
        mlua::Value::Nil => Ok(libc::SIGTERM),
        mlua::Value::Integer(n) => Ok(*n as libc::c_int),
        mlua::Value::Number(n) => Ok(*n as libc::c_int),
        mlua::Value::String(name) => match name.to_string_lossy().trim_start_matches("SIG") {
            "TERM" => Ok(libc::SIGTERM),
            "KILL" => Ok(libc::SIGKILL),
            "INT" => Ok(libc::SIGINT),
            "HUP" => Ok(libc::SIGHUP),
            "QUIT" => Ok(libc::SIGQUIT),
            "USR1" => Ok(libc::SIGUSR1),
            "USR2" => Ok(libc::SIGUSR2),
            other => Err(super::fault(format!(
                "kill: {other:?} is not a signal this knows (TERM, KILL, INT, HUP, QUIT, USR1, USR2)"
            ))),
        },
        _ => Err(super::fault("kill takes a signal's name or number")),
    }
}

impl UserData for ChildHandle {
    fn add_fields<F: UserDataFields<Self>>(fields: &mut F) {
        fields.add_field_method_get("pid", |_, this| Ok(this.pid));
    }

    fn add_methods<M: UserDataMethods<Self>>(methods: &mut M) {
        // child:kill([signal]): asks the process to end (TERM) or makes it ("KILL").
        methods.add_method("kill", |_, this, signal: mlua::Value| {
            let signal = signal_of(&signal)?;
            // The process may be gone already: a signal to a pid that was reaped could reach
            // another process, so it is sent only to one the loop still holds.
            let alive = this.core.upgrade().is_some_and(|core| {
                core.children
                    .borrow()
                    .iter()
                    .any(|child| child.id == this.id)
            });
            if alive {
                // SAFETY: a signal to a process of ours.
                unsafe {
                    libc::kill(this.pid as libc::pid_t, signal);
                }
            }
            Ok(alive)
        });
        // child:alive(): whether the process has not ended and been reported yet.
        methods.add_method("alive", |_, this, ()| {
            Ok(this.core.upgrade().is_some_and(|core| {
                core.children
                    .borrow()
                    .iter()
                    .any(|child| child.id == this.id)
            }))
        });
    }
}
