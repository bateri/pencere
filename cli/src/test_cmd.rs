//! `pencere test`: runs the tests of the app in this directory with the runner of tests.
//!
//! The runner of tests (`pencere-test`) is a program of its own: it links the kit's page engine, its
//! editors and its interpreter, which the command, copied into every app, must not carry. This
//! finds it and becomes it (`exec`), so that its output, its status and the signals it gets are the
//! command's.

use std::os::unix::process::CommandExt as _;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitCode};

use crate::build_cmd;

/// The runner of tests that belongs to this command, by the file's name.
const RUNNER: &str = "pencere-test";

/// Where `PENCERE_TEST` names the runner of tests, instead of the one found.
const VARIABLE: &str = "PENCERE_TEST";

/// The name of the runner of tests in a release: by version and target, as the command's.
pub fn release_file(target: &str) -> String {
    format!("{RUNNER}-{}-{target}", build_cmd::VERSION)
}

/// The runner of tests to run, and where it was found, or why there is none.
fn locate() -> Result<PathBuf, String> {
    if let Some(named) = std::env::var_os(VARIABLE) {
        let path = PathBuf::from(&named);
        return if path.is_file() {
            Ok(path)
        } else {
            Err(format!(
                "{VARIABLE} names {}, which is not a file",
                path.display()
            ))
        };
    }
    // Beside this command: a checkout built with `cargo build -p pencere-cli -p pencere-test`, or
    // a directory the two were put in. It is the same program as this one only if the two were
    // built together, which a checkout does and a person who mixes them answers for.
    if let Ok(own) = std::env::current_exe()
        && let Some(directory) = own.parent()
    {
        let beside = directory.join(RUNNER);
        if beside.is_file() {
            return Ok(beside);
        }
    }
    // The release of this version, once, checked against its SHA256SUMS, and kept.
    let target = build_cmd::OWN;
    let file = release_file(target);
    match build_cmd::fetch_release_file(&file) {
        Ok(path) => Ok(path),
        Err(why) => Err(format!(
            "pencere test needs the runner of tests ({RUNNER}), which is not here: it is not beside this command, {VARIABLE} names none, and the release of this version does not give it ({why}).\n\
             In a checkout of pencere: cargo build --release -p pencere-test, and {VARIABLE}=target/release/{RUNNER}."
        )),
    }
}

/// Runs the runner of tests with `args`; it replaces this process, so what returns is a failure to.
pub fn command(args: &[String]) -> ExitCode {
    let runner = match locate() {
        Ok(runner) => runner,
        Err(why) => {
            eprintln!("pencere: {why}");
            return ExitCode::from(1);
        }
    };
    let error = Command::new(&runner).args(args).exec();
    eprintln!("pencere: {} cannot be run: {error}", display(&runner));
    ExitCode::from(1)
}

fn display(path: &Path) -> String {
    path.display().to_string()
}
