//! `pencere`: the command that makes apps written in Lua, and the runner a built app is.
//!
//! A binary with an app inside it runs that app. The same binary with none is the command: `new`,
//! `dev` and `build`. All of it is the crate's library (`src/lib.rs`).

fn main() -> std::process::ExitCode {
    pencere_cli::main()
}
