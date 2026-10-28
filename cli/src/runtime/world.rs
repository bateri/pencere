//! What an app finds around it: the variables and the directory it runs in, and whether there is a
//! terminal. An app that is the program has the process's own; an app that is one of several in a
//! process (a test's, and the program it tests) has an environment of its own, so that none of them
//! changes what the others find: the process's variables and directory are never written.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

/// The environment an app runs in when it is not the process's.
///
/// With one, the app has no terminal either: its standard input is empty, `io.is_terminal` says no,
/// and the programs it runs get the variables and the directory below, not the process's.
#[derive(Debug, Clone, Default)]
pub struct Env {
    /// Every variable the app finds, and the programs it runs.
    pub vars: BTreeMap<String, String>,
    /// The directory its relative paths are in, and the programs it runs start in.
    pub cwd: PathBuf,
}

impl Env {
    /// The process's own environment and directory, as they are now: what a layer is laid over.
    pub fn of_process() -> std::io::Result<Self> {
        Ok(Self {
            // A variable that is not text is not one an app can read.
            vars: std::env::vars_os()
                .filter_map(|(name, value)| {
                    Some((name.into_string().ok()?, value.into_string().ok()?))
                })
                .collect(),
            cwd: std::env::current_dir()?,
        })
    }
}

/// The environment of an app, as the runtime reads it.
#[derive(Debug, Default)]
pub struct World {
    env: Option<Env>,
}

impl World {
    pub fn new(env: Option<Env>) -> Self {
        Self { env }
    }

    /// Whether the app is the process: it has the process's terminal, its input and its signals.
    pub fn is_process(&self) -> bool {
        self.env.is_none()
    }

    /// `path` as the app means it: a relative one is in the app's directory.
    pub fn path(&self, path: &str) -> PathBuf {
        match &self.env {
            Some(env) if !Path::new(path).is_absolute() => env.cwd.join(path),
            _ => PathBuf::from(path),
        }
    }

    pub fn var(&self, name: &str) -> Option<String> {
        match &self.env {
            Some(env) => env.vars.get(name).cloned(),
            None => std::env::var(name).ok(),
        }
    }

    pub fn vars(&self) -> Vec<(String, String)> {
        match &self.env {
            Some(env) => env
                .vars
                .iter()
                .map(|(name, value)| (name.clone(), value.clone()))
                .collect(),
            None => std::env::vars().collect(),
        }
    }

    pub fn cwd(&self) -> std::io::Result<PathBuf> {
        match &self.env {
            Some(env) => Ok(env.cwd.clone()),
            None => std::env::current_dir(),
        }
    }

    /// A command of this environment: the app's variables and directory, not the process's.
    pub fn command(&self, program: &str) -> Command {
        let Some(env) = &self.env else {
            return Command::new(program);
        };
        // A relative path to a program is found from the directory it starts in, as for a process.
        let mut command = Command::new(program);
        command.env_clear().envs(&env.vars).current_dir(&env.cwd);
        command
    }
}
