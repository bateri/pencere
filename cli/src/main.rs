//! `pencere`: the command that makes apps written in Lua, and the runner a built app is.
//!
//! A binary with an app inside it runs that app. The same binary with none is the command: `new`,
//! `dev` and `build`.

mod app;
mod build_cmd;
mod dev;
mod files;
mod macho;
mod new;
mod payload;
mod runtime;

use std::process::ExitCode;

/// What `pencere --help` says.
const HELP: &str = "\
pencere: apps that show a view in the terminal pane they run in

usage:
  pencere new <name> [--lang lua|ts|rust] [--page lua|web]
                                             make a project to start from (--page: the view of a
                                             Lua project is a page of the kit, or a web page)
  pencere dev [dir] [--log FILE]             run the Lua app in dir (default .) in this pane, and
                                             run it again when its files change
  pencere build [dir] [--target T] [--runner FILE] [-o FILE]
                                             make one executable of the Lua app in dir
  pencere --version

An app built by `pencere build` runs by itself:
  app [--log FILE] [--] [arguments...]       --log sends `print` and errors to FILE, because the
                                             terminal is under the view while it is up

targets: aarch64-apple-darwin x86_64-apple-darwin aarch64-unknown-linux-musl x86_64-unknown-linux-musl
";

fn main() -> ExitCode {
    if let Some(payload) = payload::own() {
        return app::run_built(payload, std::env::args().skip(1).collect());
    }
    command(std::env::args().skip(1).collect())
}

fn command(args: Vec<String>) -> ExitCode {
    let mut args = args.into_iter();
    let Some(name) = args.next() else {
        print!("{HELP}");
        return ExitCode::from(64);
    };
    let rest: Vec<String> = args.collect();
    match name.as_str() {
        "new" => new::command(&rest),
        "dev" => dev::command(&rest),
        "build" => build_cmd::command(&rest),
        "--version" | "-V" | "version" => {
            println!("pencere {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        "--help" | "-h" | "help" => {
            print!("{HELP}");
            ExitCode::SUCCESS
        }
        other => {
            eprintln!("pencere: {other:?} is not a command\n\n{HELP}");
            ExitCode::from(64)
        }
    }
}

/// A command line problem: what is wrong, then the status for it.
pub fn usage(why: impl std::fmt::Display) -> ExitCode {
    eprintln!("pencere: {why}\n\n{HELP}");
    ExitCode::from(64)
}
