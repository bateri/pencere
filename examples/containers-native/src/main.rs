//! `containers-native`: a made-up Docker host, and the smallest program that shows a view of its own.
//!
//! In a terminal that speaks pencere it opens the container list as a view of the terminal's
//! own (a WebAssembly module it carries, `containers.wasm`, or with `--lua` the same page as a
//! script in the Lua language, `containers.lua`) and, once a second, tells it how much CPU and
//! memory each running container uses — the view does not ask again. In a terminal that does not,
//! and when asked for text, it prints the list. ⌃C ends it and the shell is where it was. A view
//! that fails says why once the terminal is back: the error of a script, with its line.
//!
//! ```text
//! containers-native            the view if the terminal shows one, the list as text if not
//! containers-native --lua      the same, with the page as a Lua script
//! containers-native --script F the view is the Lua script in the file F (to try a script of one's own)
//! containers-native --flood    the view, with the readings pushed as fast as the terminal takes them
//! containers-native --stall N  the view, with the readings stopping after N seconds (a Docker that hangs)
//! containers-native --text     the list as text, whatever the terminal is
//! containers-native --json     the list as JSON, a line for each container
//! ```
//!
//! In the view, Shift-U sends this program a large payload; the program counts what it gets and
//! says how much when it leaves.
//!
//! The list and the numbers are the page's own (`containers-page`): this program makes up nothing
//! the page does not know the names of. The protocol half (frames, bundle, token, and the
//! messages of the view) is `pencere-wire`'s, written by the crate that reads it.

mod table;
mod terminal;
mod view;

use std::process::ExitCode;
use std::time::Duration;

/// The view's module: built by `make guest`, which builds it before this program. Empty where the
/// program was built without it (`build.rs`).
static VIEW: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/containers.wasm"));

/// The same page as a script.
const SCRIPT: &[u8] = include_bytes!("../../../kit/luau/examples/containers.lua");

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let mut flood = false;
    let mut stall = None;
    let mut lua = false;
    let mut script = None;
    let mut text = false;
    let mut json = false;
    let mut args = args.iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--flood" => flood = true,
            "--stall" => match args
                .next()
                .map(|s| s.parse::<f64>().map(Duration::try_from_secs_f64))
            {
                Some(Ok(Ok(after))) => stall = Some(after),
                _ => {
                    eprintln!("{USAGE}");
                    return ExitCode::from(64);
                }
            },
            "--lua" => lua = true,
            "--script" => match args.next().map(std::fs::read) {
                Some(Ok(bytes)) => script = Some(bytes),
                Some(Err(why)) => {
                    eprintln!("containers-native: the script could not be read: {why}");
                    return ExitCode::from(66);
                }
                None => {
                    eprintln!("{USAGE}");
                    return ExitCode::from(64);
                }
            },
            "--text" => text = true,
            "--json" => json = true,
            "--help" | "-h" => {
                println!("{USAGE}");
                return ExitCode::SUCCESS;
            }
            _ => {
                eprintln!("{USAGE}");
                return ExitCode::from(64);
            }
        }
    }
    match (text, json) {
        (true, true) => {
            eprintln!("{USAGE}");
            ExitCode::from(64)
        }
        (true, false) => {
            print!("{}", table::text());
            ExitCode::SUCCESS
        }
        (false, true) => {
            print!("{}", table::json());
            ExitCode::SUCCESS
        }
        (false, false) if script.is_some() => shown(
            view::Kind::LUA,
            script.as_deref().unwrap_or_default(),
            flood,
            stall,
        ),
        (false, false) if lua => shown(view::Kind::LUA, SCRIPT, flood, stall),
        (false, false) if VIEW.is_empty() => {
            eprintln!(
                "containers-native: this build carries no WebAssembly module (`make guest` builds it first); --lua shows the same page as a script"
            );
            ExitCode::from(69)
        }
        (false, false) => shown(view::Kind::WASM, VIEW, flood, stall),
    }
}

/// The view if the terminal shows one, and the list as text if not.
fn shown(kind: view::Kind, file: &[u8], flood: bool, stall: Option<Duration>) -> ExitCode {
    if terminal::is_terminal() {
        let (outcome, uploaded, failure) = view::run(kind, file, flood, stall);
        if uploaded > 0 {
            println!("containers-native: the view sent {uploaded} bytes");
        }
        if let Some(why) = failure {
            // The terminal is the shell's again: this is where the person can read it.
            eprintln!("containers-native: the view failed: {why}");
        }
        match outcome {
            view::Outcome::Ended => return ExitCode::SUCCESS,
            view::Outcome::Interrupted => return ExitCode::from(130),
            view::Outcome::NoView => {}
        }
    }
    print!("{}", table::text());
    ExitCode::SUCCESS
}

const USAGE: &str = "usage: containers-native [--lua | --script FILE] [--flood] [--stall N] | --text | --json\n\n  \
    (no option)  a view in a terminal that shows one, the list as text in one that does not\n  \
    --lua        the view as a Lua script, not a WebAssembly component\n  \
    --script F   the view as the Lua script in the file F\n  \
    --flood      the view, with the readings pushed as fast as the terminal takes them\n  \
    --stall N    the view, with the readings stopping after N seconds\n  \
    --text       the list as text\n  \
    --json       the list as JSON, a line for each container";
