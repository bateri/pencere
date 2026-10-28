//! A built app, run: the files inside the executable, the arguments it was given, and the loop.

use std::path::PathBuf;
use std::process::ExitCode;

use crate::files::Files;
use crate::runtime::{Config, Exit, Runner};

/// The runner's own flags are read before the app's arguments: `--log FILE`, and `--` to end them.
/// What comes after is the app's.
pub struct Split {
    pub log: Option<PathBuf>,
    pub args: Vec<String>,
}

pub fn split(args: Vec<String>) -> Result<Split, String> {
    let mut log = None;
    let mut rest = Vec::new();
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--" => {
                rest.extend(args);
                break;
            }
            "--log" => match args.next() {
                Some(file) => log = Some(PathBuf::from(file)),
                None => return Err("--log needs the file to write to".into()),
            },
            other => match other.strip_prefix("--log=") {
                Some(file) => log = Some(PathBuf::from(file)),
                None => rest.push(arg),
            },
        }
    }
    Ok(Split { log, args: rest })
}

/// The status to leave with: that of the app, which is 130 after ⌃C.
pub fn status_of(exit: &Exit, log_is_file: bool) -> ExitCode {
    match exit {
        Exit::Done(code) => ExitCode::from(u8::try_from(*code & 0xff).unwrap_or(1)),
        Exit::Failed(why) => {
            // Without a log the report was printed where it happened.
            if log_is_file {
                eprintln!("{why}");
            }
            ExitCode::from(1)
        }
        Exit::Restart => ExitCode::SUCCESS,
    }
}

pub fn run_built(payload: &[u8], args: Vec<String>) -> ExitCode {
    let (meta, files) = match Files::unpack(payload) {
        Ok(app) => app,
        Err(why) => {
            eprintln!("pencere: {why}");
            return ExitCode::from(70);
        }
    };
    let split = match split(args) {
        Ok(split) => split,
        Err(why) => {
            eprintln!("{}: {why}", meta.name);
            return ExitCode::from(64);
        }
    };
    let log_is_file = split.log.is_some();
    let config = Config::process(files, meta.name, split.args, split.log);
    let runner = match Runner::new(config) {
        Ok(runner) => runner,
        Err(why) => {
            eprintln!("pencere: {why}");
            return ExitCode::from(70);
        }
    };
    status_of(&runner.run(), log_is_file)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn the_runners_flags_come_before_the_apps_arguments() {
        let split = split(strings(&[
            "--log",
            "/tmp/a.log",
            "one",
            "--flag",
            "--",
            "--log",
            "x",
        ]))
        .expect("arguments");
        assert_eq!(split.log, Some(PathBuf::from("/tmp/a.log")));
        assert_eq!(split.args, ["one", "--flag", "--log", "x"]);
        let split = super::split(strings(&["--log=/tmp/b.log"])).expect("arguments");
        assert_eq!(split.log, Some(PathBuf::from("/tmp/b.log")));
        assert!(super::split(strings(&["--log"])).is_err());
    }
}
