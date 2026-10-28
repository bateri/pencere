//! The `pencere` command as a library: what its binary runs ([`main`]), and the Lua runtime an app
//! runs in with the files of an app, for `pencere-test`, which runs an app's tests in that runtime.
//!
//! **This is the command's insides, not an interface:** the crate is not published, `pencere-test`
//! is the one crate that depends on it (`make audit`), and the two are released together, of one
//! version. What `pencere-test` needs is all that is public here; the rest of the command is not.
//!
//! The command is a library, and its binary three lines that call it, so that the runtime is
//! compiled in one crate with the rest: a crate of its own between them makes the runner, which is
//! copied into every app, larger by the generic code both crates would each compile.

mod app;
mod build_cmd;
mod dev;
mod dev_cargo;
mod files;
mod macho;
mod new;
mod payload;
mod runtime;
mod upgrade;

use std::process::ExitCode;

pub use dev_cargo::{build as build_package, is_project as is_package};
pub use files::{Files, Project};
/// The Lua library the runtime is made with, so that a module made for it is made with this version.
pub use mlua;
pub use runtime::{
    Config, Env, Exit, Handed, Module, Remote, Runner, Serve, Served, from_json, to_json,
};

/// What `pencere --help` says.
const HELP: &str = "\
pencere: apps that show a view in the terminal pane they run in

usage:
  pencere new <name> [--lang lua|ts|rust] [--page lua|web]
                                             make a project to start from (--page: the view of a
                                             Lua project is a page of the kit, or a web page)
  pencere dev [dir] [--web DIR] [--name NAME] [--log FILE]
                                             run the app in dir (default .) in this pane, and run
                                             it again when its files change; a Cargo package (a
                                             program in Rust) is built with Cargo, and its view is
                                             shown again without ending it
  pencere build [dir] [--web DIR] [--name NAME] [--target T] [--runner FILE] [-o FILE]
                                             make one executable of the app in dir; a Cargo
                                             package is built by Cargo (--release, --target passed
                                             on) and its program copied to dist/
  pencere upgrade [--check] [--version X.Y.Z]
                                             replace this command with the newest release of it
                                             (or X.Y.Z), checked against the release's SHA256SUMS;
                                             --check only says whether there is a newer one
  pencere --version

An app is a directory with the program in src/program/ (app.lua and its modules), the page in
src/view/ (page.lua, or index.html for a web page) and the pictures in src/assets/. Builds go to
dist/. --web packs the page from the directory a tool built it into (dist/view) instead of
src/view/; --name names the executable and the app in the messages it prints (the directory's name
unless told). A directory with an app.lua in it is an app too, the simple form: its files are packed
as they lie.

An app built by `pencere build` runs by itself:
  app [--log FILE] [--] [arguments...]       --log sends `print` and errors to FILE, because the
                                             terminal is under the view while it is up

targets: aarch64-apple-darwin x86_64-apple-darwin aarch64-unknown-linux-musl x86_64-unknown-linux-musl
";

/// What the binary does: run the app inside it, or, with none, be the command.
pub fn main() -> ExitCode {
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
        "upgrade" => upgrade::command(&rest),
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
