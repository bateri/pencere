//! `pencere-test`: the runner of an app's tests.
//!
//! A test is Luau that opens the page of an app under the host with no screen — the page engine,
//! its editors and the kit's interpreter, with a clock the test moves — with the program the app
//! has, and does to it what a person does, looking at what it shows. `pencere test` finds this
//! program and runs it; it is a program of its own because it links the kit, which the command that
//! is copied into every app must not.
//!
//! One process runs the files of the app's `test/` on workers, processes of this same program
//! (`--worker`), so that a test stuck in a call or ended by a crash costs its file and not the run.

mod file;
mod page;
mod peer;
mod program;
mod report;
mod run;
mod worker;

use std::path::PathBuf;
use std::process::ExitCode;

use run::{Format, Options};

const HELP: &str = "\
pencere-test: run the tests of a pencere app

usage:
  pencere-test [FILE[:LINE] | DIR]... [--grep TEXT] [--jobs N] [--timeout SECONDS]
               [--reporter pretty|tap] [--junit FILE] [--artifacts DIR] [--root DIR]

Runs the *_test.lua files of the app's test/ (the app is the directory with src/program/app.lua, here
or above), or the files and directories given; FILE:LINE runs the test that starts on that line.
  --grep TEXT        only the tests whose name has TEXT in it
  --jobs N           files run at once (default: the cores, at most 8)
  --timeout SECONDS  what a test is given (default 30)
  --reporter R       pretty (default) or tap
  --junit FILE       also write a JUnit report
  --artifacts DIR    where a failed test's pictures and notes are kept (default .pencere/test)
  --root DIR         the app, if it is not here
Exit status: 0 every test passed, 1 a test failed, a file could not run or no test ran, 64 usage.
";

fn usage(why: &str) -> ExitCode {
    eprintln!("pencere-test: {why}\n\n{HELP}");
    ExitCode::from(64)
}

fn parse(args: Vec<String>) -> Result<Options, String> {
    let mut options = Options {
        paths: Vec::new(),
        grep: None,
        jobs: None,
        timeout: 30.0,
        format: Format::Pretty,
        junit: None,
        artifacts: None,
        root: None,
    };
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        let (name, inline) = match arg.split_once('=') {
            Some((name, value)) if name.starts_with("--") => {
                (name.to_owned(), Some(value.to_owned()))
            }
            _ => (arg.clone(), None),
        };
        let mut value = |what: &str| -> Result<String, String> {
            inline
                .clone()
                .or_else(|| args.next())
                .ok_or_else(|| format!("{name} takes {what}"))
        };
        match name.as_str() {
            "--grep" => options.grep = Some(value("the text a test's name has in it")?),
            "--jobs" => {
                let jobs: usize = value("a number")?
                    .parse()
                    .map_err(|_| "--jobs takes a number of files, 1 or more".to_owned())?;
                if jobs == 0 {
                    return Err("--jobs takes a number of files, 1 or more".to_owned());
                }
                options.jobs = Some(jobs);
            }
            "--timeout" => {
                options.timeout = value("seconds")?
                    .parse()
                    .ok()
                    .filter(|seconds: &f64| seconds.is_finite() && *seconds > 0.0)
                    .ok_or_else(|| "--timeout takes seconds, more than 0".to_owned())?;
            }
            "--reporter" => {
                options.format = match value("pretty or tap")?.as_str() {
                    "pretty" => Format::Pretty,
                    "tap" => Format::Tap,
                    other => return Err(format!("--reporter is pretty or tap, not {other:?}")),
                };
            }
            "--junit" => options.junit = Some(PathBuf::from(value("a file")?)),
            "--artifacts" => options.artifacts = Some(PathBuf::from(value("a directory")?)),
            "--root" => options.root = Some(PathBuf::from(value("a directory")?)),
            flag if flag.starts_with("--") => return Err(format!("there is no flag {flag}")),
            _ => options.paths.push(arg),
        }
    }
    Ok(options)
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("--worker") => return worker::main(),
        Some("--help" | "-h") => {
            print!("{HELP}");
            return ExitCode::SUCCESS;
        }
        Some("--version") => {
            println!("pencere-test {}", env!("CARGO_PKG_VERSION"));
            return ExitCode::SUCCESS;
        }
        _ => {}
    }
    let options = match parse(args) {
        Ok(options) => options,
        Err(why) => return usage(&why),
    };
    let exe = match std::env::current_exe() {
        Ok(exe) => exe,
        Err(error) => {
            eprintln!("pencere-test: {error}");
            return ExitCode::from(1);
        }
    };
    run::run(&options, &exe)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn flags_and_paths_are_told_apart_and_a_flag_that_does_not_exist_is_refused() {
        let options = parse(strings(&[
            "test/a_test.lua:7",
            "--grep=clock",
            "--jobs",
            "3",
            "--timeout",
            "2.5",
            "--reporter",
            "tap",
            "--junit",
            "out.xml",
        ]))
        .unwrap_or_else(|why| unreachable!("{why}"));
        assert_eq!(options.paths, ["test/a_test.lua:7"]);
        assert_eq!(options.grep.as_deref(), Some("clock"));
        assert_eq!(options.jobs, Some(3));
        assert!((options.timeout - 2.5).abs() < f64::EPSILON);
        assert_eq!(options.format, Format::Tap);
        assert_eq!(options.junit, Some(PathBuf::from("out.xml")));
        assert!(parse(strings(&["--nope"])).is_err());
        assert!(parse(strings(&["--jobs", "0"])).is_err());
        assert!(parse(strings(&["--timeout"])).is_err());
        assert!(parse(strings(&["--reporter", "xml"])).is_err());
    }
}
